//! 物理入力 → デバイス判別 → 設定解決 → 操作 までを、OS の入力 backend なしで確認する。
//!
//! 各テストは [`Pipeline`] へ backend と同じ [`ControlInputEvent`] を入れ、押下集合の追跡、
//! [`resolve_input`] による解決、画面側の Action 決定までを通す。

use bmz_core::lane::{KeyMode, Lane};
use bmz_gameplay::input::backend::{DeviceId, DeviceTimestamp, PhysicalControl};
use winit::event::ElementState;
use winit::keyboard::{KeyCode, PhysicalKey};

use super::*;
use crate::app::input_runtime::{AppInputRuntime, ControlInputEvent};
use crate::app::integration_support::{TargetCycle, target_cycle_from_input};
use crate::app::scene_input::{
    DecideAction, ResultAction, SelectAction, SelectMove, decide_action, result_action,
    select_action,
};
use crate::config::key_config::{
    ActionControllerBinding, KeyBindingGroup, KeyBindingSlot, KeyBindingTarget,
    action_controller_binding, apply_play_binding, clear_play_binding, format_play_binding,
    restore_action_group_defaults,
};
use crate::config::profile_config::{
    InputActionConfig, LaneConfig, ProfileConfig, ProfileInputConfig, SelectInputModeConfig,
};
use crate::input::gamepad::{GamepadPressedButton, GamepadSlotMap};

const LEFT: DeviceId = DeviceId(0x1001);
const RIGHT: DeviceId = DeviceId(0x1002);
const SPARE: DeviceId = DeviceId(0x1003);

fn profile() -> ProfileConfig {
    ProfileConfig::new_default("input-test", "Input Test", 0)
}

fn both_slots() -> GamepadSlotMap {
    GamepadSlotMap::from_device_ids([Some(LEFT), Some(RIGHT)])
}

fn set_key(
    input: &mut ProfileInputConfig,
    mode: KeyMode,
    lane: LaneConfig,
    slot: KeyBindingSlot,
    control: &str,
) {
    apply_play_binding(input, mode, KeyBindingTarget::Key { lane, slot }, control).unwrap();
}

fn set_action(
    input: &mut ProfileInputConfig,
    action: InputActionConfig,
    slot: KeyBindingSlot,
    control: &str,
) {
    apply_play_binding(input, KeyMode::K7, KeyBindingTarget::Action { action, slot }, control)
        .unwrap();
}

fn clear_action(input: &mut ProfileInputConfig, action: InputActionConfig, slot: KeyBindingSlot) {
    clear_play_binding(input, KeyMode::K7, KeyBindingTarget::Action { action, slot }).unwrap();
}

fn pad(device: DeviceId, button: &str, pressed: bool) -> ControlInputEvent {
    ControlInputEvent::gamepad(device, button, pressed)
}

fn analog(device: DeviceId, button: &str, pressed: bool) -> ControlInputEvent {
    ControlInputEvent {
        synthesized_analog_axis: true,
        timestamp: DeviceTimestamp::MonotonicNs(1),
        ..ControlInputEvent::gamepad(device, button, pressed)
    }
}

fn key(code: KeyCode, pressed: bool) -> ControlInputEvent {
    ControlInputEvent::keyboard_parts(
        PhysicalKey::Code(code),
        if pressed { ElementState::Pressed } else { ElementState::Released },
        false,
    )
}

/// app の入力経路から OS の backend と画面描画を除いたもの。
struct Pipeline {
    input: ProfileInputConfig,
    runtime: AppInputRuntime,
    scene: SceneInputResolver,
    play: Option<crate::app::PlayOptionInput>,
    slots: GamepadSlotMap,
}

impl Pipeline {
    fn new(input: ProfileInputConfig, slots: GamepadSlotMap) -> Self {
        let scene = SceneInputResolver::from_profile(&input);
        Self { input, runtime: AppInputRuntime::default(), scene, play: None, slots }
    }

    /// プレイセッションを始める。`mode` は現在の設定対象モード。
    fn start_play(&mut self, mode: KeyMode) {
        self.play = Some(crate::app::PlayOptionInput::new(
            mode,
            crate::config::play::lane_binding_for_chart_with_slots(&self.input, mode, self.slots),
            &self.input,
            self.slots,
        ));
    }

    /// 設定を変えたときと同じく resolver を作り直す。
    fn reload_config(&mut self, input: ProfileInputConfig) {
        self.input = input;
        self.scene = SceneInputResolver::from_profile(&self.input);
    }

    /// プレー終了。選曲へ戻る。
    fn end_play(&mut self) {
        self.play = None;
    }

    /// app と同じく、進行中のプレーは開始時の割り当て、それ以外は実行時の割り当て。
    fn input_slots(&self) -> GamepadSlotMap {
        session_input_slots(self.play.as_ref(), self.slots)
    }

    /// backend からの1イベント。押下集合を更新してから解決する (app と同じ順)。
    fn event(&mut self, event: ControlInputEvent) -> ResolvedInput {
        self.runtime.track_control(&event);
        let slots = self.input_slots();
        resolve_input(&self.scene, self.play.as_ref(), slots, event)
    }

    fn press(&mut self, event: ControlInputEvent) -> ResolvedControl {
        *self.event(event).scene()
    }

    fn axis(&self, device: DeviceId, axis: &str, ticks: i32) -> ResolvedAxisTick {
        resolve_axis_tick(&self.scene, self.play.as_ref(), self.input_slots(), device, axis, ticks)
    }

    /// 押下中の入力元をそれぞれ解決して集約した E1 / E2 / E1〜E4。
    fn holds(&self) -> (bool, bool, Vec<InputActionConfig>) {
        let pressed = self.runtime.pressed_sources().map(|(device, control)| {
            self.scene.resolve_control(device, control, self.input_slots())
        });
        let (e1, e2, actions) = crate::app::select_hold_state_from_resolved(pressed);
        let mut actions: Vec<_> = actions.into_iter().collect();
        actions.sort_by_key(|action| format!("{action:?}"));
        (e1, e2, actions)
    }
}

// ---------------------------------------------------------------------------
// 1P / 2P の設定が異なる 14K コントローラー (不具合 A)
// ---------------------------------------------------------------------------

#[test]
fn swapped_2p_key1_key2_only_changes_the_right_controller() {
    let mut profile = profile();
    set_key(
        &mut profile.input,
        KeyMode::K14,
        LaneConfig::Key8,
        KeyBindingSlot::Controller2P,
        "Button2",
    );
    set_key(
        &mut profile.input,
        KeyMode::K14,
        LaneConfig::Key9,
        KeyBindingSlot::Controller2P,
        "Button1",
    );
    let mut pipeline = Pipeline::new(profile.input, both_slots());

    // 1P は 7K 設定のまま Button1 = 決定、Button2 = 戻る。
    assert_eq!(
        select_action(&pipeline.event(pad(LEFT, "Button1", true))),
        Some(SelectAction::EnterOrPlay)
    );
    assert_eq!(
        select_action(&pipeline.event(pad(LEFT, "Button2", true))),
        Some(SelectAction::ExitFolder)
    );
    // 2P は自分の 14K 設定で Button1 = 2P KEY2 = 戻る、Button2 = 2P KEY1 = 決定。
    assert_eq!(
        select_action(&pipeline.event(pad(RIGHT, "Button1", true))),
        Some(SelectAction::ExitFolder)
    );
    assert_eq!(
        select_action(&pipeline.event(pad(RIGHT, "Button2", true))),
        Some(SelectAction::EnterOrPlay)
    );
    // 設定画面と決定画面も同じ解決結果を使う。
    let right_button1 = pipeline.press(pad(RIGHT, "Button1", true));
    assert!(right_button1.settings_input().back && !right_button1.settings_input().confirm);
    let decide = pipeline.event(pad(RIGHT, "Button2", true));
    assert_eq!(decide_action(&decide), Some(DecideAction::Confirm));
    assert_eq!(decide_action(&pipeline.event(pad(RIGHT, "Button1", true))), None);
    // PLAY OPTION の配置変更は 2P 側の向きで扱う。
    assert_eq!(
        pipeline.press(pad(RIGHT, "Button2", true)).arrange_input(),
        Some((PlayerSide::P2, true))
    );
    assert_eq!(
        pipeline.press(pad(RIGHT, "Button1", true)).arrange_input(),
        Some((PlayerSide::P2, false))
    );
    assert_eq!(
        pipeline.press(pad(LEFT, "Button1", true)).arrange_input(),
        Some((PlayerSide::P1, true))
    );
}

#[test]
fn swapped_2p_key1_key3_changes_gauge_not_1p_random() {
    let mut profile = profile();
    set_key(
        &mut profile.input,
        KeyMode::K14,
        LaneConfig::Key10,
        KeyBindingSlot::Controller2P,
        "Button1",
    );
    set_key(
        &mut profile.input,
        KeyMode::K14,
        LaneConfig::Key8,
        KeyBindingSlot::Controller2P,
        "Button3",
    );
    let mut pipeline = Pipeline::new(profile.input, both_slots());

    let right = pipeline.press(pad(RIGHT, "Button1", true));
    assert_eq!(right.ui_key(), Some(3), "2P Button1 は 2P KEY3 (GAUGE)");
    assert_eq!(right.arrange_input(), None, "1P RANDOM へ流れない");
    assert_eq!(right.option_lane(), Some(Lane::Key10));
    let right_key1 = pipeline.press(pad(RIGHT, "Button3", true));
    assert_eq!(right_key1.arrange_input(), Some((PlayerSide::P2, true)));
    let left = pipeline.press(pad(LEFT, "Button1", true));
    assert_eq!(left.arrange_input(), Some((PlayerSide::P1, true)));
    assert_eq!(left.option_lane(), Some(Lane::Key1));
}

#[test]
fn different_button_numbers_per_side_never_fall_back_to_the_other_side() {
    let mut profile = profile();
    // 2P の KEY1 を 1P と別の番号にする。2P の Button1 は 14K 2P 設定に存在しない。
    set_key(
        &mut profile.input,
        KeyMode::K14,
        LaneConfig::Key8,
        KeyBindingSlot::Controller2P,
        "Button21",
    );
    let mut pipeline = Pipeline::new(profile.input, both_slots());

    assert_eq!(pipeline.press(pad(RIGHT, "Button21", true)).ui_key(), Some(1));
    assert_eq!(
        pipeline.press(pad(RIGHT, "Button21", true)).arrange_input(),
        Some((PlayerSide::P2, true))
    );
    // 1P は同じ番号の 2P 設定を候補にしない。
    assert_eq!(pipeline.press(pad(LEFT, "Button21", true)).ui_key(), None);
    // 14K の 2P KEY1 を明示したので、7K の KEY1 (Button1) は 2P KEY1 として使わない。
    let right_button1 = pipeline.press(pad(RIGHT, "Button1", true));
    assert_eq!(right_button1.ui_key(), None);
    assert_eq!(right_button1.arrange_input(), None);
    assert!(!right_button1.is_enter());
}

#[test]
fn physically_swapped_controllers_follow_slot_assignment() {
    let mut profile = profile();
    set_key(
        &mut profile.input,
        KeyMode::K14,
        LaneConfig::Key8,
        KeyBindingSlot::Controller2P,
        "Button2",
    );
    set_key(
        &mut profile.input,
        KeyMode::K14,
        LaneConfig::Key9,
        KeyBindingSlot::Controller2P,
        "Button1",
    );
    // 物理的に左右を入れ替えた: RIGHT が 1P、LEFT が 2P。
    let mut pipeline =
        Pipeline::new(profile.input, GamepadSlotMap::from_device_ids([Some(RIGHT), Some(LEFT)]));
    assert!(pipeline.press(pad(RIGHT, "Button1", true)).is_enter());
    assert!(pipeline.press(pad(LEFT, "Button1", true)).is_back());
    assert_eq!(
        pipeline.press(pad(LEFT, "Button2", true)).arrange_input(),
        Some((PlayerSide::P2, true))
    );
}

#[test]
fn unresolved_slot_uses_only_common_bindings() {
    let mut profile = profile();
    set_key(
        &mut profile.input,
        KeyMode::K14,
        LaneConfig::Key8,
        KeyBindingSlot::Controller2P,
        "Button21",
    );
    // 2P が未接続 (未解決)。SPARE はどの側にも割り当たっていない。
    let mut pipeline =
        Pipeline::new(profile.input, GamepadSlotMap::from_device_ids([Some(LEFT), None]));
    let spare = pipeline.press(pad(SPARE, "Button21", true));
    assert_eq!(spare.slot, None);
    assert_eq!(spare.ui_key(), None, "未解決スロットへ 2P 専用 binding を割り当てない");
    // 共通 (7K) の binding は未割り当て機器でも使える。
    let common = pipeline.press(pad(SPARE, "Button1", true));
    assert!(common.is_enter());
    assert_eq!(common.arrange_input(), Some((PlayerSide::P1, true)));
    // 1P だけ接続されていても 1P は通常どおり。
    assert!(pipeline.press(pad(LEFT, "Button1", true)).is_enter());
}

#[test]
fn unassigned_slot_is_not_filled_with_a_default_device() {
    let slots = GamepadSlotMap::from_device_ids([None, Some(RIGHT)]);
    assert_eq!(slots.device_id_for_player(1), None);
    assert_eq!(slots.player_for_device(RIGHT), Some(2));
    assert_eq!(
        controller_slot(RIGHT, &PhysicalControl::GamepadButton("Button1".into()), slots),
        Some(PlayerSide::P2)
    );
    assert_eq!(
        controller_slot(LEFT, &PhysicalControl::GamepadButton("Button1".into()), slots),
        None
    );
}

// ---------------------------------------------------------------------------
// SCR とアナログ入力 (不具合 B)
// ---------------------------------------------------------------------------

#[test]
fn same_axis_with_opposite_direction_per_side_scrolls_independently() {
    // 既定の 14K は 1P Axis1+ = Up、2P Axis1+ = Down。7K 共通は Axis1+ = Up。
    let pipeline = Pipeline::new(profile().input, both_slots());
    assert_eq!(pipeline.axis(LEFT, "Axis1", 2).scene_scroll, Some(-2));
    assert_eq!(pipeline.axis(RIGHT, "Axis1", 2).scene_scroll, Some(2));
    assert_eq!(pipeline.axis(LEFT, "Axis1", -3).scene_scroll, Some(3));
    assert_eq!(pipeline.axis(RIGHT, "Axis1", -3).scene_scroll, Some(-3));
    // 合算前に tick ごとに解決するので、同時に回しても打ち消し合いは入力元ごとに決まる。
    let total: i32 = [(LEFT, 2), (RIGHT, 2)]
        .into_iter()
        .filter_map(|(device, ticks)| pipeline.axis(device, "Axis1", ticks).scene_scroll)
        .sum();
    assert_eq!(total, 0);
}

#[test]
fn seven_and_fourteen_key_scratch_on_different_axes() {
    let mut profile = profile();
    {
        let (lane, slot, up, down) =
            (LaneConfig::Scratch2, KeyBindingSlot::Controller2P, "Axis2-", "Axis2+");
        apply_play_binding(
            &mut profile.input,
            KeyMode::K14,
            KeyBindingTarget::Scratch {
                lane,
                direction: crate::config::key_config::ScratchDirection::Up,
                slot,
            },
            up,
        )
        .unwrap();
        apply_play_binding(
            &mut profile.input,
            KeyMode::K14,
            KeyBindingTarget::Scratch {
                lane,
                direction: crate::config::key_config::ScratchDirection::Down,
                slot,
            },
            down,
        )
        .unwrap();
    }
    let pipeline = Pipeline::new(profile.input.clone(), both_slots());
    assert_eq!(pipeline.axis(RIGHT, "Axis2", 1).scene_scroll, Some(1));
    assert_eq!(pipeline.axis(RIGHT, "Axis2", -1).scene_scroll, Some(-1));
    // 1P の Axis2 はどこにも割り当てていないので何もしない。
    assert_eq!(pipeline.axis(LEFT, "Axis2", 4).scene_scroll, None);
    assert_eq!(pipeline.axis(LEFT, "Axis1", 1).scene_scroll, Some(-1));
    // 14K の 2P SCR を明示したので、7K の SCR (Axis1) は 2P では使わない。
    assert_eq!(pipeline.axis(RIGHT, "Axis1", 1).scene_scroll, None);
}

#[test]
fn unbound_axis_does_nothing() {
    let pipeline = Pipeline::new(profile().input, both_slots());
    assert_eq!(
        pipeline.axis(LEFT, "Axis5", 3),
        ResolvedAxisTick { scene_scroll: None, play_scroll: None }
    );
    assert_eq!(
        pipeline.axis(LEFT, "Axis1", 0),
        ResolvedAxisTick { scene_scroll: None, play_scroll: None }
    );
}

#[test]
fn synthesized_analog_press_does_not_double_scroll_but_digital_axis_moves_once() {
    let mut pipeline = Pipeline::new(profile().input, both_slots());
    // 合成 Press は tick 側でスクロールするので単発の移動にしない。
    let synthesized = pipeline.event(analog(LEFT, "Axis1+", true));
    assert!(synthesized.uses_analog_scroll());
    assert_eq!(select_action(&synthesized), None);
    pipeline.event(analog(LEFT, "Axis1+", false));
    // デジタル化した軸 (合成ではない Press) は1回だけ移動する。
    let digital = pipeline.event(pad(LEFT, "Axis1+", true));
    assert_eq!(select_action(&digital), Some(SelectAction::Move(SelectMove::Previous)));
    assert_eq!(select_action(&pipeline.event(pad(LEFT, "Axis1+", false))), None);
}

#[test]
fn play_option_scroll_uses_current_play_config_not_select_keys() {
    let mut profile = profile();
    // 選曲用 7K では Axis3 を使わず、14K の 2P スクラッチだけに割り当てる。
    for (direction, control) in [
        (crate::config::key_config::ScratchDirection::Up, "Axis3-"),
        (crate::config::key_config::ScratchDirection::Down, "Axis3+"),
    ] {
        apply_play_binding(
            &mut profile.input,
            KeyMode::K14,
            KeyBindingTarget::Scratch {
                lane: LaneConfig::Scratch2,
                direction,
                slot: KeyBindingSlot::Controller2P,
            },
            control,
        )
        .unwrap();
    }
    let mut pipeline = Pipeline::new(profile.input, both_slots());
    pipeline.start_play(KeyMode::K14);
    assert_eq!(pipeline.axis(RIGHT, "Axis3", 2).play_scroll, Some(2));
    assert_eq!(pipeline.axis(RIGHT, "Axis3", -2).play_scroll, Some(-2));
    assert_eq!(pipeline.axis(LEFT, "Axis3", 2).play_scroll, None);
    // 7K プレイでは 14K の 2P 設定を使わない。
    pipeline.start_play(KeyMode::K7);
    assert_eq!(pipeline.axis(RIGHT, "Axis3", 2).play_scroll, None);

    // E2 + SCR (デジタル) もプレイ設定で解決する。
    pipeline.start_play(KeyMode::K14);
    let input = pipeline.event(pad(RIGHT, "Axis3-", true));
    let action = crate::app::play_option_control_for_input(
        &input,
        false,
        true,
        pipeline.play.as_ref(),
        &pipeline.input,
    );
    assert_eq!(
        action,
        Some(crate::app::PlayOptionControl::GreenNumber(crate::app::GreenNumberChange::Up))
    );
}

// ---------------------------------------------------------------------------
// E1〜E4 の共通・1P/2P 個別設定
// ---------------------------------------------------------------------------

#[test]
fn e_actions_default_to_common_controller_binding() {
    let mut pipeline = Pipeline::new(profile().input, both_slots());
    assert!(pipeline.press(pad(LEFT, "Button9", true)).is_start());
    assert!(pipeline.press(pad(RIGHT, "Button9", true)).is_start());
    assert!(pipeline.press(pad(SPARE, "Button9", true)).is_start());
}

#[test]
fn per_side_e_action_overrides_common_and_keeps_other_side() {
    let mut profile = profile();
    set_action(&mut profile.input, InputActionConfig::E1, KeyBindingSlot::Controller2P, "Button13");
    assert_eq!(
        action_controller_binding(
            &profile.input,
            InputActionConfig::E1,
            KeyBindingSlot::Controller1P
        ),
        ActionControllerBinding::Inherit(vec!["Button9".to_string()])
    );
    assert_eq!(
        action_controller_binding(
            &profile.input,
            InputActionConfig::E1,
            KeyBindingSlot::Controller2P
        ),
        ActionControllerBinding::Explicit(vec!["Button13".to_string()])
    );
    let mut pipeline = Pipeline::new(profile.input.clone(), both_slots());
    assert!(pipeline.press(pad(LEFT, "Button9", true)).is_start());
    assert!(!pipeline.press(pad(RIGHT, "Button9", true)).is_start());
    assert!(pipeline.press(pad(RIGHT, "Button13", true)).is_start());
    assert!(!pipeline.press(pad(LEFT, "Button13", true)).is_start());

    // 1P を明示的に未割り当てにしても 2P と共通設定は消えない。
    clear_action(&mut profile.input, InputActionConfig::E1, KeyBindingSlot::Controller1P);
    assert_eq!(
        action_controller_binding(
            &profile.input,
            InputActionConfig::E1,
            KeyBindingSlot::Controller1P
        ),
        ActionControllerBinding::Unassigned
    );
    assert_eq!(
        action_controller_binding(
            &profile.input,
            InputActionConfig::E1,
            KeyBindingSlot::Controller
        ),
        ActionControllerBinding::Explicit(vec!["Button9".to_string()])
    );
    pipeline.reload_config(profile.input.clone());
    assert!(!pipeline.press(pad(LEFT, "Button9", true)).is_start());
    assert!(pipeline.press(pad(RIGHT, "Button13", true)).is_start());
    assert!(pipeline.press(pad(SPARE, "Button9", true)).is_start(), "未割り当て機器は共通設定");

    // 共通設定を変えても個別設定の側へは影響しない。
    set_action(&mut profile.input, InputActionConfig::E1, KeyBindingSlot::Controller, "Button15");
    pipeline.reload_config(profile.input.clone());
    assert!(!pipeline.press(pad(LEFT, "Button15", true)).is_start());
    assert!(!pipeline.press(pad(RIGHT, "Button15", true)).is_start());
    assert!(pipeline.press(pad(RIGHT, "Button13", true)).is_start());

    // 1P を既定へ戻すと継承に戻る。
    restore_action_group_defaults(
        &mut profile.input,
        KeyBindingGroup::Common,
        KeyBindingSlot::Controller1P,
    );
    pipeline.reload_config(profile.input.clone());
    assert!(pipeline.press(pad(LEFT, "Button15", true)).is_start());
    assert!(pipeline.press(pad(RIGHT, "Button13", true)).is_start());
}

#[test]
fn per_side_e_actions_survive_save_and_reload() {
    let mut profile = profile();
    set_action(&mut profile.input, InputActionConfig::E2, KeyBindingSlot::Controller1P, "Button14");
    clear_action(&mut profile.input, InputActionConfig::E3, KeyBindingSlot::Controller2P);
    let saved = toml::to_string(&profile).unwrap();
    let mut loaded: ProfileConfig = toml::from_str(&saved).unwrap();
    crate::config::play_input::normalize_profile_input(&mut loaded.input);

    for (action, slot, expected) in [
        (
            InputActionConfig::E2,
            KeyBindingSlot::Controller1P,
            ActionControllerBinding::Explicit(vec!["Button14".into()]),
        ),
        (
            InputActionConfig::E2,
            KeyBindingSlot::Controller2P,
            ActionControllerBinding::Inherit(vec!["Button10".into()]),
        ),
        (InputActionConfig::E3, KeyBindingSlot::Controller2P, ActionControllerBinding::Unassigned),
        (
            InputActionConfig::E3,
            KeyBindingSlot::Controller1P,
            ActionControllerBinding::Inherit(vec!["Button11".into()]),
        ),
    ] {
        assert_eq!(
            action_controller_binding(&loaded.input, action, slot),
            expected,
            "{action:?} {slot:?}"
        );
    }
    let format = |action, slot| {
        format_play_binding(&loaded, KeyMode::K7, KeyBindingTarget::Action { action, slot })
    };
    assert_eq!(format(InputActionConfig::E2, KeyBindingSlot::Controller1P), "Button14");
    assert_eq!(format(InputActionConfig::E2, KeyBindingSlot::Controller2P), "(COMMON: Button10)");
    assert_eq!(format(InputActionConfig::E3, KeyBindingSlot::Controller2P), "(none)");

    let mut pipeline = Pipeline::new(loaded.input, both_slots());
    assert!(pipeline.press(pad(LEFT, "Button14", true)).is_e2());
    assert!(!pipeline.press(pad(LEFT, "Button10", true)).is_e2());
    assert!(pipeline.press(pad(RIGHT, "Button10", true)).is_e2());
    assert!(!pipeline.press(pad(RIGHT, "Button11", true)).is_e3());
    assert!(pipeline.press(pad(LEFT, "Button11", true)).is_e3());
}

#[test]
fn e_button_sharing_a_number_with_the_other_sides_key_stays_on_its_side() {
    let mut profile = profile();
    // 2P の E2 を、1P の KEY1 と同じ番号の Button1 にする。
    set_action(&mut profile.input, InputActionConfig::E2, KeyBindingSlot::Controller2P, "Button1");
    let mut pipeline = Pipeline::new(profile.input, both_slots());
    let left = pipeline.press(pad(LEFT, "Button1", true));
    assert!(!left.is_e2());
    assert!(left.is_enter());
    let right = pipeline.press(pad(RIGHT, "Button1", true));
    assert!(right.is_e2());
    assert_eq!(right.ui_key(), None, "E-action を選曲レーンより優先する");
    assert_eq!(right.settings_input(), SettingsInput::default(), "E2 は設定画面で何もしない");
}

#[test]
fn releasing_one_sides_e1_keeps_the_other_sides_hold() {
    let mut pipeline = Pipeline::new(profile().input, both_slots());
    pipeline.event(pad(LEFT, "Button9", true));
    pipeline.event(pad(RIGHT, "Button9", true));
    assert!(pipeline.holds().0);
    pipeline.event(pad(LEFT, "Button9", false));
    assert!(pipeline.holds().0, "2P の E1 がまだ押されている");
    pipeline.event(pad(RIGHT, "Button9", false));
    assert!(!pipeline.holds().0);
}

#[test]
fn cross_side_e1_e2_chord_is_kept() {
    let mut pipeline = Pipeline::new(profile().input, both_slots());
    pipeline.event(pad(LEFT, "Button9", true));
    pipeline.event(pad(RIGHT, "Button10", true));
    let (e1, e2, actions) = pipeline.holds();
    assert!(e1 && e2);
    assert_eq!(actions, vec![InputActionConfig::E1, InputActionConfig::E2]);
}

#[test]
fn per_side_e1_hold_is_aggregated_from_each_source() {
    let mut profile = profile();
    set_action(&mut profile.input, InputActionConfig::E1, KeyBindingSlot::Controller2P, "Button13");
    let mut pipeline = Pipeline::new(profile.input, both_slots());
    pipeline.event(pad(RIGHT, "Button9", true));
    assert!(!pipeline.holds().0, "2P の Button9 は E1 ではない");
    pipeline.event(pad(RIGHT, "Button13", true));
    pipeline.event(pad(LEFT, "Button9", true));
    pipeline.event(pad(RIGHT, "Button13", false));
    assert!(pipeline.holds().0, "1P の E1 が残る");
}

// ---------------------------------------------------------------------------
// 押下状態の整合: 切断・フォーカス喪失・設定変更・割り当て変更
// ---------------------------------------------------------------------------

#[test]
fn disconnect_resync_releases_only_that_device() {
    let mut pipeline = Pipeline::new(profile().input, both_slots());
    pipeline.event(pad(LEFT, "Button9", true));
    pipeline.event(pad(RIGHT, "Button9", true));
    pipeline.event(key(KeyCode::KeyW, true));
    // RIGHT が切断され、backend の押下スナップショットから消えた。
    pipeline.runtime.replace_gamepad_pressed_controls(&[GamepadPressedButton {
        name: "Button9".into(),
        device_id: LEFT,
    }]);
    let (e1, e2, _) = pipeline.holds();
    assert!(e1 && e2);
    pipeline.runtime.replace_gamepad_pressed_controls(&[]);
    let (e1, e2, _) = pipeline.holds();
    assert!(!e1 && e2, "キーボードの E2 は残る");
}

#[test]
fn focus_loss_clears_all_holds() {
    let mut pipeline = Pipeline::new(profile().input, both_slots());
    pipeline.event(pad(LEFT, "Button9", true));
    pipeline.event(key(KeyCode::KeyQ, true));
    let _ = pipeline.runtime.handle_focus_lost();
    assert_eq!(pipeline.holds(), (false, false, Vec::new()));
}

#[test]
fn config_change_reresolves_held_sources() {
    let mut profile = profile();
    let mut pipeline = Pipeline::new(profile.input.clone(), both_slots());
    pipeline.event(pad(RIGHT, "Button13", true));
    assert!(!pipeline.holds().0);
    set_action(&mut profile.input, InputActionConfig::E1, KeyBindingSlot::Controller2P, "Button13");
    pipeline.reload_config(profile.input);
    assert!(pipeline.holds().0, "押したままの入力元も新しい設定で集約する");
    pipeline.event(pad(RIGHT, "Button13", false));
    assert!(!pipeline.holds().0);
}

#[test]
fn slot_reassignment_changes_the_side_of_the_next_event() {
    let mut profile = profile();
    set_key(
        &mut profile.input,
        KeyMode::K14,
        LaneConfig::Key8,
        KeyBindingSlot::Controller2P,
        "Button21",
    );
    let mut pipeline = Pipeline::new(profile.input, both_slots());
    assert_eq!(pipeline.press(pad(RIGHT, "Button21", true)).ui_key(), Some(1));
    pipeline.slots = GamepadSlotMap::from_device_ids([Some(RIGHT), Some(LEFT)]);
    assert_eq!(pipeline.press(pad(RIGHT, "Button21", true)).ui_key(), None);
    assert_eq!(pipeline.press(pad(LEFT, "Button21", true)).ui_key(), Some(1));
}

#[test]
fn release_identity_is_the_physical_source() {
    let mut pipeline = Pipeline::new(profile().input, both_slots());
    let press = pipeline.event(pad(LEFT, "Button1", true));
    let release_other = pipeline.event(pad(RIGHT, "Button1", false));
    assert_ne!(press.source(), release_other.source(), "別機器の同名ボタンの Release とは区別する");
    let release = pipeline.event(pad(LEFT, "Button1", false));
    assert_eq!(press.source(), release.source());
    assert!(!release.pressed());
}

// ---------------------------------------------------------------------------
// キーボード・9K・既定キー
// ---------------------------------------------------------------------------

#[test]
fn keyboard_prefers_seven_key_on_conflict_and_adds_2p_keys_from_14k() {
    let mut pipeline = Pipeline::new(profile().input, both_slots());
    let z = pipeline.press(key(KeyCode::KeyZ, true));
    assert_eq!(z.option_lane(), Some(Lane::Key1));
    assert_eq!(z.arrange_input(), Some((PlayerSide::P1, true)));
    // 既定の 14K 2P KEY1 (M) は 2P 側として扱う。
    let m = pipeline.press(key(KeyCode::KeyM, true));
    assert_eq!(m.arrange_input(), Some((PlayerSide::P2, true)));
    assert!(m.is_enter());
}

#[test]
fn keyboard_defaults_are_resolver_owned() {
    let mut pipeline = Pipeline::new(profile().input, both_slots());
    let enter = pipeline.event(key(KeyCode::Enter, true));
    assert_eq!(enter.scene().standard(), Some(StandardKey::Enter));
    assert_eq!(select_action(&enter), Some(SelectAction::EnterOrPlay));
    assert_eq!(result_action(&enter), Some(ResultAction::Leave));
    assert_eq!(decide_action(&enter), Some(DecideAction::Confirm));
    let up = pipeline.press(key(KeyCode::ArrowUp, true));
    assert!(up.settings_input().increase);
    assert_eq!(target_cycle_from_input(&up), Some(TargetCycle::Next));
    // コントローラーの同名ボタンからは既定キーの意味を作らない。
    let pad_enter = pipeline.press(pad(LEFT, "Enter", true));
    assert_eq!(pad_enter.standard(), None);
}

#[test]
fn nine_key_key8_key9_are_not_the_2p_side() {
    let mut profile = profile();
    profile.input.select_input_mode = SelectInputModeConfig::Key9;
    set_key(
        &mut profile.input,
        KeyMode::K9,
        LaneConfig::Key8,
        KeyBindingSlot::Controller,
        "Button28",
    );
    set_key(
        &mut profile.input,
        KeyMode::K9,
        LaneConfig::Key9,
        KeyBindingSlot::Controller,
        "Button29",
    );
    let mut pipeline = Pipeline::new(profile.input, both_slots());
    for device in [LEFT, RIGHT] {
        let key8 = pipeline.press(pad(device, "Button28", true));
        assert_eq!(key8.arrange_input(), None);
        assert_eq!(key8.option_lane(), None);
        assert_eq!(target_cycle_from_input(&key8), Some(TargetCycle::Previous));
        let key9 = pipeline.press(pad(device, "Button29", true));
        assert_eq!(target_cycle_from_input(&key9), Some(TargetCycle::Next));
    }
}

#[test]
fn nine_key_settings_lanes_win_over_e2() {
    let mut profile = profile();
    profile.input.select_input_mode = SelectInputModeConfig::Key9;
    let pipeline = Pipeline::new(profile.input.clone(), both_slots());
    let nine_key_lane_control =
        crate::config::play_input::resolve_play_bindings(&profile.input, KeyMode::K9)
            .unwrap()
            .into_iter()
            .find(|entry| entry.device == "keyboard" && entry.lane == Some(LaneConfig::Key3))
            .map(|entry| entry.control)
            .unwrap();
    set_action(
        &mut profile.input,
        InputActionConfig::E2,
        KeyBindingSlot::KeyboardPrimary,
        &nine_key_lane_control,
    );
    let scene = SceneInputResolver::from_profile(&profile.input);
    let control = scene.resolve_control(
        crate::input::winit::W_KEYBOARD_DEVICE_ID,
        &PhysicalControl::KeyboardKey(nine_key_lane_control),
        pipeline.slots,
    );
    assert!(control.settings_input().back);
    assert!(!control.settings_input().confirm);
}

// ---------------------------------------------------------------------------
// 設定フォルダの操作 (E2 に戻る・キャンセルを持たせない)
// ---------------------------------------------------------------------------

fn settings(control: &ResolvedControl) -> SettingsInput {
    control.settings_input()
}

#[test]
fn settings_folder_e2_has_no_meaning_and_even_keys_go_back() {
    let mut pipeline = Pipeline::new(profile().input, both_slots());
    for device in [LEFT, RIGHT] {
        // 既定の E2 (Button10) は設定フォルダで何もしない。
        assert_eq!(
            settings(&pipeline.press(pad(device, "Button10", true))),
            SettingsInput::default()
        );
        let even = settings(&pipeline.press(pad(device, "Button2", true)));
        assert!(even.back && !even.confirm, "{device:?}");
        let odd = settings(&pipeline.press(pad(device, "Button1", true)));
        assert!(odd.confirm && !odd.back, "{device:?}");
    }
    // キーボードの E2 (既定 W) も同じ。Escape / 左矢印は戻る。
    assert_eq!(settings(&pipeline.press(key(KeyCode::KeyW, true))), SettingsInput::default());
    assert!(settings(&pipeline.press(key(KeyCode::Escape, true))).back);
    assert!(settings(&pipeline.press(key(KeyCode::ArrowLeft, true))).back);
    assert!(settings(&pipeline.press(key(KeyCode::KeyS, true))).back);
}

#[test]
fn settings_folder_e2_on_an_odd_key_does_not_confirm() {
    let mut profile = profile();
    // 1P の E2 を、7K KEY1 と同じ Button1 にする。2P は共通の E2 (Button10) のまま。
    set_action(&mut profile.input, InputActionConfig::E2, KeyBindingSlot::Controller1P, "Button1");
    // キーボードの E2 を 7K KEY3 (X) にする。
    set_action(&mut profile.input, InputActionConfig::E2, KeyBindingSlot::KeyboardPrimary, "X");
    let mut pipeline = Pipeline::new(profile.input, both_slots());

    let left = pipeline.press(pad(LEFT, "Button1", true));
    assert!(left.is_e2());
    assert_eq!(settings(&left), SettingsInput::default(), "E アクションを鍵盤より優先する");
    // 2P の Button1 は 2P KEY1 のまま決定。
    let right = pipeline.press(pad(RIGHT, "Button1", true));
    assert!(!right.is_e2());
    assert!(settings(&right).confirm);
    // 1P の Button10 は 1P の個別設定で E2 ではなくなったが、鍵盤でもない。
    assert_eq!(settings(&pipeline.press(pad(LEFT, "Button10", true))), SettingsInput::default());
    assert_eq!(settings(&pipeline.press(pad(RIGHT, "Button10", true))), SettingsInput::default());

    let x = pipeline.press(key(KeyCode::KeyX, true));
    assert!(x.is_e2());
    assert_eq!(settings(&x), SettingsInput::default());
}

#[test]
fn settings_folder_14k_2p_e2_on_2p_key_does_not_confirm_or_go_back() {
    let mut profile = profile();
    // 14K の 2P KEY2 を Button12 にし、2P の E2 も Button12 にする。
    set_key(
        &mut profile.input,
        KeyMode::K14,
        LaneConfig::Key9,
        KeyBindingSlot::Controller2P,
        "Button12",
    );
    set_action(&mut profile.input, InputActionConfig::E2, KeyBindingSlot::Controller2P, "Button12");
    let mut pipeline = Pipeline::new(profile.input, both_slots());
    assert_eq!(settings(&pipeline.press(pad(RIGHT, "Button12", true))), SettingsInput::default());
    assert_eq!(settings(&pipeline.press(pad(LEFT, "Button12", true))), SettingsInput::default());
}

#[test]
fn nine_key_settings_keep_lane_priority_over_e2() {
    let mut profile = profile();
    profile.input.select_input_mode = SelectInputModeConfig::Key9;
    // 9K の KEY5 (C) を E2 にしても、9K では鍵盤の意味 (決定) を優先する。
    set_action(&mut profile.input, InputActionConfig::E2, KeyBindingSlot::KeyboardPrimary, "C");
    let mut pipeline = Pipeline::new(profile.input, both_slots());
    let c = pipeline.press(key(KeyCode::KeyC, true));
    assert!(settings(&c).confirm && !settings(&c).back);
    // E2 だけに割り当てたキー (既定の W は外れたので Q 以外の E2 はない) は何もしない。
    assert_eq!(settings(&pipeline.press(key(KeyCode::KeyW, true))), SettingsInput::default());
    assert!(settings(&pipeline.press(key(KeyCode::KeyX, true))).back, "9K KEY3 は戻る");
}

// ---------------------------------------------------------------------------
// 7K / 14K 間のレーン単位のフォールバック
// ---------------------------------------------------------------------------

fn clear_key(
    input: &mut ProfileInputConfig,
    mode: KeyMode,
    lane: LaneConfig,
    slot: KeyBindingSlot,
) {
    clear_play_binding(input, mode, KeyBindingTarget::Key { lane, slot }).unwrap();
}

fn key_label(
    profile: &ProfileConfig,
    mode: KeyMode,
    lane: LaneConfig,
    slot: KeyBindingSlot,
) -> String {
    format_play_binding(profile, mode, KeyBindingTarget::Key { lane, slot })
}

/// 14K の 2P 側のレーン設定を消して「設定なし (継承)」にする (旧profileの状態)。
fn remove_14k_2p_lane(input: &mut ProfileInputConfig, lane: LaneConfig) {
    let mut bindings =
        crate::config::play_input::resolve_play_bindings_with_markers(input, KeyMode::K14).unwrap();
    bindings.retain(|entry| {
        !(entry.lane == Some(lane) && crate::config::play_input::is_gamepad_device(&entry.device))
    });
    let config = input.play.entry(KeyMode::K14.play_map_key().to_string()).or_default();
    config.inherit = None;
    config.bindings = bindings;
}

#[test]
fn same_button_numbers_resolve_each_side_from_its_own_mode() {
    let mut pipeline = Pipeline::new(profile().input, both_slots());
    for (device, side) in [(LEFT, PlayerSide::P1), (RIGHT, PlayerSide::P2)] {
        for (button, key) in [("Button1", 1), ("Button4", 4), ("Button7", 7)] {
            let control = pipeline.press(pad(device, button, true));
            assert_eq!(control.ui_key(), Some(key), "{device:?} {button}");
            assert_eq!(control.select_lane().map(|lane| lane.side), Some(side));
        }
    }
    assert_eq!(
        pipeline.press(pad(RIGHT, "Button1", true)).select_lane().map(|lane| lane.mode),
        Some(KeyMode::K14)
    );
    assert_eq!(
        pipeline.press(pad(LEFT, "Button1", true)).select_lane().map(|lane| lane.mode),
        Some(KeyMode::K7)
    );
}

#[test]
fn explicit_14k_2p_lane_disables_the_7k_lane_for_2p() {
    let mut profile = profile();
    set_key(
        &mut profile.input,
        KeyMode::K14,
        LaneConfig::Key8,
        KeyBindingSlot::Controller2P,
        "Button21",
    );
    let mut pipeline = Pipeline::new(profile.input, both_slots());
    assert_eq!(pipeline.press(pad(RIGHT, "Button21", true)).ui_key(), Some(1));
    assert_eq!(pipeline.press(pad(RIGHT, "Button1", true)).ui_key(), None);
    // 他のレーンは 14K の 2P 設定のまま。1P は変わらない。
    assert_eq!(pipeline.press(pad(RIGHT, "Button2", true)).ui_key(), Some(2));
    assert_eq!(pipeline.press(pad(LEFT, "Button1", true)).ui_key(), Some(1));
    assert_eq!(pipeline.press(pad(LEFT, "Button21", true)).ui_key(), None);
}

#[test]
fn inherited_14k_2p_lane_falls_back_to_the_same_devices_7k_lane() {
    let mut profile = profile();
    remove_14k_2p_lane(&mut profile.input, LaneConfig::Key8);
    remove_14k_2p_lane(&mut profile.input, LaneConfig::Scratch2);
    // 7K の KEY1 を別の番号にすると、2P KEY1 も 7K のレーン設定に従う。
    set_key(
        &mut profile.input,
        KeyMode::K7,
        LaneConfig::Key1,
        KeyBindingSlot::Controller,
        "Button15",
    );
    assert_eq!(
        key_label(&profile, KeyMode::K14, LaneConfig::Key8, KeyBindingSlot::Controller2P),
        "(SELECT 7K: Button15)"
    );
    let mut pipeline = Pipeline::new(profile.input, both_slots());
    let right = pipeline.press(pad(RIGHT, "Button15", true));
    assert_eq!(right.ui_key(), Some(1));
    assert_eq!(right.arrange_input(), Some((PlayerSide::P2, true)));
    assert_eq!(right.select_lane().map(|lane| lane.mode), Some(KeyMode::K7));
    // 7K の KEY1 から外した Button1 は、2P KEY1 にもならない。
    assert_eq!(pipeline.press(pad(RIGHT, "Button1", true)).ui_key(), None);
    // 2P の SCR も 7K の SCR (Axis1) を継承する。
    assert_eq!(pipeline.axis(RIGHT, "Axis1", 1).scene_scroll, Some(-1));
    // 1P は 7K の設定で解決する。
    assert_eq!(pipeline.press(pad(LEFT, "Button15", true)).ui_key(), Some(1));
}

#[test]
fn explicitly_unassigned_lane_does_not_fall_back_and_clearing_again_inherits() {
    let mut profile = profile();
    clear_key(&mut profile.input, KeyMode::K14, LaneConfig::Key8, KeyBindingSlot::Controller2P);
    assert_eq!(
        key_label(&profile, KeyMode::K14, LaneConfig::Key8, KeyBindingSlot::Controller2P),
        "(none)"
    );
    let mut pipeline = Pipeline::new(profile.input.clone(), both_slots());
    assert_eq!(pipeline.press(pad(RIGHT, "Button1", true)).ui_key(), None, "7K へ戻らない");
    assert_eq!(pipeline.press(pad(LEFT, "Button1", true)).ui_key(), Some(1));

    // もう一度外すと「設定なし」へ戻り、同じ 2P 機器の 7K KEY1 を使う。
    clear_key(&mut profile.input, KeyMode::K14, LaneConfig::Key8, KeyBindingSlot::Controller2P);
    assert_eq!(
        key_label(&profile, KeyMode::K14, LaneConfig::Key8, KeyBindingSlot::Controller2P),
        "(SELECT 7K: Button1)"
    );
    pipeline.reload_config(profile.input.clone());
    let right = pipeline.press(pad(RIGHT, "Button1", true));
    assert_eq!(right.ui_key(), Some(1));
    assert_eq!(right.select_lane().map(|lane| lane.side), Some(PlayerSide::P2));
}

#[test]
fn seven_key_lane_for_1p_is_preferred_and_unassigned_stops_the_14k_fallback() {
    let mut profile = profile();
    // 14K の 1P KEY1 を Button21 にしても、1P は 7K の KEY1 (Button1) を優先する。
    set_key(
        &mut profile.input,
        KeyMode::K14,
        LaneConfig::Key1,
        KeyBindingSlot::Controller1P,
        "Button21",
    );
    let mut pipeline = Pipeline::new(profile.input.clone(), both_slots());
    assert_eq!(pipeline.press(pad(LEFT, "Button1", true)).ui_key(), Some(1));
    assert_eq!(pipeline.press(pad(LEFT, "Button21", true)).ui_key(), None);

    // 7K の KEY1 を明示的に外すと、14K へもフォールバックしない。
    clear_key(&mut profile.input, KeyMode::K7, LaneConfig::Key1, KeyBindingSlot::Controller);
    pipeline.reload_config(profile.input.clone());
    assert_eq!(pipeline.press(pad(LEFT, "Button1", true)).ui_key(), None);
    assert_eq!(pipeline.press(pad(LEFT, "Button21", true)).ui_key(), None);
    // 2P は 14K の 2P KEY1 (Button1) のまま。
    assert_eq!(pipeline.press(pad(RIGHT, "Button1", true)).ui_key(), Some(1));

    // もう一度外して「設定なし」にすると、14K の 1P KEY1 を使う。
    clear_key(&mut profile.input, KeyMode::K7, LaneConfig::Key1, KeyBindingSlot::Controller);
    assert_eq!(
        key_label(&profile, KeyMode::K7, LaneConfig::Key1, KeyBindingSlot::Controller),
        "(SELECT 14K 1P: Button21)"
    );
    pipeline.reload_config(profile.input);
    let left = pipeline.press(pad(LEFT, "Button21", true));
    assert_eq!(left.ui_key(), Some(1));
    assert_eq!(left.select_lane().map(|lane| lane.side), Some(PlayerSide::P1));
}

#[test]
fn lane_markers_survive_save_and_reload_and_stay_out_of_play_bindings() {
    let mut profile = profile();
    clear_key(&mut profile.input, KeyMode::K14, LaneConfig::Key8, KeyBindingSlot::Controller2P);
    remove_14k_2p_lane(&mut profile.input, LaneConfig::Key9);
    let saved = toml::to_string(&profile).unwrap();
    let mut loaded: ProfileConfig = toml::from_str(&saved).unwrap();
    crate::config::play_input::normalize_profile_input(&mut loaded.input);
    let mut pipeline = Pipeline::new(loaded.input.clone(), both_slots());
    // KEY1 は明示的な未割り当て、KEY2 は継承 (7K の Button2)。
    assert_eq!(pipeline.press(pad(RIGHT, "Button1", true)).ui_key(), None);
    assert_eq!(pipeline.press(pad(RIGHT, "Button2", true)).ui_key(), Some(2));
    // プレイと競合検出は marker を割り当てとして扱わない。
    let binding = crate::config::play::lane_binding_for_chart_with_slots(
        &loaded.input,
        KeyMode::K14,
        both_slots(),
    );
    assert!(
        crate::config::play_input::resolve_play_bindings(&loaded.input, KeyMode::K14)
            .unwrap()
            .iter()
            .all(|entry| !entry.control.is_empty())
    );
    pipeline.play =
        Some(crate::app::PlayOptionInput::new(KeyMode::K14, binding, &loaded.input, both_slots()));
    let play = pipeline.event(pad(RIGHT, "Button1", true));
    assert_eq!(play.play().and_then(ResolvedControl::ui_key), None);
}

#[test]
fn restoring_14k_2p_defaults_makes_the_lanes_explicit_again() {
    let mut profile = profile();
    clear_key(&mut profile.input, KeyMode::K14, LaneConfig::Key8, KeyBindingSlot::Controller2P);
    crate::config::key_config::restore_key_mode_defaults(
        &mut profile.input,
        KeyMode::K14,
        KeyBindingSlot::Controller,
    )
    .unwrap();
    assert_eq!(
        key_label(&profile, KeyMode::K14, LaneConfig::Key8, KeyBindingSlot::Controller2P),
        "Button1"
    );
    let mut pipeline = Pipeline::new(profile.input, both_slots());
    assert_eq!(pipeline.press(pad(RIGHT, "Button1", true)).ui_key(), Some(1));
}

#[test]
fn unassigned_controller_keeps_common_7k_then_14k_rule() {
    let mut profile = profile();
    clear_key(&mut profile.input, KeyMode::K14, LaneConfig::Key8, KeyBindingSlot::Controller2P);
    let mut pipeline =
        Pipeline::new(profile.input, GamepadSlotMap::from_device_ids([Some(LEFT), Some(RIGHT)]));
    let spare = pipeline.press(pad(SPARE, "Button1", true));
    assert_eq!(spare.slot, None);
    assert_eq!(spare.ui_key(), Some(1));
    assert_eq!(spare.select_lane().map(|lane| lane.mode), Some(KeyMode::K7));
}

// ---------------------------------------------------------------------------
// プレー中の 1P / 2P 割り当ての変更 (開始時のスナップショットで解決する)
// ---------------------------------------------------------------------------

/// 14K で 2P の KEY1 を 1P と別の番号にした profile。側ごとの解決の違いが見える。
fn split_14k_profile() -> ProfileInputConfig {
    let mut profile = profile();
    set_key(
        &mut profile.input,
        KeyMode::K14,
        LaneConfig::Key8,
        KeyBindingSlot::Controller2P,
        "Button21",
    );
    set_action(&mut profile.input, InputActionConfig::E1, KeyBindingSlot::Controller2P, "Button22");
    profile.input
}

fn note_lane(pipeline: &Pipeline, device: DeviceId, button: &str) -> Option<Lane> {
    pipeline
        .play
        .as_ref()
        .unwrap()
        .resolve_entry(device, &PhysicalControl::GamepadButton(button.into()))
        .map(|entry| entry.lane)
}

#[test]
fn swapping_controllers_mid_play_keeps_the_play_start_assignment() {
    let mut pipeline = Pipeline::new(split_14k_profile(), both_slots());
    pipeline.start_play(KeyMode::K14);
    // プレー中に左右を入れ替えた。
    pipeline.slots = GamepadSlotMap::from_device_ids([Some(RIGHT), Some(LEFT)]);

    let right = pipeline.event(pad(RIGHT, "Button21", true));
    assert_eq!(right.scene().slot, Some(PlayerSide::P2), "選曲側の意味も開始時の割り当て");
    assert_eq!(right.play().and_then(|play| play.slot), Some(PlayerSide::P2));
    assert_eq!(note_lane(&pipeline, RIGHT, "Button21"), Some(Lane::Key8));
    assert_eq!(note_lane(&pipeline, LEFT, "Button1"), Some(Lane::Key1));
    // E1 の個別設定も開始時の側で解決する。
    assert!(pipeline.event(pad(RIGHT, "Button22", true)).play().unwrap().is_play_start());
    assert!(!pipeline.event(pad(LEFT, "Button22", true)).play().unwrap().is_play_start());

    // 選曲へ戻ると、次の操作とプレーには更新後の割り当てを使う。
    pipeline.end_play();
    assert_eq!(pipeline.press(pad(RIGHT, "Button1", true)).slot, Some(PlayerSide::P1));
    assert_eq!(pipeline.press(pad(LEFT, "Button21", true)).ui_key(), Some(1));
    pipeline.start_play(KeyMode::K14);
    assert_eq!(note_lane(&pipeline, LEFT, "Button21"), Some(Lane::Key8));
    assert_eq!(note_lane(&pipeline, RIGHT, "Button1"), Some(Lane::Key1));
}

#[test]
fn press_and_release_across_a_mid_play_swap_resolve_to_the_same_lane() {
    let mut pipeline = Pipeline::new(split_14k_profile(), both_slots());
    pipeline.start_play(KeyMode::K14);
    let press = pipeline.event(pad(RIGHT, "Button22", true));
    assert!(press.play().unwrap().is_play_start());
    assert!(pipeline.holds().0);
    pipeline.slots = GamepadSlotMap::from_device_ids([Some(RIGHT), Some(LEFT)]);
    // 入替え後も押したままの 2P の E1 は 2P の E1 のまま。
    assert!(pipeline.holds().0);
    let release = pipeline.event(pad(RIGHT, "Button22", false));
    assert_eq!(release.source(), press.source());
    assert!(release.play().unwrap().is_play_start());
    assert!(!pipeline.holds().0);
}

#[test]
fn one_side_disconnect_and_reconnect_mid_play() {
    let mut pipeline = Pipeline::new(split_14k_profile(), both_slots());
    pipeline.start_play(KeyMode::K14);
    pipeline.event(pad(RIGHT, "Button22", true));
    pipeline.event(pad(LEFT, "Button9", true));

    // 2P が切断された。backend の押下スナップショットから消え、実行時の割り当ても空く。
    pipeline.slots = GamepadSlotMap::from_device_ids([Some(LEFT), None]);
    pipeline.runtime.replace_gamepad_pressed_controls(&[GamepadPressedButton {
        name: "Button9".into(),
        device_id: LEFT,
    }]);
    let (e1, _, actions) = pipeline.holds();
    assert!(e1, "1P の E1 は残る");
    assert_eq!(actions, vec![InputActionConfig::E1]);
    pipeline.event(pad(LEFT, "Button9", false));
    assert!(!pipeline.holds().0, "切断した 2P の押下は残らない");

    // 切断中に別のコントローラーが 2P として認識されても、このプレーでは 2P にしない。
    pipeline.slots = GamepadSlotMap::from_device_ids([Some(LEFT), Some(SPARE)]);
    let spare = pipeline.event(pad(SPARE, "Button21", true));
    assert_eq!(spare.scene().slot, None);
    assert_eq!(spare.play().and_then(|play| play.slot), None);
    assert_eq!(note_lane(&pipeline, SPARE, "Button21"), None);
    pipeline.event(pad(SPARE, "Button21", false));

    // 元の 2P が戻ると、開始時と同じく 2P で解決する。
    pipeline.slots = both_slots();
    assert_eq!(pipeline.event(pad(RIGHT, "Button21", true)).scene().slot, Some(PlayerSide::P2));
    assert_eq!(note_lane(&pipeline, RIGHT, "Button21"), Some(Lane::Key8));
}

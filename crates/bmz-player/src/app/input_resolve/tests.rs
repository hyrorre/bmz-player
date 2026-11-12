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

    /// backend からの1イベント。押下集合を更新してから解決する (app と同じ順)。
    fn event(&mut self, event: ControlInputEvent) -> ResolvedInput {
        self.runtime.track_control(&event);
        resolve_input(&self.scene, self.play.as_ref(), self.slots, event)
    }

    fn press(&mut self, event: ControlInputEvent) -> ResolvedControl {
        *self.event(event).scene()
    }

    fn axis(&self, device: DeviceId, axis: &str, ticks: i32) -> ResolvedAxisTick {
        resolve_axis_tick(&self.scene, self.play.as_ref(), self.slots, device, axis, ticks)
    }

    /// 押下中の入力元をそれぞれ解決して集約した E1 / E2 / E1〜E4。
    fn holds(&self) -> (bool, bool, Vec<InputActionConfig>) {
        let pressed = self
            .runtime
            .pressed_sources()
            .map(|(device, control)| self.scene.resolve_control(device, control, self.slots));
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
    // 2P の Button1 は 14K で見つからなければ同じ入力元の 7K 設定で解決する (側は 2P のまま)。
    let right_button1 = pipeline.press(pad(RIGHT, "Button1", true));
    assert_eq!(right_button1.ui_key(), Some(1));
    assert_eq!(right_button1.arrange_input(), Some((PlayerSide::P2, true)));
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
    // 2P の Axis1 は 14K から外れても、同じ入力元の 7K 共通設定で解決する。
    assert_eq!(pipeline.axis(RIGHT, "Axis1", 1).scene_scroll, Some(-1));
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
    assert!(right.settings_input().back);
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

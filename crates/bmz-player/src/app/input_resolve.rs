//! キーコンフィグ対象の物理入力を、操作層へ渡す前に解決する共通層。
//!
//! 入力経路は次の順に固定する。
//!
//! 1. backend から [`ControlInputEvent`] を受け取る（取得時刻・合成アナログ属性を保持）。
//! 2. 物理 [`DeviceId`] から論理スロット（1P / 2P）を [`GamepadSlotMap`] で特定する。
//! 3. 画面・入力モード・設定対象モードに応じた binding 表（[`LaneTable`] /
//!    [`ActionTable`]）でキーコンフィグを解決する。
//! 4. 出自と意味を保持した [`ResolvedInput`] へ変換する。
//! 5. 画面側は [`ResolvedControl`] の意味だけを見て Action を決める。
//!
//! [`ResolvedInput`] のフィールドはこのモジュールの外から組み立てられない。操作層の
//! API は解決済み入力を受け取り、生のボタン名・軸名から操作を再判定しない。
//! 生の名前が必要なキーコンフィグ待ち受け・診断表示は、解決前の専用経路で扱う。

use bmz_core::input::ScratchDirection;
use bmz_core::lane::{KeyMode, Lane};
use bmz_gameplay::input::backend::{DeviceId, DeviceTimestamp, PhysicalControl};

use super::input_runtime::ControlInputEvent;
use crate::config::play_input::{control_from_config, gamepad_player_index, resolve_play_bindings};
use crate::config::profile_config::{
    BindingConfigEntry, InputActionConfig, LaneConfig, ProfileInputConfig, ScratchDirectionConfig,
    SelectInputModeConfig,
};
use crate::input::gamepad::GamepadSlotMap;

#[cfg(test)]
#[path = "input_resolve/tests.rs"]
mod tests;

/// 論理プレイヤー側。コントローラーの論理スロット、鍵盤の左右に使う。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(super) enum PlayerSide {
    P1,
    P2,
}

impl PlayerSide {
    pub(super) fn from_player_index(index: u32) -> Option<Self> {
        match index {
            1 => Some(Self::P1),
            2 => Some(Self::P2),
            _ => None,
        }
    }

    const fn slot_index(self) -> usize {
        match self {
            Self::P1 => 0,
            Self::P2 => 1,
        }
    }
}

/// binding が対象とする物理デバイスの種類。
///
/// `AnyController` は意図的な全コントローラー共通設定 (`device = "gamepad"`)、
/// `ControllerSlot` は論理スロットの実機だけを対象にする。未解決スロットの実機は
/// どの `ControllerSlot` にも一致しない。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum BindingDevice {
    Keyboard,
    AnyController,
    ControllerSlot(PlayerSide),
    Hid,
}

impl BindingDevice {
    pub(super) fn from_config(device: &str) -> Option<Self> {
        let trimmed = device.trim();
        if trimmed.eq_ignore_ascii_case("keyboard") {
            return Some(Self::Keyboard);
        }
        if trimmed.eq_ignore_ascii_case("gamepad") {
            return Some(Self::AnyController);
        }
        if trimmed.eq_ignore_ascii_case("hid") {
            return Some(Self::Hid);
        }
        gamepad_player_index(trimmed)
            .and_then(PlayerSide::from_player_index)
            .map(Self::ControllerSlot)
    }

    fn matches(self, control: &PhysicalControl, slot: Option<PlayerSide>) -> bool {
        match (self, control) {
            (Self::Keyboard, PhysicalControl::KeyboardKey(_)) => true,
            (Self::AnyController, PhysicalControl::GamepadButton(_)) => true,
            (Self::ControllerSlot(side), PhysicalControl::GamepadButton(_)) => slot == Some(side),
            (Self::Hid, PhysicalControl::HidButton(_)) => true,
            _ => false,
        }
    }

    const fn is_slot_specific(self) -> bool {
        matches!(self, Self::ControllerSlot(_))
    }
}

/// 入力元。押下状態・長押し・Release の対応付けに使う。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(super) struct InputSource {
    pub(super) device: DeviceId,
    pub(super) control: PhysicalControl,
}

/// 選曲側のスクラッチ方向。旧profileの方向未指定は両方向扱いになる。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SceneScratch {
    Up,
    Down,
    Both,
}

/// 解決済みのレーン。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct ResolvedLane {
    /// 解決元の設定対象モードでのレーン。
    pub(super) lane: Lane,
    /// 解決元の設定対象モード。
    pub(super) mode: KeyMode,
    /// 鍵盤の左右。7K 設定で解決した場合は入力元のスロット (未割り当ては 1P)。
    pub(super) side: PlayerSide,
    /// プレイ用のスクラッチ方向。
    pub(super) scratch: Option<ScratchDirection>,
    /// 選曲用のスクラッチ方向。
    pub(super) scene_scratch: Option<SceneScratch>,
}

impl ResolvedLane {
    /// 各側の KEY 番号。7K/14K は 1..=7、9K は 1..=9。スクラッチは `None`。
    pub(super) fn side_key(self) -> Option<u8> {
        let index = match self.lane {
            Lane::Key1 => 1,
            Lane::Key2 => 2,
            Lane::Key3 => 3,
            Lane::Key4 => 4,
            Lane::Key5 => 5,
            Lane::Key6 => 6,
            Lane::Key7 => 7,
            Lane::Key8 => 8,
            Lane::Key9 => 9,
            Lane::Key10 => 10,
            Lane::Key11 => 11,
            Lane::Key12 => 12,
            Lane::Key13 => 13,
            Lane::Key14 => 14,
            Lane::Scratch | Lane::Scratch2 => return None,
        };
        if self.mode == KeyMode::K9 {
            return Some(index);
        }
        Some(if index > 7 { index - 7 } else { index })
    }
}

/// キーボードの既定操作。共通 resolver が扱う組み込み binding で、画面側は
/// キー名ではなくこの値で判定する。コントローラーからは生成しない。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum StandardKey {
    Enter,
    Space,
    Escape,
    ArrowLeft,
    ArrowRight,
    ArrowUp,
    ArrowDown,
    PageUp,
    PageDown,
    Home,
    End,
    Tab,
    Delete,
    Backspace,
    /// Result の再挑戦キー。
    Retry,
    /// 数字キー (テンキーを含む)。
    Digit(u8),
    LControl,
    RControl,
    LShift,
    RShift,
}

impl StandardKey {
    fn from_keyboard_name(name: &str) -> Option<Self> {
        Some(match name {
            "Enter" => Self::Enter,
            "Space" => Self::Space,
            "Escape" => Self::Escape,
            "ArrowLeft" => Self::ArrowLeft,
            "ArrowRight" => Self::ArrowRight,
            "ArrowUp" => Self::ArrowUp,
            "ArrowDown" => Self::ArrowDown,
            "PageUp" => Self::PageUp,
            "PageDown" => Self::PageDown,
            "Home" => Self::Home,
            "End" => Self::End,
            "Tab" => Self::Tab,
            "Delete" => Self::Delete,
            "Backspace" => Self::Backspace,
            "R" => Self::Retry,
            "LControl" => Self::LControl,
            "RControl" => Self::RControl,
            "LShift" => Self::LShift,
            "RShift" => Self::RShift,
            _ => {
                let digit = name.strip_prefix("Numpad").unwrap_or(name);
                let mut chars = digit.chars();
                let (Some(c), None) = (chars.next(), chars.next()) else {
                    return None;
                };
                return c.to_digit(10).map(|value| Self::Digit(value as u8));
            }
        })
    }
}

/// 設定可能な単発ショートカット。ビット集合で保持する。
const SHORTCUT_ACTIONS: &[InputActionConfig] = &[
    InputActionConfig::SelectOpenFolder,
    InputActionConfig::SelectReload,
    InputActionConfig::SelectAutoplayFolder,
    InputActionConfig::SelectOpenIr,
    InputActionConfig::SelectOpenKeyConfig,
    InputActionConfig::Screenshot,
    InputActionConfig::SelectRivalCycle,
    InputActionConfig::SelectOpenDocuments,
    InputActionConfig::SelectFavoriteSong,
    InputActionConfig::SelectFavoriteChart,
    InputActionConfig::SelectSameFolder,
    InputActionConfig::SelectModeFilter,
    InputActionConfig::SelectSort,
    InputActionConfig::SelectLnMode,
    InputActionConfig::SelectDifficultyFilter,
    InputActionConfig::SelectReplayCycle,
    InputActionConfig::SelectReplayPlay,
    InputActionConfig::PlayHispeedDown,
    InputActionConfig::PlayHispeedUp,
    InputActionConfig::PlayLaneCoverUp,
    InputActionConfig::PlayLaneCoverDown,
    InputActionConfig::PlayVisualOffsetUp,
    InputActionConfig::PlayVisualOffsetDown,
    InputActionConfig::PlayVisualOffsetAutoAdjust,
];

const E_ACTIONS: [InputActionConfig; 4] =
    [InputActionConfig::E1, InputActionConfig::E2, InputActionConfig::E3, InputActionConfig::E4];

fn shortcut_bit(action: InputActionConfig) -> Option<u32> {
    SHORTCUT_ACTIONS.iter().position(|candidate| *candidate == action).map(|index| 1 << index)
}

fn e_action_index(action: InputActionConfig) -> Option<usize> {
    E_ACTIONS.iter().position(|candidate| *candidate == action)
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct ShortcutSet(u32);

impl ShortcutSet {
    pub(super) fn contains(self, action: InputActionConfig) -> bool {
        shortcut_bit(action).is_some_and(|bit| self.0 & bit != 0)
    }

    fn insert(&mut self, action: InputActionConfig) {
        if let Some(bit) = shortcut_bit(action) {
            self.0 |= bit;
        }
    }

    /// 設定順に依存しない固定順で最初のショートカットを返す。
    pub(super) fn first_in(self, order: &[InputActionConfig]) -> Option<InputActionConfig> {
        order.iter().copied().find(|action| self.contains(*action))
    }
}

/// 設定画面の操作。1つの入力が複数の意味を持つことがある (旧profileの方向未指定スクラッチ等)。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct SettingsInput {
    pub(super) confirm: bool,
    pub(super) back: bool,
    pub(super) increase: bool,
    pub(super) decrease: bool,
}

/// 1つの物理入力が持つ意味。デバイスと設定対象モードを解決した結果。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct ResolvedControl {
    /// 入力元の論理スロット。コントローラー以外、または未割り当て機器は `None`。
    pub(super) slot: Option<PlayerSide>,
    nine_key: bool,
    lane: Option<ResolvedLane>,
    e_action: Option<InputActionConfig>,
    shortcuts: ShortcutSet,
    standard: Option<StandardKey>,
}

impl ResolvedControl {
    const NONE: Self = Self {
        slot: None,
        nine_key: false,
        lane: None,
        e_action: None,
        shortcuts: ShortcutSet(0),
        standard: None,
    };

    /// どの設定にも一致しない入力。
    pub(super) const fn unbound() -> Self {
        Self::NONE
    }

    /// プレイ用 resolver ([`super::PlayOptionInput`]) の解決結果から作る。
    pub(super) fn for_play(
        slot: Option<PlayerSide>,
        mode: KeyMode,
        binding: Option<bmz_gameplay::input::binding::BindingResolution>,
        actions: &ActionTable,
        control: &PhysicalControl,
    ) -> Self {
        let lane = binding.map(|binding| {
            // 方向未指定のスクラッチは、解決した binding 自身のコントロール名から方向を推測する。
            let scratch = binding.scratch_direction.or_else(|| {
                if !matches!(binding.lane, Lane::Scratch | Lane::Scratch2) {
                    return None;
                }
                let name = match control {
                    PhysicalControl::KeyboardKey(name) | PhysicalControl::GamepadButton(name) => {
                        name.as_str()
                    }
                    PhysicalControl::HidButton(_) => return None,
                };
                if crate::config::key_config::is_scratch_up_control(name) {
                    Some(ScratchDirection::Up)
                } else if crate::config::key_config::is_scratch_down_control(name) {
                    Some(ScratchDirection::Down)
                } else {
                    None
                }
            });
            ResolvedLane {
                lane: binding.lane,
                mode,
                side: lane_side(binding.lane),
                scratch,
                scene_scratch: scratch.map(|direction| match direction {
                    ScratchDirection::Up => SceneScratch::Up,
                    ScratchDirection::Down => SceneScratch::Down,
                }),
            }
        });
        Self {
            slot,
            nine_key: mode == KeyMode::K9,
            lane,
            e_action: actions.e_action(control, slot),
            shortcuts: actions.shortcuts(control, slot),
            standard: match control {
                PhysicalControl::KeyboardKey(name) => StandardKey::from_keyboard_name(name),
                _ => None,
            },
        }
    }

    /// E1〜E4。
    pub(super) fn e_action(&self) -> Option<InputActionConfig> {
        self.e_action
    }

    /// E-action と重なっていても保持するレーン (プレイ・9K設定画面用)。
    pub(super) fn lane(&self) -> Option<ResolvedLane> {
        self.lane
    }

    /// 選曲系の画面で使うレーン。E1〜E4 と同じ入力ではE-actionを優先する。
    pub(super) fn select_lane(&self) -> Option<ResolvedLane> {
        if self.e_action.is_some() { None } else { self.lane }
    }

    pub(super) fn standard(&self) -> Option<StandardKey> {
        self.standard
    }

    pub(super) fn has_shortcut(&self, action: InputActionConfig) -> bool {
        self.shortcuts.contains(action)
    }

    pub(super) fn shortcuts(&self) -> ShortcutSet {
        self.shortcuts
    }

    pub(super) fn is_nine_key(&self) -> bool {
        self.nine_key
    }

    pub(super) fn is_start(&self) -> bool {
        self.e_action == Some(InputActionConfig::E1)
    }

    /// プレイ画面ではE4をE1と同じ操作として扱う。
    pub(super) fn is_play_start(&self) -> bool {
        matches!(self.e_action, Some(InputActionConfig::E1 | InputActionConfig::E4))
    }

    pub(super) fn is_e2(&self) -> bool {
        self.e_action == Some(InputActionConfig::E2)
    }

    pub(super) fn is_e3(&self) -> bool {
        self.e_action == Some(InputActionConfig::E3)
    }

    /// 9K の論理レーン。9K選曲入力以外は `None`。
    pub(super) fn nine_key_lane(&self) -> Option<Lane> {
        self.nine_key.then(|| self.select_lane().map(|lane| lane.lane)).flatten()
    }

    /// 選曲 UI の KEY1〜KEY7。7K/14K では左右どちらの側も同じ番号へ対応付ける。
    /// 9K の KEY8 / KEY9 は 2P 側ではないため対応付けない。
    pub(super) fn ui_key(&self) -> Option<u8> {
        self.select_lane().and_then(ResolvedLane::side_key).filter(|key| *key <= 7)
    }

    pub(super) fn is_ui_key(&self, key: u8) -> bool {
        self.ui_key() == Some(key)
    }

    pub(super) fn ui_lane(&self) -> Option<Lane> {
        Some(match self.ui_key()? {
            1 => Lane::Key1,
            2 => Lane::Key2,
            3 => Lane::Key3,
            4 => Lane::Key4,
            5 => Lane::Key5,
            6 => Lane::Key6,
            _ => Lane::Key7,
        })
    }

    /// 選曲の決定。7K/14K は各側の奇数鍵、9K は KEY5 / KEY7。
    pub(super) fn is_enter(&self) -> bool {
        if self.nine_key {
            return matches!(self.nine_key_lane(), Some(Lane::Key5 | Lane::Key7));
        }
        self.ui_key().is_some_and(|key| key % 2 == 1)
    }

    /// 選曲の戻る。E2、7K/14K は各側の偶数鍵、9K は KEY3。
    pub(super) fn is_back(&self) -> bool {
        if self.is_e2() {
            return true;
        }
        if self.nine_key {
            return self.nine_key_lane() == Some(Lane::Key3);
        }
        self.ui_key().is_some_and(|key| key % 2 == 0)
    }

    /// 選曲スクラッチの方向。
    pub(super) fn scene_scratch(&self) -> Option<SceneScratch> {
        self.select_lane().and_then(|lane| lane.scene_scratch)
    }

    pub(super) fn is_scratch_up(&self) -> bool {
        matches!(self.scene_scratch(), Some(SceneScratch::Up | SceneScratch::Both))
    }

    pub(super) fn is_scratch_down(&self) -> bool {
        matches!(self.scene_scratch(), Some(SceneScratch::Down | SceneScratch::Both))
    }

    /// 配置変更に使う側と向き。KEY1 は次、KEY2 は前。
    pub(super) fn arrange_input(&self) -> Option<(PlayerSide, bool)> {
        let lane = self.select_lane()?;
        let forward = match lane.side_key()? {
            1 => true,
            2 => false,
            _ => return None,
        };
        Some((if self.nine_key { PlayerSide::P1 } else { lane.side }, forward))
    }

    /// オプション操作用の 14K 換算レーン。7K/14K は 2P 側を KEY8〜KEY14、9K は KEY1〜KEY7 だけ。
    pub(super) fn option_lane(&self) -> Option<Lane> {
        const LANES: [Lane; 14] = [
            Lane::Key1,
            Lane::Key2,
            Lane::Key3,
            Lane::Key4,
            Lane::Key5,
            Lane::Key6,
            Lane::Key7,
            Lane::Key8,
            Lane::Key9,
            Lane::Key10,
            Lane::Key11,
            Lane::Key12,
            Lane::Key13,
            Lane::Key14,
        ];
        let lane = self.select_lane()?;
        let key = usize::from(self.ui_key()?);
        let offset = if !self.nine_key && lane.side == PlayerSide::P2 { 7 } else { 0 };
        LANES.get(key - 1 + offset).copied()
    }

    /// 9K の上下移動 (KEY4 / KEY6)。
    pub(super) fn nine_key_move(&self) -> Option<i32> {
        match self.nine_key_lane()? {
            Lane::Key4 => Some(-1),
            Lane::Key6 => Some(1),
            _ => None,
        }
    }

    /// 9K の5行移動 (KEY1 / KEY9)。
    pub(super) fn nine_key_jump(&self) -> Option<i32> {
        match self.nine_key_lane()? {
            Lane::Key1 => Some(-1),
            Lane::Key9 => Some(1),
            _ => None,
        }
    }

    /// TARGET の前後 (9K KEY8 / KEY9)。スクラッチは [`Self::scene_scratch`] で扱う。
    pub(super) fn nine_key_target(&self) -> Option<i32> {
        match self.nine_key_lane()? {
            Lane::Key8 => Some(-1),
            Lane::Key9 => Some(1),
            _ => None,
        }
    }

    /// 120ms 長押しで G-BATTLE の相手選択を開く入力 (KEY4、9K は KEY8)。
    pub(super) fn is_ir_battle_hold(&self) -> bool {
        if self.nine_key { self.nine_key_lane() == Some(Lane::Key8) } else { self.is_ui_key(4) }
    }

    /// 旧3パネル方式のアシストパネルの event ID。
    pub(super) fn legacy_assist_event(&self) -> Option<i32> {
        self.ui_key().map(|key| 300 + i32::from(key))
    }

    /// 設定画面の操作。
    ///
    /// - キーボード既定: Enter / Space / → で決定、← / Esc で戻る、↑ で増加、↓ で減少。
    /// - 7K/14K: 各側の奇数鍵で決定、偶数鍵と E2 で戻る、Scratch Down で増加、Up で減少。
    /// - 9K: KEY5 / KEY7 で決定、KEY3 で戻る、KEY6 で増加、KEY4 で減少。9K のレーンに
    ///   割り当てた入力はレーンの意味を優先し、E2 として扱わない。
    pub(super) fn settings_input(&self) -> SettingsInput {
        let mut input = SettingsInput::default();
        match self.standard {
            Some(StandardKey::Enter | StandardKey::Space | StandardKey::ArrowRight) => {
                input.confirm = true;
            }
            Some(StandardKey::ArrowLeft | StandardKey::Escape) => input.back = true,
            Some(StandardKey::ArrowUp) => input.increase = true,
            Some(StandardKey::ArrowDown) => input.decrease = true,
            _ => {}
        }
        if self.nine_key {
            match self.lane.map(|lane| lane.lane) {
                Some(Lane::Key3) => input.back = true,
                Some(Lane::Key4) => input.decrease = true,
                Some(Lane::Key5 | Lane::Key7) => input.confirm = true,
                Some(Lane::Key6) => input.increase = true,
                Some(_) => {}
                None => input.back |= self.is_e2(),
            }
            return input;
        }
        input.back |= self.is_e2();
        if let Some(lane) = self.lane {
            match lane.side_key() {
                Some(key) if key % 2 == 1 => input.confirm = true,
                Some(_) => input.back = true,
                None => match lane.scene_scratch {
                    Some(SceneScratch::Down) => input.increase = true,
                    Some(SceneScratch::Up) => input.decrease = true,
                    Some(SceneScratch::Both) => {
                        input.increase = true;
                        input.decrease = true;
                    }
                    None => {}
                },
            }
        }
        input
    }

    /// 詳細パネルの値変更方向。奇数鍵は＋、偶数鍵は−。
    pub(super) fn detail_value_direction(&self) -> Option<i32> {
        if self.nine_key {
            return None;
        }
        self.ui_key().map(|key| if key % 2 == 1 { 1 } else { -1 })
    }
}

/// レーンの binding 1件。
#[derive(Debug, Clone)]
pub(super) struct LaneEntry {
    device: BindingDevice,
    control: PhysicalControl,
    lane: Lane,
    scratch: Option<ScratchDirection>,
    scene_scratch: Option<SceneScratch>,
}

/// ある設定対象モードのレーン binding 表。
#[derive(Debug, Clone, Default)]
pub(super) struct LaneTable {
    entries: Vec<LaneEntry>,
}

impl LaneTable {
    pub(super) fn from_config(input: &ProfileInputConfig, mode: KeyMode) -> Self {
        let entries = resolve_play_bindings(input, mode).unwrap_or_default();
        Self::from_entries(&entries)
    }

    pub(super) fn from_entries(entries: &[BindingConfigEntry]) -> Self {
        let entries = entries
            .iter()
            .filter(|entry| !entry.control.is_empty())
            .filter_map(|entry| {
                let lane = entry.lane?;
                Some(LaneEntry {
                    device: BindingDevice::from_config(&entry.device)?,
                    control: control_from_config(&entry.device, &entry.control),
                    lane: crate::config::play::lane_from_config(lane),
                    scratch: play_scratch_direction(lane, entry),
                    scene_scratch: scene_scratch_direction(lane, entry),
                })
            })
            .collect();
        Self { entries }
    }

    /// 入力元のスロット専用 binding を優先し、無ければ共通 binding を使う。
    /// 他側の番号付き binding は候補にしない。
    fn find(&self, control: &PhysicalControl, slot: Option<PlayerSide>) -> Option<&LaneEntry> {
        let mut wildcard = None;
        for entry in &self.entries {
            if entry.control != *control || !entry.device.matches(control, slot) {
                continue;
            }
            if entry.device.is_slot_specific() {
                return Some(entry);
            }
            wildcard.get_or_insert(entry);
        }
        wildcard
    }

    fn find_where(
        &self,
        control: &PhysicalControl,
        slot: Option<PlayerSide>,
        accept: impl Fn(Lane) -> bool,
    ) -> Option<&LaneEntry> {
        let mut wildcard = None;
        for entry in &self.entries {
            if entry.control != *control
                || !entry.device.matches(control, slot)
                || !accept(entry.lane)
            {
                continue;
            }
            if entry.device.is_slot_specific() {
                return Some(entry);
            }
            wildcard.get_or_insert(entry);
        }
        wildcard
    }

    fn keyboard_controls(&self) -> impl Iterator<Item = &PhysicalControl> {
        self.entries
            .iter()
            .filter(|entry| entry.device == BindingDevice::Keyboard)
            .map(|entry| &entry.control)
    }
}

fn lane_side(lane: Lane) -> PlayerSide {
    if matches!(
        lane,
        Lane::Scratch2
            | Lane::Key8
            | Lane::Key9
            | Lane::Key10
            | Lane::Key11
            | Lane::Key12
            | Lane::Key13
            | Lane::Key14
    ) {
        PlayerSide::P2
    } else {
        PlayerSide::P1
    }
}

fn resolved_lane(entry: &LaneEntry, mode: KeyMode, side: PlayerSide) -> ResolvedLane {
    ResolvedLane {
        lane: entry.lane,
        mode,
        side,
        scratch: entry.scratch,
        scene_scratch: entry.scene_scratch,
    }
}

fn play_scratch_direction(
    lane: LaneConfig,
    entry: &BindingConfigEntry,
) -> Option<ScratchDirection> {
    if !matches!(lane, LaneConfig::Scratch | LaneConfig::Scratch2) {
        return None;
    }
    match entry.scratch {
        Some(ScratchDirectionConfig::Up) => Some(ScratchDirection::Up),
        Some(ScratchDirectionConfig::Down) => Some(ScratchDirection::Down),
        None => {
            let control = entry.control.as_str();
            if control.contains("ScratchUp") || control.ends_with('-') || control == "Button9" {
                Some(ScratchDirection::Up)
            } else if control.contains("ScratchDown")
                || control.ends_with('+')
                || control == "Button8"
            {
                Some(ScratchDirection::Down)
            } else {
                None
            }
        }
    }
}

/// 選曲移動のスクラッチ方向。明示タグを優先し、旧profileはコントロール名から推測する。
fn scene_scratch_direction(lane: LaneConfig, entry: &BindingConfigEntry) -> Option<SceneScratch> {
    if !matches!(lane, LaneConfig::Scratch | LaneConfig::Scratch2) {
        return None;
    }
    Some(match entry.scratch {
        Some(ScratchDirectionConfig::Up) => SceneScratch::Up,
        Some(ScratchDirectionConfig::Down) => SceneScratch::Down,
        None => {
            let control = entry.control.as_str();
            if crate::config::key_config::is_scratch_up_control(control)
                || matches!(control, "LShift" | "RShift")
            {
                SceneScratch::Up
            } else if crate::config::key_config::is_scratch_down_control(control)
                || matches!(control, "LControl" | "RControl")
            {
                SceneScratch::Down
            } else {
                SceneScratch::Both
            }
        }
    })
}

/// E1〜E4 と単発ショートカットの binding 表。
///
/// コントローラーの E1〜E4 などは、操作ごと・側ごとに次の規則で決める。
///
/// - `gamepad1` / `gamepad2` の entry が1件でもあれば、その側は個別設定で共通設定を上書きする。
///   `control = ""` の entry は「明示的な未割り当て」を表す。
/// - 個別設定がなければ共通設定 (`device = "gamepad"`) を継承する。
/// - 未割り当て機器は共通設定だけを使う。
#[derive(Debug, Clone, Default)]
pub(super) struct ActionTable {
    entries: Vec<ActionEntry>,
    /// `[side][action]` の個別設定有無。action は [`ActionTable::override_index`] の順。
    side_overrides: [Vec<InputActionConfig>; 2],
}

#[derive(Debug, Clone)]
struct ActionEntry {
    device: BindingDevice,
    control: PhysicalControl,
    action: InputActionConfig,
}

impl ActionTable {
    pub(super) fn from_config(input: &ProfileInputConfig) -> Self {
        let mut table = Self::default();
        for entry in &input.ui.bindings {
            let Some(action) = entry.action else { continue };
            let Some(device) = BindingDevice::from_config(&entry.device) else { continue };
            if let BindingDevice::ControllerSlot(side) = device {
                let overrides = &mut table.side_overrides[side.slot_index()];
                if !overrides.contains(&action) {
                    overrides.push(action);
                }
            }
            if entry.control.is_empty() {
                continue;
            }
            let controls = keyboard_aliases(device, &entry.control);
            for control in controls {
                table.entries.push(ActionEntry {
                    device,
                    control: control_from_config(&entry.device, &control),
                    action,
                });
            }
        }
        if let Some(legacy_start) = input.start_key.as_ref().filter(|key| !key.is_empty()) {
            table.push_keyboard_default(InputActionConfig::E1, legacy_start, true);
        }
        if !table.entries.iter().any(|entry| entry.action == InputActionConfig::E1) {
            table.push_keyboard_default(InputActionConfig::E1, "Q", true);
        }
        table
    }

    /// 選曲用の既定ショートカット (F8 / F9 / 0 / 5)。
    ///
    /// 設定されておらず、そのキーを E1〜E4 が使っていないときだけ追加する。
    pub(super) fn with_select_defaults(mut self) -> Self {
        for (action, default) in [
            (InputActionConfig::SelectFavoriteSong, "F8"),
            (InputActionConfig::SelectFavoriteChart, "F9"),
            (InputActionConfig::SelectDifficultyFilter, "0"),
            (InputActionConfig::SelectReplayPlay, "5"),
        ] {
            if self.entries.iter().any(|entry| entry.action == action) {
                continue;
            }
            let default_control = PhysicalControl::KeyboardKey(default.to_string());
            let used_by_e_action = self.entries.iter().any(|entry| {
                e_action_index(entry.action).is_some()
                    && entry.device == BindingDevice::Keyboard
                    && entry.control == default_control
            });
            if !used_by_e_action {
                self.push_keyboard_default(action, default, false);
            }
        }
        self
    }

    fn push_keyboard_default(&mut self, action: InputActionConfig, control: &str, unique: bool) {
        for alias in keyboard_aliases(BindingDevice::Keyboard, control) {
            let control = PhysicalControl::KeyboardKey(alias);
            if unique
                && self.entries.iter().any(|entry| {
                    entry.action == action
                        && entry.device == BindingDevice::Keyboard
                        && entry.control == control
                })
            {
                continue;
            }
            self.entries.push(ActionEntry { device: BindingDevice::Keyboard, control, action });
        }
    }

    fn side_overrides(&self, side: PlayerSide, action: InputActionConfig) -> bool {
        self.side_overrides[side.slot_index()].contains(&action)
    }

    /// entry がこの入力元に有効か。側の個別設定があれば共通設定を使わない。
    fn entry_applies(
        &self,
        entry: &ActionEntry,
        control: &PhysicalControl,
        slot: Option<PlayerSide>,
    ) -> bool {
        if entry.control != *control || !entry.device.matches(control, slot) {
            return false;
        }
        match (entry.device, slot) {
            (BindingDevice::AnyController, Some(side)) => !self.side_overrides(side, entry.action),
            _ => true,
        }
    }

    /// E1〜E4 は E1 → E4 の順で最初に一致したもの。
    pub(super) fn e_action(
        &self,
        control: &PhysicalControl,
        slot: Option<PlayerSide>,
    ) -> Option<InputActionConfig> {
        E_ACTIONS.into_iter().find(|action| {
            self.entries
                .iter()
                .any(|entry| entry.action == *action && self.entry_applies(entry, control, slot))
        })
    }

    pub(super) fn shortcuts(
        &self,
        control: &PhysicalControl,
        slot: Option<PlayerSide>,
    ) -> ShortcutSet {
        let mut set = ShortcutSet::default();
        for entry in &self.entries {
            if self.entry_applies(entry, control, slot) {
                set.insert(entry.action);
            }
        }
        set
    }

    /// プレイ中の判定用。E4 は呼び出し側で E1 と同一視する。
    pub(super) fn has_action(
        &self,
        control: &PhysicalControl,
        slot: Option<PlayerSide>,
        action: InputActionConfig,
    ) -> bool {
        self.entries
            .iter()
            .any(|entry| entry.action == action && self.entry_applies(entry, control, slot))
    }

    fn keyboard_controls_for(&self, action: InputActionConfig) -> impl Iterator<Item = &str> {
        self.entries.iter().filter_map(move |entry| match (&entry.control, entry.device) {
            (PhysicalControl::KeyboardKey(name), BindingDevice::Keyboard)
                if entry.action == action =>
            {
                Some(name.as_str())
            }
            _ => None,
        })
    }
}

/// キーボードの数字キーはテンキーも同じ操作として扱う。
fn keyboard_aliases(device: BindingDevice, control: &str) -> Vec<String> {
    let mut controls = vec![control.to_string()];
    if device == BindingDevice::Keyboard {
        let alias = match control {
            "0" | "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" => {
                Some(format!("Numpad{control}"))
            }
            _ => control
                .strip_prefix("Numpad")
                .filter(|digit| digit.len() == 1 && digit.chars().all(|c| c.is_ascii_digit()))
                .map(str::to_string),
        };
        controls.extend(alias);
    }
    controls
}

/// 選曲・決定・リザルト・設定画面の入力 resolver。
///
/// 7K/14K 選曲入力では次の優先順位でレーンを解決する。
///
/// - キーボード: 7K → 14K の2P側 (7K のキーと同じキーは除外)。
/// - 1P のコントローラー: 7K → 14K の `gamepad1` / 共通設定。
/// - 2P のコントローラー: 14K の `gamepad2` / 共通設定 → 7K。
/// - 未割り当てのコントローラー: 7K → 14K の共通設定。
///
/// いずれも同じ入力元から届いた同じイベントだけで解決し、他側の番号付き binding は
/// 候補にしない。9K 選曲入力は 9K 設定だけを使い、KEY8 / KEY9 を2P側として扱わない。
pub(super) struct SceneInputResolver {
    nine_key: bool,
    primary: LaneTable,
    double: LaneTable,
    actions: ActionTable,
    key_hint: String,
    option_hint: String,
}

impl SceneInputResolver {
    pub(super) fn from_profile(input: &ProfileInputConfig) -> Self {
        let nine_key = input.select_input_mode == SelectInputModeConfig::Key9;
        let actions = ActionTable::from_config(input).with_select_defaults();
        let (primary, double) = if nine_key {
            (LaneTable::from_config(input, KeyMode::K9), LaneTable::default())
        } else {
            (
                LaneTable::from_config(input, KeyMode::K7),
                LaneTable::from_config(input, KeyMode::K14),
            )
        };
        let (key_hint, option_hint) =
            if nine_key { hints_9k(&primary, &actions) } else { hints_7k(&primary, &actions) };
        Self { nine_key, primary, double, actions, key_hint, option_hint }
    }

    pub(super) fn key_hint(&self) -> &str {
        &self.key_hint
    }

    pub(super) fn option_hint(&self) -> &str {
        &self.option_hint
    }

    /// 物理入力を解決する。入力ごとに設定を読み直さず、構築済みの表だけを参照する。
    pub(super) fn resolve_control(
        &self,
        device: DeviceId,
        control: &PhysicalControl,
        slots: GamepadSlotMap,
    ) -> ResolvedControl {
        let slot = controller_slot(device, control, slots);
        let lane = self.resolve_lane(control, slot);
        let e_action = self.actions.e_action(control, slot);
        let shortcuts = self.actions.shortcuts(control, slot);
        let standard = match control {
            PhysicalControl::KeyboardKey(name) => StandardKey::from_keyboard_name(name),
            _ => None,
        };
        ResolvedControl { slot, nine_key: self.nine_key, lane, e_action, shortcuts, standard }
    }

    fn resolve_lane(
        &self,
        control: &PhysicalControl,
        slot: Option<PlayerSide>,
    ) -> Option<ResolvedLane> {
        if self.nine_key {
            return self
                .primary
                .find(control, slot)
                .map(|entry| resolved_lane(entry, KeyMode::K9, PlayerSide::P1));
        }
        let from_7k = || {
            self.primary
                .find(control, slot)
                .map(|entry| resolved_lane(entry, KeyMode::K7, slot.unwrap_or(PlayerSide::P1)))
        };
        let from_14k = || match control {
            // キーボードは7Kを優先し、14Kからは2P側だけを補う。
            PhysicalControl::KeyboardKey(_) => self
                .double
                .find_where(control, slot, |lane| lane_side(lane) == PlayerSide::P2)
                .filter(|_| !self.primary.keyboard_controls().any(|primary| primary == control))
                .map(|entry| resolved_lane(entry, KeyMode::K14, PlayerSide::P2)),
            _ => self
                .double
                .find(control, slot)
                .map(|entry| resolved_lane(entry, KeyMode::K14, lane_side(entry.lane))),
        };
        match (control, slot) {
            (PhysicalControl::GamepadButton(_), Some(PlayerSide::P2)) => {
                from_14k().or_else(from_7k)
            }
            _ => from_7k().or_else(from_14k),
        }
    }
}

/// 入力元がコントローラーなら論理スロットを返す。
pub(super) fn controller_slot(
    device: DeviceId,
    control: &PhysicalControl,
    slots: GamepadSlotMap,
) -> Option<PlayerSide> {
    match control {
        PhysicalControl::GamepadButton(_) => {
            slots.player_for_device(device).and_then(PlayerSide::from_player_index)
        }
        _ => None,
    }
}

fn first_keyboard_lane(table: &LaneTable, lane: Lane, primary_only: bool) -> Vec<String> {
    table
        .entries
        .iter()
        .filter(|entry| entry.device == BindingDevice::Keyboard && entry.lane == lane)
        .filter_map(|entry| match &entry.control {
            PhysicalControl::KeyboardKey(name) => Some(name.clone()),
            _ => None,
        })
        .take(if primary_only { 1 } else { usize::MAX })
        .collect()
}

fn keyboard_lane_hint(table: &LaneTable, actions: &ActionTable, lanes: &[Lane]) -> Vec<String> {
    let e_keys: Vec<&str> =
        E_ACTIONS.iter().flat_map(|action| actions.keyboard_controls_for(*action)).collect();
    lanes
        .iter()
        .flat_map(|lane| first_keyboard_lane(table, *lane, false))
        .filter(|control| !e_keys.contains(&control.as_str()))
        .collect()
}

fn first_action_key(actions: &ActionTable, action: InputActionConfig) -> Option<String> {
    actions.keyboard_controls_for(action).next().map(str::to_string)
}

fn shortcut_hints(actions: &ActionTable) -> impl Fn(InputActionConfig) -> String + '_ {
    move |action| first_action_key(actions, action).unwrap_or_else(|| "-".to_string())
}

fn hints_7k(primary: &LaneTable, actions: &ActionTable) -> (String, String) {
    let lane_hint = |lanes: &[Lane]| keyboard_lane_hint(primary, actions, lanes);
    let enter = lane_hint(&[Lane::Key1, Lane::Key3, Lane::Key5, Lane::Key7]);
    let back = lane_hint(&[Lane::Key2, Lane::Key4, Lane::Key6]);
    let enter_str = if enter.is_empty() { String::new() } else { format!("/{}", enter.join("/")) };
    let back_str = if back.is_empty() {
        first_action_key(actions, InputActionConfig::E2)
            .map(|key| format!("/{key}"))
            .unwrap_or_default()
    } else {
        format!("/{}", back.join("/"))
    };
    let key2_str = lane_hint(&[Lane::Key2]).into_iter().next().unwrap_or_else(|| "Key2".into());
    let start_str = first_action_key(actions, InputActionConfig::E1).unwrap_or_else(|| "Q".into());
    let key_hint =
        format!("UP DOWN  RIGHT{enter_str}:ENTER  LEFT{back_str}:BACK  ENTER {start_str}");
    let bga_str = lane_hint(&[Lane::Key1]).into_iter().next().unwrap_or_else(|| "?".into());
    let hint = shortcut_hints(actions);
    let option_hint = format!(
        "F1 MENU  {}:FOLDER/HASH  {}:RELOAD  {}:AUTOPLAY  {}:IR  {}:KEY CONFIG  {}:SHOT  \
         {}:MODE  {}:SORT  {}:LN  {}:CYCLE  {}:SAME  {}:RIVAL  {}:TEXT   \
         {start_str}:PLAY OPT  BACK:E2 OPT  {start_str}+BACK:DETAIL OPT  \
         {start_str}+K1/K2:1P ARR  {start_str}+2P K1/K2:2P ARR  {start_str}+K3/K4:GAUGE  \
         {start_str}+K5:HS-FIX  {start_str}+K6:DP OPT  {start_str}+K7:AUTOPLAY  \
         BACK:DETAIL OPTIONS  \
         {start_str}+BACK+{key2_str}:GAS  {start_str}+UP/DOWN:TARGET  {start_str}+{bga_str}:BGA  {start_str}+K4/K6:GREEN  {start_str}+K5/K7:TIMING  {start_str}+1..4:REPLAY  N5:REPLAY",
        hint(InputActionConfig::SelectOpenFolder),
        hint(InputActionConfig::SelectReload),
        hint(InputActionConfig::SelectAutoplayFolder),
        hint(InputActionConfig::SelectOpenIr),
        hint(InputActionConfig::SelectOpenKeyConfig),
        hint(InputActionConfig::Screenshot),
        hint(InputActionConfig::SelectModeFilter),
        hint(InputActionConfig::SelectSort),
        hint(InputActionConfig::SelectLnMode),
        hint(InputActionConfig::SelectReplayCycle),
        hint(InputActionConfig::SelectSameFolder),
        hint(InputActionConfig::SelectRivalCycle),
        hint(InputActionConfig::SelectOpenDocuments),
    );
    (key_hint, option_hint)
}

fn hints_9k(primary: &LaneTable, actions: &ActionTable) -> (String, String) {
    let lane_hint = |lane: Lane| keyboard_lane_hint(primary, actions, &[lane]);
    let first = |lane: Lane, fallback: &str| {
        lane_hint(lane).into_iter().next().unwrap_or_else(|| fallback.to_string())
    };
    let start_str = first_action_key(actions, InputActionConfig::E1).unwrap_or_else(|| "Q".into());
    let mut enter = lane_hint(Lane::Key5);
    for control in lane_hint(Lane::Key7) {
        if !enter.contains(&control) {
            enter.push(control);
        }
    }
    let enter_str = if enter.is_empty() { String::new() } else { format!("/{}", enter.join("/")) };
    let back_str = lane_hint(Lane::Key3)
        .into_iter()
        .next()
        .or_else(|| first_action_key(actions, InputActionConfig::E2))
        .unwrap_or_else(|| "KEY3".to_string());
    let key_hint = format!(
        "UP {}  DOWN {}  UP5 {}  DOWN5 {}  RIGHT{enter_str}:ENTER  LEFT/{back_str}:BACK  ENTER {start_str}",
        first(Lane::Key4, "KEY4"),
        first(Lane::Key6, "KEY6"),
        first(Lane::Key1, "KEY1"),
        first(Lane::Key9, "KEY9"),
    );
    let bga_str = first(Lane::Key1, "?");
    let hint = shortcut_hints(actions);
    let option_hint = format!(
        "F1 MENU  {}:FOLDER/HASH  {}:RELOAD  {}:AUTOPLAY  {}:IR  {}:KEY CONFIG  {}:SHOT  \
         {}:MODE  {}:SORT  {}:LN  {}:CYCLE  {}:SAME  {}:RIVAL  {}:TEXT   \
         {start_str}:PLAY OPT  BACK:E2 OPT  {start_str}+BACK:DETAIL OPT  \
         {start_str}+K1/K2:1P ARR  {start_str}+K3:GAUGE  {start_str}+K5:HS-FIX  \
         BACK:DETAIL OPTIONS  \
         {start_str}+K8/K9:TARGET  {start_str}+{bga_str}:BGA  {start_str}+K4/K6:GREEN  {start_str}+K5/K7:TIMING  {start_str}+1..4:REPLAY  N5:REPLAY",
        hint(InputActionConfig::SelectOpenFolder),
        hint(InputActionConfig::SelectReload),
        hint(InputActionConfig::SelectAutoplayFolder),
        hint(InputActionConfig::SelectOpenIr),
        hint(InputActionConfig::SelectOpenKeyConfig),
        hint(InputActionConfig::Screenshot),
        hint(InputActionConfig::SelectModeFilter),
        hint(InputActionConfig::SelectSort),
        hint(InputActionConfig::SelectLnMode),
        hint(InputActionConfig::SelectReplayCycle),
        hint(InputActionConfig::SelectSameFolder),
        hint(InputActionConfig::SelectRivalCycle),
        hint(InputActionConfig::SelectOpenDocuments),
    );
    (key_hint, option_hint)
}

/// 解決済みの入力イベント。出自 (デバイス・物理入力・取得時刻) と意味を保持する。
///
/// 生成は [`resolve_input`] だけが行う。操作層はこの型を受け取り、`scene()` /
/// `play()` の意味から Action を決める。
pub(super) struct ResolvedInput {
    source: Option<InputSource>,
    pressed: bool,
    repeat: bool,
    timestamp: DeviceTimestamp,
    synthesized_analog_axis: bool,
    scene: ResolvedControl,
    play: Option<ResolvedControl>,
}

impl ResolvedInput {
    pub(super) fn source(&self) -> Option<&InputSource> {
        self.source.as_ref()
    }

    pub(super) fn device(&self) -> Option<DeviceId> {
        self.source.as_ref().map(|source| source.device)
    }

    pub(super) fn physical(&self) -> Option<&PhysicalControl> {
        self.source.as_ref().map(|source| &source.control)
    }

    pub(super) fn is_keyboard(&self) -> bool {
        matches!(self.physical(), Some(PhysicalControl::KeyboardKey(_)))
    }

    pub(super) fn is_controller(&self) -> bool {
        matches!(self.physical(), Some(PhysicalControl::GamepadButton(_)))
    }

    pub(super) fn pressed(&self) -> bool {
        self.pressed
    }

    pub(super) fn repeat(&self) -> bool {
        self.repeat
    }

    pub(super) fn timestamp(&self) -> DeviceTimestamp {
        self.timestamp
    }

    pub(super) fn synthesized_analog_axis(&self) -> bool {
        self.synthesized_analog_axis
    }

    /// 選曲・決定・リザルト・設定画面の意味。
    pub(super) fn scene(&self) -> &ResolvedControl {
        &self.scene
    }

    /// 現在のプレイセッションの設定対象モードでの意味。プレイ文脈が無ければ `None`。
    pub(super) fn play(&self) -> Option<&ResolvedControl> {
        self.play.as_ref()
    }

    /// 合成アナログ Press/Release のうち、tick 側でスクロールを扱うもの。
    /// デジタル化した軸 (tick なし) は通常のボタンとして扱う。
    pub(super) fn uses_analog_scroll(&self) -> bool {
        self.synthesized_analog_axis && (self.scene.is_scratch_up() || self.scene.is_scratch_down())
    }
}

/// 物理入力を解決済み入力へ変換する唯一の入口。
pub(super) fn resolve_input(
    scene: &SceneInputResolver,
    play: Option<&super::PlayOptionInput>,
    slots: GamepadSlotMap,
    event: ControlInputEvent,
) -> ResolvedInput {
    let ControlInputEvent {
        device,
        physical,
        pressed,
        repeat,
        timestamp,
        synthesized_analog_axis,
        ..
    } = event;
    let (scene_control, play_control) = match physical.as_ref() {
        Some(control) => (
            scene.resolve_control(device, control, slots),
            play.map(|play| play.resolve_control(device, control)),
        ),
        None => (ResolvedControl::unbound(), play.map(|_| ResolvedControl::unbound())),
    };
    ResolvedInput {
        source: physical.map(|control| InputSource { device, control }),
        pressed,
        repeat,
        timestamp,
        synthesized_analog_axis,
        scene: scene_control,
        play: play_control,
    }
}

/// アナログ tick の解決結果。合算前に各 tick のデバイス・方向を解決する。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct ResolvedAxisTick {
    /// 選曲スクロールの寄与。Next 方向を正とする。未割り当て軸は `None`。
    pub(super) scene_scroll: Option<i32>,
    /// プレイ用 SCR 設定での寄与。Scratch Down を正とする。未割り当て軸は `None`。
    pub(super) play_scroll: Option<i32>,
}

/// アナログ tick を、方向付きの軸ボタン (`Axis1+` 等) として解決する。
pub(super) fn resolve_axis_tick(
    scene: &SceneInputResolver,
    play: Option<&super::PlayOptionInput>,
    slots: GamepadSlotMap,
    device: DeviceId,
    axis: &str,
    ticks: i32,
) -> ResolvedAxisTick {
    if ticks == 0 {
        return ResolvedAxisTick { scene_scroll: None, play_scroll: None };
    }
    let control =
        PhysicalControl::GamepadButton(format!("{axis}{}", if ticks > 0 { "+" } else { "-" }));
    let resolved = scene.resolve_control(device, &control, slots);
    let scene_scroll = if resolved.is_scratch_down() {
        Some(ticks.abs())
    } else if resolved.is_scratch_up() {
        Some(-ticks.abs())
    } else {
        None
    };
    let play_scroll = play
        .and_then(|play| play.resolve_entry(device, &control))
        .and_then(|binding| binding.scratch_direction)
        .map(|direction| match direction {
            ScratchDirection::Down => ticks.abs(),
            ScratchDirection::Up => -ticks.abs(),
        });
    ResolvedAxisTick { scene_scroll, play_scroll }
}

impl super::WinitApp {
    /// 現在の画面・プレイセッション・機器割り当てで物理入力を解決する。
    pub(super) fn resolve_control_event(&self, event: ControlInputEvent) -> ResolvedInput {
        resolve_input(
            &self.select.select_keys,
            self.play.play_option_input.as_ref(),
            self.input.gamepad_slots,
            event,
        )
    }

    /// 押下中の入力元を、現在の設定で選曲側の意味へ解決する。
    pub(super) fn resolve_scene_source(
        &self,
        device: DeviceId,
        control: &PhysicalControl,
    ) -> ResolvedControl {
        self.select.select_keys.resolve_control(device, control, self.input.gamepad_slots)
    }

    /// 押下中のすべての入力元の解決結果。
    pub(super) fn pressed_scene_controls(&self) -> impl Iterator<Item = ResolvedControl> + '_ {
        self.input
            .pressed_sources()
            .map(|(device, control)| self.resolve_scene_source(device, control))
    }

    /// アナログ tick を現在の設定で解決する。
    pub(super) fn resolve_gamepad_axis_tick(
        &self,
        device: DeviceId,
        axis: &str,
        ticks: i32,
    ) -> ResolvedAxisTick {
        resolve_axis_tick(
            &self.select.select_keys,
            self.play.play_option_input.as_ref(),
            self.input.gamepad_slots,
            device,
            axis,
            ticks,
        )
    }
}

/// 既存テスト向け: プレイ文脈なし・機器未割り当てで、指定 resolver を通して解決する。
#[cfg(test)]
pub(super) fn resolve_scene_for_test(
    scene: &SceneInputResolver,
    event: ControlInputEvent,
) -> ResolvedInput {
    resolve_input(scene, None, GamepadSlotMap::default(), event)
}

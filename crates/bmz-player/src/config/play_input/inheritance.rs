use super::*;

#[derive(Debug, PartialEq, Eq)]
pub enum InheritError {
    Disallowed { child: KeyMode, parent: KeyMode },
    UnknownKey { key: String },
    Cycle { chain: Vec<KeyMode> },
    RootWithInherit { mode: KeyMode },
}

impl fmt::Display for InheritError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Disallowed { child, parent } => {
                write!(f, "inherit from {} to {} is not allowed", parent.as_str(), child.as_str())
            }
            Self::UnknownKey { key } => write!(f, "unknown play map key: {key}"),
            Self::Cycle { chain } => write!(f, "inherit cycle detected: {chain:?}"),
            Self::RootWithInherit { mode } => {
                write!(f, "root mode {} cannot declare inherit", mode.as_str())
            }
        }
    }
}

impl std::error::Error for InheritError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InheritRule {
    FilterOnly,
    Remap(&'static [(Lane, Lane)]),
}

const REMAP_4K: [(Lane, Lane); 4] = [
    (Lane::Key1, Lane::Key1),
    (Lane::Key2, Lane::Key2),
    (Lane::Key3, Lane::Key4),
    (Lane::Key4, Lane::Key5),
];

const REMAP_6K: [(Lane, Lane); 6] = [
    (Lane::Key1, Lane::Key1),
    (Lane::Key2, Lane::Key2),
    (Lane::Key3, Lane::Key3),
    (Lane::Key4, Lane::Key5),
    (Lane::Key5, Lane::Key6),
    (Lane::Key6, Lane::Key7),
];

fn implicit_inherit(child: KeyMode) -> Option<KeyMode> {
    match child {
        KeyMode::K5 | KeyMode::K4 | KeyMode::K6 => Some(KeyMode::K7),
        KeyMode::K10 => Some(KeyMode::K14),
        KeyMode::K7 | KeyMode::K8 | KeyMode::K14 | KeyMode::K9 => None,
    }
}

fn inherit_rule(child: KeyMode, parent: KeyMode) -> Option<InheritRule> {
    match (child, parent) {
        (KeyMode::K5, KeyMode::K7) | (KeyMode::K10, KeyMode::K14) | (KeyMode::K8, KeyMode::K7) => {
            Some(InheritRule::FilterOnly)
        }
        (KeyMode::K4, KeyMode::K7) | (KeyMode::K4, KeyMode::K5) => {
            Some(InheritRule::Remap(&REMAP_4K))
        }
        (KeyMode::K6, KeyMode::K7) => Some(InheritRule::Remap(&REMAP_6K)),
        _ => None,
    }
}

fn is_root_mode(mode: KeyMode) -> bool {
    matches!(mode, KeyMode::K7 | KeyMode::K14 | KeyMode::K9)
}

/// profile 内の明示 inherit 宣言を検証する。
pub fn validate_play_inherit_config(input: &ProfileInputConfig) -> Result<(), InheritError> {
    for (key, config) in &input.play {
        let Some(child) = KeyMode::from_play_map_key(key) else {
            continue;
        };
        if let Some(inherit_key) = config.inherit.as_deref() {
            if is_root_mode(child) {
                return Err(InheritError::RootWithInherit { mode: child });
            }
            let parent = KeyMode::from_play_map_key(inherit_key)
                .ok_or_else(|| InheritError::UnknownKey { key: inherit_key.to_string() })?;
            inherit_rule(child, parent).ok_or(InheritError::Disallowed { child, parent })?;
        }
    }
    Ok(())
}

pub fn lane_binding_for_key_mode(
    input: &ProfileInputConfig,
    key_mode: KeyMode,
) -> Result<LaneBinding, InheritError> {
    lane_binding_for_key_mode_with_slots(input, key_mode, GamepadSlotMap::default())
}

pub fn lane_binding_for_key_mode_with_slots(
    input: &ProfileInputConfig,
    key_mode: KeyMode,
    slots: GamepadSlotMap,
) -> Result<LaneBinding, InheritError> {
    let bindings = resolve_play_bindings(input, key_mode)?;
    Ok(LaneBinding {
        entries: bindings
            .into_iter()
            .filter_map(|entry| {
                let lane = entry.lane?;
                let lane_value = lane_from_config(lane);
                if !key_mode.active_lanes().contains(&lane_value) {
                    return None;
                }
                Some(BindingEntry {
                    device: binding_device_from_config(&entry.device, slots)?,
                    control: control_from_config(&entry.device, &entry.control),
                    lane: lane_value,
                    scratch_direction: scratch_direction_from_binding(lane, &entry),
                })
            })
            .collect(),
    })
}

/// プレイ中の E1/E2 + Scratch 操作用 binding を返す。
///
/// 4K / 6K / 8K / 9K は譜面レーンとして Scratch を持たないため、通常の
/// gameplay binding へ混ぜず、7K の Scratch 設定だけを option 操作用に借りる。
/// mode 側に Scratch binding が明示されている場合はそちらを優先する。
pub fn lane_binding_for_play_option_scratch_with_slots(
    input: &ProfileInputConfig,
    key_mode: KeyMode,
    slots: GamepadSlotMap,
) -> Result<LaneBinding, InheritError> {
    let mut bindings: Vec<_> = resolve_play_bindings(input, key_mode)?
        .into_iter()
        .filter(|entry| matches!(entry.lane, Some(LaneConfig::Scratch | LaneConfig::Scratch2)))
        .collect();
    if bindings.is_empty()
        && matches!(key_mode, KeyMode::K4 | KeyMode::K6 | KeyMode::K8 | KeyMode::K9)
    {
        bindings = resolve_play_bindings(input, KeyMode::K7)?
            .into_iter()
            .filter(|entry| entry.lane == Some(LaneConfig::Scratch))
            .collect();
    }

    Ok(LaneBinding {
        entries: bindings
            .into_iter()
            .filter_map(|entry| {
                let lane = entry.lane?;
                Some(BindingEntry {
                    device: binding_device_from_config(&entry.device, slots)?,
                    control: control_from_config(&entry.device, &entry.control),
                    lane: lane_from_config(lane),
                    scratch_direction: scratch_direction_from_binding(lane, &entry),
                })
            })
            .collect(),
    })
}

fn scratch_direction_from_binding(
    lane: LaneConfig,
    entry: &BindingConfigEntry,
) -> Option<ScratchDirection> {
    if !matches!(lane, LaneConfig::Scratch | LaneConfig::Scratch2) {
        return None;
    }
    match entry.scratch {
        Some(ScratchDirectionConfig::Up) => Some(ScratchDirection::Up),
        Some(ScratchDirectionConfig::Down) => Some(ScratchDirection::Down),
        None => infer_scratch_direction_from_control(&entry.control),
    }
}

fn infer_scratch_direction_from_control(control: &str) -> Option<ScratchDirection> {
    if control.contains("ScratchUp") || control.ends_with('-') || control == "Button9" {
        Some(ScratchDirection::Up)
    } else if control.contains("ScratchDown") || control.ends_with('+') || control == "Button8" {
        Some(ScratchDirection::Down)
    } else {
        None
    }
}

/// 設定対象モードの binding を継承込みで返す。
///
/// 明示的な未割り当ての marker (`control = ""`) は含めない。
pub fn resolve_play_bindings(
    input: &ProfileInputConfig,
    key_mode: KeyMode,
) -> Result<Vec<BindingConfigEntry>, InheritError> {
    let mut bindings = resolve_play_bindings_with_markers(input, key_mode)?;
    bindings.retain(|entry| !entry.control.is_empty());
    Ok(bindings)
}

/// [`resolve_play_bindings`] と同じだが、明示的な未割り当ての marker (`control = ""`) を残す。
///
/// marker はコントローラーのレーン単位で「設定なし (継承)」と「明示的な未割り当て」を
/// 区別するために使う。設定の書き換えと、選曲入力の 7K / 14K 間のフォールバック判定だけが読む。
pub fn resolve_play_bindings_with_markers(
    input: &ProfileInputConfig,
    key_mode: KeyMode,
) -> Result<Vec<BindingConfigEntry>, InheritError> {
    let mut chain = Vec::new();
    resolve_play_bindings_inner(input, key_mode, &mut chain, &mut HashSet::new())
}

/// プレイ中の E1/E2 + 鍵盤操作に使う論理レーンの方向を返す。
///
/// 8K だけは profile のレーン別 override を優先し、それ以外はモード既定を使う。
/// Scratchと9KのKEY8/9はレーンカバー操作なのでここでは方向を返さない。
pub fn hispeed_direction_for_lane(
    input: &ProfileInputConfig,
    key_mode: KeyMode,
    lane: Lane,
) -> Option<HispeedDirectionConfig> {
    let default = default_hispeed_direction_for_lane(key_mode, lane)?;
    if key_mode != KeyMode::K8 {
        return Some(default);
    }
    input
        .play
        .get(key_mode.play_map_key())
        .and_then(|config| config.hispeed.get(&lane_to_config(lane)))
        .copied()
        .or(Some(default))
}

/// 8K の1レーン分の方向 override を更新する。
///
/// 既定値と同じ方向は map から取り除き、旧 profile と同じ省略形へ戻す。
pub fn set_eight_key_hispeed_direction(
    input: &mut ProfileInputConfig,
    lane: LaneConfig,
    direction: HispeedDirectionConfig,
) -> bool {
    let lane = lane_from_config(lane);
    let Some(default) = default_hispeed_direction_for_lane(KeyMode::K8, lane) else {
        return false;
    };
    let lane_config = lane_to_config(lane);
    let current = hispeed_direction_for_lane(input, KeyMode::K8, lane);
    if current == Some(direction) {
        return false;
    }

    if direction == default {
        if let Some(config) = input.play.get_mut(KeyMode::K8.play_map_key()) {
            config.hispeed.remove(&lane_config);
        }
    } else {
        input
            .play
            .entry(KeyMode::K8.play_map_key().to_string())
            .or_default()
            .hispeed
            .insert(lane_config, direction);
    }
    true
}

pub const fn default_hispeed_direction_for_lane(
    key_mode: KeyMode,
    lane: Lane,
) -> Option<HispeedDirectionConfig> {
    use HispeedDirectionConfig::{Down, Up};

    match key_mode {
        KeyMode::K4 => match lane {
            Lane::Key1 | Lane::Key4 => Some(Down),
            Lane::Key2 | Lane::Key3 => Some(Up),
            _ => None,
        },
        KeyMode::K5 => match lane {
            Lane::Key1 | Lane::Key3 | Lane::Key5 => Some(Down),
            Lane::Key2 | Lane::Key4 => Some(Up),
            _ => None,
        },
        KeyMode::K6 => match lane {
            Lane::Key1 | Lane::Key3 | Lane::Key4 | Lane::Key6 => Some(Down),
            Lane::Key2 | Lane::Key5 => Some(Up),
            _ => None,
        },
        KeyMode::K7 => match lane {
            Lane::Key1 | Lane::Key3 | Lane::Key5 | Lane::Key7 => Some(Down),
            Lane::Key2 | Lane::Key4 | Lane::Key6 => Some(Up),
            _ => None,
        },
        KeyMode::K8 => match lane {
            Lane::Key2 | Lane::Key4 | Lane::Key5 | Lane::Key7 => Some(Down),
            Lane::Key1 | Lane::Key3 | Lane::Key6 | Lane::Key8 => Some(Up),
            _ => None,
        },
        KeyMode::K9 => match lane {
            Lane::Key1 | Lane::Key3 | Lane::Key5 | Lane::Key7 => Some(Down),
            Lane::Key2 | Lane::Key4 | Lane::Key6 => Some(Up),
            _ => None,
        },
        KeyMode::K10 => match lane {
            Lane::Key1 | Lane::Key3 | Lane::Key5 | Lane::Key8 | Lane::Key10 | Lane::Key12 => {
                Some(Down)
            }
            Lane::Key2 | Lane::Key4 | Lane::Key9 | Lane::Key11 => Some(Up),
            _ => None,
        },
        KeyMode::K14 => match lane {
            Lane::Key1
            | Lane::Key3
            | Lane::Key5
            | Lane::Key7
            | Lane::Key8
            | Lane::Key10
            | Lane::Key12
            | Lane::Key14 => Some(Down),
            Lane::Key2 | Lane::Key4 | Lane::Key6 | Lane::Key9 | Lane::Key11 | Lane::Key13 => {
                Some(Up)
            }
            _ => None,
        },
    }
}

fn resolve_play_bindings_inner(
    input: &ProfileInputConfig,
    key_mode: KeyMode,
    chain: &mut Vec<KeyMode>,
    visiting: &mut HashSet<KeyMode>,
) -> Result<Vec<BindingConfigEntry>, InheritError> {
    if !visiting.insert(key_mode) {
        chain.push(key_mode);
        return Err(InheritError::Cycle { chain: chain.clone() });
    }
    chain.push(key_mode);

    let play_config = input.play.get(key_mode.play_map_key());
    let explicit_parent = play_config
        .and_then(|config| config.inherit.as_deref())
        .map(|key| {
            KeyMode::from_play_map_key(key)
                .ok_or_else(|| InheritError::UnknownKey { key: key.to_string() })
        })
        .transpose()?;

    if is_root_mode(key_mode) && explicit_parent.is_some() {
        visiting.remove(&key_mode);
        return Err(InheritError::RootWithInherit { mode: key_mode });
    }

    let parent = explicit_parent.or_else(|| implicit_inherit(key_mode));

    let resolved = if let Some(parent_mode) = parent {
        inherit_rule(key_mode, parent_mode)
            .ok_or(InheritError::Disallowed { child: key_mode, parent: parent_mode })?;
        let parent_bindings = resolve_play_bindings_inner(input, parent_mode, chain, visiting)?;
        let mut resolved = apply_inherit(key_mode, parent_mode, &parent_bindings)?;
        if let Some(overrides) = play_config
            .map(|config| config.bindings.as_slice())
            .filter(|bindings| !bindings.is_empty())
        {
            resolved = merge_lane_overrides(resolved, overrides);
        }
        resolved
    } else {
        let own = play_config.map(|config| config.bindings.as_slice()).unwrap_or(&[]);
        if own.is_empty() { default_play_bindings(key_mode) } else { own.to_vec() }
    };

    visiting.remove(&key_mode);
    chain.pop();
    Ok(resolved)
}

fn apply_inherit(
    child: KeyMode,
    parent: KeyMode,
    parent_bindings: &[BindingConfigEntry],
) -> Result<Vec<BindingConfigEntry>, InheritError> {
    let rule = inherit_rule(child, parent).ok_or(InheritError::Disallowed { child, parent })?;

    let out = match rule {
        InheritRule::FilterOnly => parent_bindings
            .iter()
            .filter(|entry| {
                entry
                    .lane
                    .is_some_and(|lane| child.active_lanes().contains(&lane_from_config(lane)))
            })
            .cloned()
            .collect(),
        InheritRule::Remap(remap) => parent_bindings
            .iter()
            .filter_map(|entry| {
                let parent_lane = entry.lane?;
                let &(child_lane, _) = remap
                    .iter()
                    .find(|&&(_, candidate)| lane_to_config(candidate) == parent_lane)?;
                let mut remapped = entry.clone();
                remapped.lane = Some(lane_to_config(child_lane));
                Some(remapped)
            })
            .collect(),
    };

    Ok(out)
}

fn merge_lane_overrides(
    mut base: Vec<BindingConfigEntry>,
    overrides: &[BindingConfigEntry],
) -> Vec<BindingConfigEntry> {
    let overridden_lanes: HashSet<_> = overrides.iter().filter_map(|entry| entry.lane).collect();
    base.retain(|entry| entry.lane.is_none_or(|lane| !overridden_lanes.contains(&lane)));
    base.extend(overrides.iter().filter(|entry| entry.lane.is_some()).cloned());
    base
}

fn parent_lane_to_config(lane: Lane) -> LaneConfig {
    match lane {
        Lane::Scratch => LaneConfig::Scratch,
        Lane::Key1 => LaneConfig::Key1,
        Lane::Key2 => LaneConfig::Key2,
        Lane::Key3 => LaneConfig::Key3,
        Lane::Key4 => LaneConfig::Key4,
        Lane::Key5 => LaneConfig::Key5,
        Lane::Key6 => LaneConfig::Key6,
        Lane::Key7 => LaneConfig::Key7,
        Lane::Scratch2 => LaneConfig::Scratch2,
        Lane::Key8 => LaneConfig::Key8,
        Lane::Key9 => LaneConfig::Key9,
        Lane::Key10 => LaneConfig::Key10,
        Lane::Key11 => LaneConfig::Key11,
        Lane::Key12 => LaneConfig::Key12,
        Lane::Key13 => LaneConfig::Key13,
        Lane::Key14 => LaneConfig::Key14,
    }
}

fn lane_to_config(lane: Lane) -> LaneConfig {
    parent_lane_to_config(lane)
}

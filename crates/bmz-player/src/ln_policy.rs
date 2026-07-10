use bmz_chart::model::{LongNoteMode, PlayableChart};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LnPolicySetting {
    #[default]
    AutoLn,
    AutoCn,
    AutoHcn,
    ForceLn,
    ForceCn,
    ForceHcn,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum LnScorePolicy {
    AutoLn,
    AutoCn,
    AutoHcn,
    ForceLn,
    ForceCn,
    ForceHcn,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ChartLnProfile {
    pub has_undefined_ln: bool,
    pub has_defined_ln: bool,
    pub has_defined_cn: bool,
    pub has_defined_hcn: bool,
}

impl ChartLnProfile {
    pub fn from_chart(chart: &PlayableChart) -> Self {
        let mut profile = Self::default();
        for pair in &chart.long_notes {
            match pair.mode {
                Some(LongNoteMode::Ln) => profile.has_defined_ln = true,
                Some(LongNoteMode::Cn) => profile.has_defined_cn = true,
                Some(LongNoteMode::Hcn) => profile.has_defined_hcn = true,
                None => profile.has_undefined_ln = true,
            }
        }
        profile
    }

    pub fn has_any_ln(self) -> bool {
        self.has_undefined_ln || self.has_any_defined_ln()
    }

    pub fn has_any_defined_ln(self) -> bool {
        self.has_defined_ln || self.has_defined_cn || self.has_defined_hcn
    }

    fn single_defined_mode(self) -> Option<LongNoteMode> {
        match (self.has_defined_ln, self.has_defined_cn, self.has_defined_hcn) {
            (true, false, false) => Some(LongNoteMode::Ln),
            (false, true, false) => Some(LongNoteMode::Cn),
            (false, false, true) => Some(LongNoteMode::Hcn),
            _ => None,
        }
    }
}

impl LnPolicySetting {
    pub const ORDER: [Self; 6] =
        [Self::AutoLn, Self::AutoCn, Self::AutoHcn, Self::ForceLn, Self::ForceCn, Self::ForceHcn];

    pub const fn is_force(self) -> bool {
        matches!(self, Self::ForceLn | Self::ForceCn | Self::ForceHcn)
    }

    pub const fn mode(self) -> LongNoteMode {
        match self {
            Self::AutoLn | Self::ForceLn => LongNoteMode::Ln,
            Self::AutoCn | Self::ForceCn => LongNoteMode::Cn,
            Self::AutoHcn | Self::ForceHcn => LongNoteMode::Hcn,
        }
    }

    pub fn next(self) -> Self {
        cycle_ln_policy_setting(self, 1)
    }

    pub fn previous(self) -> Self {
        cycle_ln_policy_setting(self, -1)
    }

    pub const fn display_label(self) -> &'static str {
        match self {
            Self::AutoLn => "AUTO(LN)",
            Self::AutoCn => "AUTO(CN)",
            Self::AutoHcn => "AUTO(HCN)",
            Self::ForceLn => "FORCE(LN)",
            Self::ForceCn => "FORCE(CN)",
            Self::ForceHcn => "FORCE(HCN)",
        }
    }

    pub const fn as_ir_str(self) -> &'static str {
        match self {
            Self::AutoLn => "AutoLn",
            Self::AutoCn => "AutoCn",
            Self::AutoHcn => "AutoHcn",
            Self::ForceLn => "ForceLn",
            Self::ForceCn => "ForceCn",
            Self::ForceHcn => "ForceHcn",
        }
    }
}

fn cycle_ln_policy_setting(current: LnPolicySetting, direction: i32) -> LnPolicySetting {
    let index = LnPolicySetting::ORDER.iter().position(|value| *value == current).unwrap_or(0);
    let len = LnPolicySetting::ORDER.len();
    if direction >= 0 {
        LnPolicySetting::ORDER[(index + 1) % len]
    } else {
        LnPolicySetting::ORDER[(index + len - 1) % len]
    }
}

impl LnScorePolicy {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::AutoLn => "AutoLn",
            Self::AutoCn => "AutoCn",
            Self::AutoHcn => "AutoHcn",
            Self::ForceLn => "ForceLn",
            Self::ForceCn => "ForceCn",
            Self::ForceHcn => "ForceHcn",
        }
    }

    pub fn from_str_opt(value: &str) -> Option<Self> {
        match value {
            "AutoLn" => Some(Self::AutoLn),
            "AutoCn" => Some(Self::AutoCn),
            "AutoHcn" => Some(Self::AutoHcn),
            "ForceLn" => Some(Self::ForceLn),
            "ForceCn" => Some(Self::ForceCn),
            "ForceHcn" => Some(Self::ForceHcn),
            _ => None,
        }
    }

    pub const fn force(mode: LongNoteMode) -> Self {
        match mode {
            LongNoteMode::Ln => Self::ForceLn,
            LongNoteMode::Cn => Self::ForceCn,
            LongNoteMode::Hcn => Self::ForceHcn,
        }
    }

    pub const fn auto(mode: LongNoteMode) -> Self {
        match mode {
            LongNoteMode::Ln => Self::AutoLn,
            LongNoteMode::Cn => Self::AutoCn,
            LongNoteMode::Hcn => Self::AutoHcn,
        }
    }
}

pub fn score_ln_policy(setting: LnPolicySetting, profile: ChartLnProfile) -> LnScorePolicy {
    if !profile.has_any_ln() {
        return LnScorePolicy::ForceLn;
    }

    if setting.is_force() {
        return LnScorePolicy::force(setting.mode());
    }

    if profile.has_undefined_ln && !profile.has_any_defined_ln() {
        return LnScorePolicy::force(setting.mode());
    }

    if !profile.has_undefined_ln {
        if let Some(mode) = profile.single_defined_mode() {
            return LnScorePolicy::force(mode);
        }
        return LnScorePolicy::AutoLn;
    }

    LnScorePolicy::auto(setting.mode())
}

pub fn score_ln_policy_for_chart(setting: LnPolicySetting, chart: &PlayableChart) -> LnScorePolicy {
    score_ln_policy(setting, ChartLnProfile::from_chart(chart))
}

pub fn apply_ln_policy_to_chart(setting: LnPolicySetting, chart: &mut PlayableChart) {
    let effective_mode = effective_ln_mode(setting, ChartLnProfile::from_chart(chart));
    chart.metadata.long_note_mode = effective_mode;
    if setting.is_force() {
        for pair in &mut chart.long_notes {
            pair.mode = Some(effective_mode);
        }
    }
}

pub fn force_ln_mode_for_chart(mode: LongNoteMode, chart: &mut PlayableChart) {
    chart.metadata.long_note_mode = mode;
    for pair in &mut chart.long_notes {
        pair.mode = Some(mode);
    }
}

pub fn effective_ln_mode(setting: LnPolicySetting, profile: ChartLnProfile) -> LongNoteMode {
    match score_ln_policy(setting, profile) {
        LnScorePolicy::AutoLn | LnScorePolicy::ForceLn => LongNoteMode::Ln,
        LnScorePolicy::AutoCn | LnScorePolicy::ForceCn => LongNoteMode::Cn,
        LnScorePolicy::AutoHcn | LnScorePolicy::ForceHcn => LongNoteMode::Hcn,
    }
}

/// Score-target note count under `setting`, matching judge `affects_score` rules.
///
/// Base count is Tap + LongStart (`chart.total_notes`). Each long pair whose
/// effective mode is CN/HCN adds one more (the end note also scores). LN pairs
/// score once at the end, so they do not add beyond the base LongStart.
pub fn expected_scored_note_count(chart: &PlayableChart, setting: LnPolicySetting) -> u32 {
    let profile = ChartLnProfile::from_chart(chart);
    let policy = score_ln_policy(setting, profile);
    let fallback = match policy {
        LnScorePolicy::AutoLn | LnScorePolicy::ForceLn => LongNoteMode::Ln,
        LnScorePolicy::AutoCn | LnScorePolicy::ForceCn => LongNoteMode::Cn,
        LnScorePolicy::AutoHcn | LnScorePolicy::ForceHcn => LongNoteMode::Hcn,
    };
    let force_all =
        matches!(policy, LnScorePolicy::ForceLn | LnScorePolicy::ForceCn | LnScorePolicy::ForceHcn);

    let mut extra_ends = 0u32;
    for pair in &chart.long_notes {
        let mode = if force_all { fallback } else { pair.mode.unwrap_or(fallback) };
        if matches!(mode, LongNoteMode::Cn | LongNoteMode::Hcn) {
            extra_ends = extra_ends.saturating_add(1);
        }
    }
    chart.total_notes.saturating_add(extra_ends)
}

/// Expected score-target note count for an already-resolved [`LnScorePolicy`].
///
/// `Force*` forces every long pair to that mode. `Auto*` keeps defined pair modes
/// and applies the policy fallback to undefined pairs.
pub fn expected_scored_note_count_for_policy(chart: &PlayableChart, policy: LnScorePolicy) -> u32 {
    let (force_all, fallback) = match policy {
        LnScorePolicy::ForceLn => (true, LongNoteMode::Ln),
        LnScorePolicy::ForceCn => (true, LongNoteMode::Cn),
        LnScorePolicy::ForceHcn => (true, LongNoteMode::Hcn),
        LnScorePolicy::AutoLn => (false, LongNoteMode::Ln),
        LnScorePolicy::AutoCn => (false, LongNoteMode::Cn),
        LnScorePolicy::AutoHcn => (false, LongNoteMode::Hcn),
    };
    let mut extra_ends = 0u32;
    for pair in &chart.long_notes {
        let mode = if force_all { fallback } else { pair.mode.unwrap_or(fallback) };
        if matches!(mode, LongNoteMode::Cn | LongNoteMode::Hcn) {
            extra_ends = extra_ends.saturating_add(1);
        }
    }
    chart.total_notes.saturating_add(extra_ends)
}

#[cfg(test)]
mod tests {
    use bmz_chart::model::{ChartMetadata, LongNotePair, LongNoteStyle};
    use bmz_core::chart::ChartIdentity;
    use bmz_core::ids::NoteId;
    use bmz_core::lane::Lane;
    use bmz_core::time::{ChartTick, TimeUs};

    use super::*;

    const NONE: ChartLnProfile = ChartLnProfile {
        has_undefined_ln: false,
        has_defined_ln: false,
        has_defined_cn: false,
        has_defined_hcn: false,
    };
    const UNDEFINED_ONLY: ChartLnProfile = ChartLnProfile { has_undefined_ln: true, ..NONE };
    const DEFINED_LN_ONLY: ChartLnProfile = ChartLnProfile { has_defined_ln: true, ..NONE };
    const DEFINED_CN_ONLY: ChartLnProfile = ChartLnProfile { has_defined_cn: true, ..NONE };
    const DEFINED_HCN_ONLY: ChartLnProfile = ChartLnProfile { has_defined_hcn: true, ..NONE };
    const DEFINED_MIXED: ChartLnProfile =
        ChartLnProfile { has_defined_ln: true, has_defined_cn: true, ..NONE };
    const UNDEFINED_AND_DEFINED: ChartLnProfile =
        ChartLnProfile { has_undefined_ln: true, has_defined_cn: true, ..NONE };

    #[test]
    fn policy_setting_ir_strings_use_score_policy_casing() {
        assert_eq!(LnPolicySetting::AutoLn.as_ir_str(), "AutoLn");
        assert_eq!(LnPolicySetting::AutoCn.as_ir_str(), "AutoCn");
        assert_eq!(LnPolicySetting::AutoHcn.as_ir_str(), "AutoHcn");
        assert_eq!(LnPolicySetting::ForceLn.as_ir_str(), "ForceLn");
        assert_eq!(LnPolicySetting::ForceCn.as_ir_str(), "ForceCn");
        assert_eq!(LnPolicySetting::ForceHcn.as_ir_str(), "ForceHcn");
    }

    #[test]
    fn score_policy_canonicalizes_no_ln() {
        for setting in [
            LnPolicySetting::AutoLn,
            LnPolicySetting::AutoCn,
            LnPolicySetting::AutoHcn,
            LnPolicySetting::ForceLn,
            LnPolicySetting::ForceCn,
            LnPolicySetting::ForceHcn,
        ] {
            assert_eq!(score_ln_policy(setting, NONE), LnScorePolicy::ForceLn);
        }
    }

    #[test]
    fn score_policy_collapses_undefined_only_auto_to_force() {
        assert_eq!(
            score_ln_policy(LnPolicySetting::AutoLn, UNDEFINED_ONLY),
            LnScorePolicy::ForceLn
        );
        assert_eq!(
            score_ln_policy(LnPolicySetting::AutoCn, UNDEFINED_ONLY),
            LnScorePolicy::ForceCn
        );
        assert_eq!(
            score_ln_policy(LnPolicySetting::AutoHcn, UNDEFINED_ONLY),
            LnScorePolicy::ForceHcn
        );
    }

    #[test]
    fn score_policy_collapses_single_defined_mode_auto_to_force() {
        assert_eq!(
            score_ln_policy(LnPolicySetting::AutoCn, DEFINED_LN_ONLY),
            LnScorePolicy::ForceLn
        );
        assert_eq!(
            score_ln_policy(LnPolicySetting::AutoLn, DEFINED_CN_ONLY),
            LnScorePolicy::ForceCn
        );
        assert_eq!(
            score_ln_policy(LnPolicySetting::AutoLn, DEFINED_HCN_ONLY),
            LnScorePolicy::ForceHcn
        );
    }

    #[test]
    fn score_policy_keeps_auto_for_mixed_cases() {
        assert_eq!(score_ln_policy(LnPolicySetting::AutoCn, DEFINED_MIXED), LnScorePolicy::AutoLn);
        assert_eq!(
            score_ln_policy(LnPolicySetting::AutoCn, UNDEFINED_AND_DEFINED),
            LnScorePolicy::AutoCn
        );
        assert_eq!(
            score_ln_policy(LnPolicySetting::AutoHcn, UNDEFINED_AND_DEFINED),
            LnScorePolicy::AutoHcn
        );
    }

    #[test]
    fn force_setting_always_forces_when_chart_has_ln() {
        assert_eq!(
            score_ln_policy(LnPolicySetting::ForceLn, DEFINED_MIXED),
            LnScorePolicy::ForceLn
        );
        assert_eq!(
            score_ln_policy(LnPolicySetting::ForceCn, UNDEFINED_AND_DEFINED),
            LnScorePolicy::ForceCn
        );
        assert_eq!(
            score_ln_policy(LnPolicySetting::ForceHcn, DEFINED_CN_ONLY),
            LnScorePolicy::ForceHcn
        );
    }

    #[test]
    fn expected_scored_notes_force_cn_adds_end_per_long_pair() {
        let chart = chart_with_long_modes(&[None, None]);
        // base total_notes = 2 (LongStarts); ForceCn scores start+end => 4
        assert_eq!(expected_scored_note_count(&chart, LnPolicySetting::ForceCn), 4);
        assert_eq!(expected_scored_note_count_for_policy(&chart, LnScorePolicy::ForceCn), 4);
        assert_eq!(expected_scored_note_count(&chart, LnPolicySetting::ForceLn), 2);
    }

    #[test]
    fn expected_scored_notes_auto_keeps_defined_and_applies_fallback() {
        let chart = chart_with_long_modes(&[None, Some(LongNoteMode::Ln)]);
        // AutoCn -> AutoCn policy: undefined->CN (+1), defined LN (+0) => base 2 + 1
        assert_eq!(expected_scored_note_count(&chart, LnPolicySetting::AutoCn), 3);
        // AutoLn on defined CN only collapses to ForceCn => both ends score
        let defined_cn = chart_with_long_modes(&[Some(LongNoteMode::Cn)]);
        assert_eq!(expected_scored_note_count(&defined_cn, LnPolicySetting::AutoLn), 2);
    }

    #[test]
    fn auto_policy_keeps_defined_modes_and_sets_undefined_fallback() {
        let mut chart = chart_with_long_modes(&[None, Some(LongNoteMode::Hcn)]);

        apply_ln_policy_to_chart(LnPolicySetting::AutoCn, &mut chart);

        assert_eq!(chart.metadata.long_note_mode, LongNoteMode::Cn);
        assert_eq!(chart.long_notes[0].mode, None);
        assert_eq!(chart.long_notes[1].mode, Some(LongNoteMode::Hcn));
    }

    #[test]
    fn force_policy_overwrites_defined_and_undefined_modes() {
        let mut chart = chart_with_long_modes(&[None, Some(LongNoteMode::Ln)]);

        apply_ln_policy_to_chart(LnPolicySetting::ForceHcn, &mut chart);

        assert_eq!(chart.metadata.long_note_mode, LongNoteMode::Hcn);
        assert!(chart.long_notes.iter().all(|pair| pair.mode == Some(LongNoteMode::Hcn)));
    }

    fn chart_with_long_modes(modes: &[Option<LongNoteMode>]) -> PlayableChart {
        PlayableChart {
            identity: ChartIdentity { file_md5: [0; 16], file_sha256: [0; 32] },
            metadata: ChartMetadata::default(),
            lane_notes: std::array::from_fn(|_| Vec::new()),
            long_notes: modes
                .iter()
                .enumerate()
                .map(|(index, mode)| LongNotePair {
                    lane: Lane::Key1,
                    style: LongNoteStyle::ChannelPair,
                    mode: *mode,
                    start_note_id: NoteId((index * 2 + 1) as u32),
                    end_note_id: NoteId((index * 2 + 2) as u32),
                    start_tick: ChartTick(0),
                    end_tick: ChartTick(192),
                    start_time: TimeUs(0),
                    end_time: TimeUs(1_000_000),
                    sound: None,
                })
                .collect(),
            bgm_events: Vec::new(),
            bga_events: Vec::new(),
            timing_events: Vec::new(),
            scroll_events: Vec::new(),
            speed_events: Vec::new(),
            judge_rank_events: Vec::new(),
            bgm_volume_events: Vec::new(),
            key_volume_events: Vec::new(),
            text_events: Vec::new(),
            bga_opacity_events: Vec::new(),
            bga_argb_events: Vec::new(),
            swbga_definitions: Vec::new(),
            bga_keybound_events: Vec::new(),
            bga_asset_by_bmp_key: std::collections::HashMap::new(),
            bar_lines: Vec::new(),
            sounds: Vec::new(),
            bga_assets: Vec::new(),
            total_notes: modes.len() as u32,
            end_time: TimeUs(1_000_000),
        }
    }
}

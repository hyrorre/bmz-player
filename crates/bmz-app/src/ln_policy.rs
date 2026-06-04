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

pub fn effective_ln_mode(setting: LnPolicySetting, profile: ChartLnProfile) -> LongNoteMode {
    match score_ln_policy(setting, profile) {
        LnScorePolicy::AutoLn | LnScorePolicy::ForceLn => LongNoteMode::Ln,
        LnScorePolicy::AutoCn | LnScorePolicy::ForceCn => LongNoteMode::Cn,
        LnScorePolicy::AutoHcn | LnScorePolicy::ForceHcn => LongNoteMode::Hcn,
    }
}

#[cfg(test)]
mod tests {
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
}

use std::io::Write;
use std::path::Path;

use anyhow::{Result, bail};
use bmz_core::clear::GaugeType;
use bmz_core::replay::ReplayEvent;
use bmz_gameplay::replay::ReplayPlayer;
use bmz_gameplay::rule::RuleMode;
use serde::{Deserialize, Serialize};

use crate::ln_policy::LnScorePolicy;
use crate::screens::play_session::SRandomScheme;
use crate::select_options::{ArrangeOption, DoubleOption, DoubleOptionScoreBucket};

pub const REPLAY_FILE_VERSION: u32 = 9;
pub const SEED_SCHEME_BEATORAJA_24BIT_V1: &str = "beatoraja_24bit_v1";
pub const SEED_SCHEME_LEGACY_SHARED_V3: &str = "legacy_shared_v3";
pub const S_RANDOM_SCHEME_LEGACY_40MS_V1: &str = SRandomScheme::LEGACY_40MS_V1;
pub const S_RANDOM_SCHEME_LM_120HZ_V1: &str = SRandomScheme::LM_120HZ_V1;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplayFile {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch_decisions: Option<Vec<bmz_core::replay::BranchDecision>>,
    pub version: u32,
    pub chart_sha256: String,
    #[serde(default)]
    pub ln_policy: String,
    pub played_at: i64,
    #[serde(default)]
    pub random_seed: Option<i64>,
    #[serde(default = "default_arrange")]
    pub arrange: String,
    #[serde(default = "default_arrange")]
    pub arrange_2p: String,
    #[serde(default = "default_double_option")]
    pub double_option: String,
    /// DP option actually applied to the chart. `double_option` remains the
    /// score bucket, where FLIP intentionally shares Off.
    #[serde(default)]
    pub applied_double_option: String,
    /// Gauge selected when the replay was recorded. Empty on Replay v1-v5.
    #[serde(default)]
    pub gauge_type: String,
    /// beatoraja H-RANDOM/ALL-SCR key-lane threshold. The source `.brd`
    /// does not carry it, so importers may recover it from config_player.json.
    #[serde(default)]
    pub h_random_threshold_ms: Option<u32>,
    #[serde(default)]
    pub arrange_seed: Option<i64>,
    #[serde(default)]
    pub arrange_seed_2p: Option<i64>,
    #[serde(default)]
    pub bms_random_choices: Option<Vec<i32>>,
    #[serde(default)]
    pub bms_switch_choices: Option<Vec<u64>>,
    #[serde(default)]
    pub seed_scheme: String,
    #[serde(default)]
    pub s_random_scheme: String,
    #[serde(default)]
    pub s_random_scheme_2p: String,
    #[serde(default)]
    pub lane_shuffle_pattern: Option<Vec<u8>>,
    pub events: Vec<ReplayEvent>,
}

fn default_arrange() -> String {
    "Normal".to_string()
}

fn default_double_option() -> String {
    "Off".to_string()
}

impl ReplayFile {
    pub fn player(&self) -> ReplayPlayer {
        ReplayPlayer {
            events: self.events.clone(),
            branch_decisions: self.branch_decisions.clone(),
            next_index: 0,
        }
    }

    pub fn with_branch_decisions(
        mut self,
        decisions: Option<Vec<bmz_core::replay::BranchDecision>>,
    ) -> Self {
        self.branch_decisions = decisions;
        self
    }
    pub fn new(
        chart_sha256: [u8; 32],
        played_at: i64,
        random_seed: Option<i64>,
        arrange: ArrangeOption,
        arrange_seed: Option<i64>,
        lane_shuffle_pattern: Option<Vec<u8>>,
        events: Vec<ReplayEvent>,
    ) -> Self {
        Self::new_with_policy(
            chart_sha256,
            LnScorePolicy::ForceLn,
            DoubleOptionScoreBucket::Off,
            played_at,
            random_seed,
            arrange,
            ArrangeOption::Normal,
            arrange_seed,
            lane_shuffle_pattern,
            events,
        )
    }

    pub fn new_with_policy(
        chart_sha256: [u8; 32],
        ln_policy: LnScorePolicy,
        double_option: DoubleOptionScoreBucket,
        played_at: i64,
        random_seed: Option<i64>,
        arrange: ArrangeOption,
        arrange_2p: ArrangeOption,
        arrange_seed: Option<i64>,
        lane_shuffle_pattern: Option<Vec<u8>>,
        mut events: Vec<ReplayEvent>,
    ) -> Self {
        // Device timestamps received in the same poll can differ by a few
        // microseconds and arrive out of order. ReplayPlayer consumes a time-
        // ordered stream, so preserve equal-time order while normalizing it.
        events.sort_by_key(|event| event.time);
        Self {
            branch_decisions: None,
            version: REPLAY_FILE_VERSION,
            chart_sha256: hex_encode(&chart_sha256),
            ln_policy: ln_policy.as_str().to_string(),
            played_at,
            random_seed,
            arrange: arrange.to_persistent_str().to_string(),
            arrange_2p: arrange_2p.to_persistent_str().to_string(),
            double_option: double_option.as_str().to_string(),
            applied_double_option: double_option.as_double_option().to_persistent_str().to_string(),
            gauge_type: String::new(),
            h_random_threshold_ms: None,
            arrange_seed,
            arrange_seed_2p: None,
            bms_random_choices: Some(Vec::new()),
            bms_switch_choices: Some(Vec::new()),
            seed_scheme: SEED_SCHEME_BEATORAJA_24BIT_V1.to_string(),
            s_random_scheme: S_RANDOM_SCHEME_LM_120HZ_V1.to_string(),
            s_random_scheme_2p: String::new(),
            lane_shuffle_pattern,
            events,
        }
    }

    pub fn with_randomization(
        mut self,
        arrange_seed_2p: Option<i64>,
        bms_random_choices: Vec<i32>,
        bms_switch_choices: Vec<u64>,
    ) -> Self {
        self.arrange_seed_2p = arrange_seed_2p;
        self.bms_random_choices = Some(bms_random_choices);
        self.bms_switch_choices = Some(bms_switch_choices);
        self
    }

    pub fn with_seed_scheme(mut self, seed_scheme: impl Into<String>) -> Self {
        self.seed_scheme = seed_scheme.into();
        self
    }

    pub fn with_s_random_scheme(mut self, s_random_scheme: SRandomScheme) -> Self {
        self.s_random_scheme = s_random_scheme.as_str().to_string();
        self.s_random_scheme_2p.clear();
        self
    }

    pub fn with_s_random_schemes(
        mut self,
        s_random_scheme: SRandomScheme,
        s_random_scheme_2p: Option<SRandomScheme>,
    ) -> Self {
        self.s_random_scheme = s_random_scheme.as_str().to_string();
        self.s_random_scheme_2p = s_random_scheme_2p
            .filter(|&scheme| scheme != s_random_scheme)
            .map(|scheme| scheme.as_str().to_string())
            .unwrap_or_default();
        self
    }

    pub fn with_playback_metadata(
        mut self,
        applied_double_option: DoubleOption,
        gauge_type: GaugeType,
        h_random_threshold_ms: Option<u32>,
    ) -> Self {
        self.applied_double_option = applied_double_option.to_persistent_str().to_string();
        self.gauge_type = gauge_type.as_str().to_string();
        self.h_random_threshold_ms = h_random_threshold_ms;
        self
    }

    pub fn effective_seed_scheme(&self) -> &str {
        if self.version < 4 || self.seed_scheme.is_empty() {
            SEED_SCHEME_LEGACY_SHARED_V3
        } else {
            &self.seed_scheme
        }
    }

    pub fn uses_legacy_seed_scheme(&self) -> bool {
        self.effective_seed_scheme() == SEED_SCHEME_LEGACY_SHARED_V3
    }

    pub fn effective_s_random_scheme(&self) -> Result<SRandomScheme> {
        let declared = if self.s_random_scheme.is_empty() {
            None
        } else {
            Some(SRandomScheme::from_persistent_str(&self.s_random_scheme)?)
        };
        match (self.version >= 5, declared) {
            (true, Some(scheme)) => Ok(scheme),
            _ => Ok(SRandomScheme::Legacy40MsV1),
        }
    }

    pub fn effective_s_random_scheme_2p(&self) -> Result<SRandomScheme> {
        let declared = if self.s_random_scheme_2p.is_empty() {
            None
        } else {
            Some(SRandomScheme::from_persistent_str(&self.s_random_scheme_2p)?)
        };
        match (self.version >= 5, declared) {
            (true, Some(scheme)) => Ok(scheme),
            _ => self.effective_s_random_scheme(),
        }
    }

    pub fn arrange_option(&self) -> ArrangeOption {
        ArrangeOption::from_persistent_str(&self.arrange)
    }

    pub fn arrange_2p_option(&self) -> ArrangeOption {
        ArrangeOption::from_persistent_str(&self.arrange_2p)
    }

    pub fn double_option(&self) -> DoubleOption {
        if !self.applied_double_option.is_empty() {
            return DoubleOption::from_persistent_str(&self.applied_double_option);
        }
        match DoubleOptionScoreBucket::from_str_or_off(&self.double_option) {
            DoubleOptionScoreBucket::Off => DoubleOption::Off,
            DoubleOptionScoreBucket::Battle => DoubleOption::Battle,
            DoubleOptionScoreBucket::BattleAutoScratch => DoubleOption::BattleAutoScratch,
        }
    }

    pub fn double_option_bucket(&self) -> DoubleOptionScoreBucket {
        DoubleOptionScoreBucket::from_str_or_off(&self.double_option)
    }

    pub fn recorded_gauge_type(&self) -> Option<GaugeType> {
        match self.gauge_type.as_str() {
            "AssistEasy" => Some(GaugeType::AssistEasy),
            "Easy" => Some(GaugeType::Easy),
            "Normal" => Some(GaugeType::Normal),
            "Hard" => Some(GaugeType::Hard),
            "ExHard" => Some(GaugeType::ExHard),
            "Hazard" => Some(GaugeType::Hazard),
            "Class" => Some(GaugeType::Class),
            "ExClass" => Some(GaugeType::ExClass),
            "ExHardClass" => Some(GaugeType::ExHardClass),
            _ => None,
        }
    }
}

pub fn save_replay(path: &Path, replay: &ReplayFile) -> Result<()> {
    save_replay_with_hash(path, replay)?;
    Ok(())
}

/// リプレイを保存し、書き込んだバイト列の SHA256 (hex) を返す。
/// 保存直後にファイルを読み直して hash を取るのを避けるため、
/// serialize したテキストから直接計算する。
pub fn save_replay_with_hash(path: &Path, replay: &ReplayFile) -> Result<String> {
    save_replay_with_durability(path, replay, true)
}

/// Bulk imports remain recoverable from their source files, so they keep the
/// atomic temp-file rename but avoid forcing every individual replay to disk.
pub(crate) fn save_replay_for_import(path: &Path, replay: &ReplayFile) -> Result<()> {
    save_replay_with_durability(path, replay, false)?;
    Ok(())
}

fn save_replay_with_durability(
    path: &Path,
    replay: &ReplayFile,
    sync_file: bool,
) -> Result<String> {
    use sha2::{Digest, Sha256};

    let text = toml::to_string_pretty(replay)?;
    let hash = super::common::hash_to_hex(&Sha256::digest(text.as_bytes()));

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let tmp_path = path.with_extension("tmp");
    {
        let mut file = std::fs::File::create(&tmp_path)?;
        file.write_all(text.as_bytes())?;
        if sync_file {
            file.sync_all()?;
        }
    }
    std::fs::rename(tmp_path, path)?;
    Ok(hash)
}

pub fn load_replay(path: &Path) -> Result<ReplayFile> {
    let text = std::fs::read_to_string(path)?;
    parse_replay(&text)
}

/// Parse a replay received from local storage or a trusted IR transport.
pub fn parse_replay(text: &str) -> Result<ReplayFile> {
    let replay: ReplayFile = toml::from_str(text)?;
    if let Some(decisions) = &replay.branch_decisions {
        if replay.version < 9 {
            bail!("conditional replay requires format version 9 or later");
        }
        let mut seen = std::collections::HashSet::new();
        if decisions.iter().any(|d| !seen.insert(d.block) || d.time.0 < 0)
            || decisions.windows(2).any(|w| w[0].time > w[1].time)
        {
            bail!("invalid conditional replay decisions");
        }
    }
    if replay.ln_policy == "ForceHln" && replay.version < 8 {
        bail!("HLN replay requires format version 8 or later");
    }
    replay.effective_s_random_scheme()?;
    replay.effective_s_random_scheme_2p()?;
    Ok(replay)
}

pub fn load_replay_player(path: &Path) -> Result<ReplayPlayer> {
    let replay = load_replay(path)?;
    Ok(replay.player())
}

pub fn load_replay_player_for_chart(path: &Path, chart_sha256: [u8; 32]) -> Result<ReplayPlayer> {
    let replay = load_replay_for_chart(path, chart_sha256)?;
    Ok(replay.player())
}

pub fn load_replay_for_chart(path: &Path, chart_sha256: [u8; 32]) -> Result<ReplayFile> {
    let replay = load_replay(path)?;
    if replay.chart_sha256_bytes()? != chart_sha256 {
        bail!("replay chart hash does not match selected chart");
    }
    Ok(replay)
}

pub fn load_replay_for_chart_and_policy(
    path: &Path,
    chart_sha256: [u8; 32],
    ln_policy: LnScorePolicy,
) -> Result<ReplayFile> {
    let replay = load_replay_for_chart(path, chart_sha256)?;
    if !replay.ln_policy.is_empty()
        && LnScorePolicy::from_str_opt(&replay.ln_policy) != Some(ln_policy)
    {
        bail!("replay long note policy does not match selected chart policy");
    }
    Ok(replay)
}

pub fn load_replay_for_chart_policy_and_double_option(
    path: &Path,
    chart_sha256: [u8; 32],
    ln_policy: LnScorePolicy,
    double_option: DoubleOptionScoreBucket,
) -> Result<ReplayFile> {
    let replay = load_replay_for_chart_and_policy(path, chart_sha256, ln_policy)?;
    if replay.double_option_bucket() != double_option {
        bail!("replay double option does not match selected score bucket");
    }
    Ok(replay)
}

pub fn replay_file_name(chart_sha256: [u8; 32], played_at: i64) -> String {
    format!("{}-{played_at}.toml", hex_encode(&chart_sha256))
}

/// One queued replay inside a course attempt: keeps the current chart id, the
/// per-chart replay file (events + arrange info), and the chart sha256 the
/// replay was recorded against so callers can verify before launch.
#[derive(Debug, Clone)]
pub struct QueuedCourseReplay {
    pub position: i64,
    pub chart_id: i64,
    pub chart_sha256: [u8; 32],
    pub replay: ReplayFile,
}

/// Load every replay file referenced by a `course_scores` row.
///
/// `entries` is the list of `(position, chart_sha256, replay_path)` rows from
/// `course_replays` (already ordered by position).  `lookup_chart_id` resolves
/// a chart sha256 to the current library row id.
/// `replay_root` is the directory that relative replay paths are joined onto
/// (matches `ProfilePaths.root_dir`).
///
/// Returns the queued replays in order.  Returns an error if any file is
/// missing, malformed, or refers to a chart that is no longer in the library.
pub fn load_course_replays(
    entries: &[(i64, [u8; 32], String)],
    replay_root: &Path,
    lookup_chart_id: impl Fn([u8; 32]) -> Result<Option<i64>>,
) -> Result<Vec<QueuedCourseReplay>> {
    let mut out = Vec::with_capacity(entries.len());
    for (position, sha, rel_path) in entries {
        let Some(chart_id) = lookup_chart_id(*sha)? else {
            bail!("chart {} is no longer in the library", hex_encode(sha));
        };
        let abs = replay_root.join(rel_path);
        let replay = load_replay_for_chart(&abs, *sha)?;
        out.push(QueuedCourseReplay { position: *position, chart_id, chart_sha256: *sha, replay });
    }
    Ok(out)
}

pub fn replay_slot_file_name(
    chart_sha256: [u8; 32],
    ln_policy: LnScorePolicy,
    double_option: DoubleOptionScoreBucket,
    rule_mode: RuleMode,
    slot: u8,
) -> String {
    let double_suffix = match double_option {
        DoubleOptionScoreBucket::Off => String::new(),
        other => format!("-{}", other.as_str()),
    };
    let rule_suffix = match rule_mode {
        RuleMode::Beatoraja => String::new(),
        other => format!("-{}", other.as_str()),
    };
    format!(
        "{}-{}{}{}-slot{slot}.toml",
        hex_encode(&chart_sha256),
        ln_policy.as_str(),
        double_suffix,
        rule_suffix
    )
}

pub fn imported_course_replay_file_name(
    course_hash: &str,
    ln_policy: LnScorePolicy,
    rule_mode: RuleMode,
    slot: u8,
    position: usize,
) -> String {
    let rule_suffix = match rule_mode {
        RuleMode::Beatoraja => String::new(),
        other => format!("-{}", other.as_str()),
    };
    format!(
        "course-{course_hash}-{}{}-slot{slot}-stage{position}.toml",
        ln_policy.as_str(),
        rule_suffix,
    )
}

fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0x0f) as usize] as char);
    }
    out
}

impl ReplayFile {
    pub fn chart_sha256_bytes(&self) -> Result<[u8; 32]> {
        hex_decode_32(&self.chart_sha256)
    }
}

fn hex_decode_32(value: &str) -> Result<[u8; 32]> {
    if value.len() != 64 {
        bail!("expected 64 hex characters");
    }

    let mut out = [0_u8; 32];
    for (index, chunk) in value.as_bytes().as_chunks::<2>().0.iter().enumerate() {
        out[index] = (hex_digit(chunk[0])? << 4) | hex_digit(chunk[1])?;
    }
    Ok(out)
}

fn hex_digit(byte: u8) -> Result<u8> {
    match byte {
        b'0'..=b'9' => Ok(byte - b'0'),
        b'a'..=b'f' => Ok(byte - b'a' + 10),
        b'A'..=b'F' => Ok(byte - b'A' + 10),
        _ => bail!("invalid hex digit"),
    }
}

#[cfg(test)]
mod tests {
    use bmz_core::clear::GaugeType;
    use bmz_core::input::{InputDeviceKind, InputKind, ScratchDirection};
    use bmz_core::lane::Lane;
    use bmz_core::time::TimeUs;

    use super::*;

    #[test]
    fn conditional_replay_roundtrip_and_duplicate_rejection() {
        let decision = bmz_core::replay::BranchDecision { block: 2, branch: 1, time: TimeUs(123) };
        let replay =
            ReplayFile::new([1; 32], 1, None, ArrangeOption::Normal, None, None, Vec::new())
                .with_branch_decisions(Some(vec![decision]));
        let parsed = parse_replay(&toml::to_string(&replay).unwrap()).unwrap();
        assert_eq!(parsed.player().branch_decisions, Some(vec![decision]));
        let duplicate = replay.with_branch_decisions(Some(vec![decision, decision]));
        assert!(parse_replay(&toml::to_string(&duplicate).unwrap()).is_err());
    }

    #[test]
    fn replay_constructor_stably_orders_input_events() {
        let event = |lane, time| ReplayEvent {
            lane,
            kind: InputKind::Press,
            time: TimeUs(time),
            device_kind: InputDeviceKind::Keyboard,
            scratch_direction: None,
        };

        let replay = ReplayFile::new(
            [1; 32],
            1,
            None,
            ArrangeOption::Normal,
            None,
            None,
            vec![event(Lane::Key1, 30), event(Lane::Key2, 20), event(Lane::Key3, 20)],
        );

        assert_eq!(
            replay.events.iter().map(|event| event.time.0).collect::<Vec<_>>(),
            vec![20, 20, 30]
        );
        assert_eq!(replay.events[0].lane, Lane::Key2);
        assert_eq!(replay.events[1].lane, Lane::Key3);
    }

    #[test]
    fn hln_replay_v8_policy_round_trip_and_legacy_rejection() {
        let replay = ReplayFile::new_with_policy(
            [4; 32],
            LnScorePolicy::ForceHln,
            DoubleOptionScoreBucket::Off,
            1,
            None,
            ArrangeOption::Normal,
            ArrangeOption::Normal,
            None,
            None,
            vec![],
        );
        let text = toml::to_string(&replay).unwrap();
        let parsed = parse_replay(&text).unwrap();
        assert_eq!(parsed.version, REPLAY_FILE_VERSION);
        assert_eq!(parsed.ln_policy, "ForceHln");
        let legacy = text.replace(&format!("version = {REPLAY_FILE_VERSION}"), "version = 7");
        assert!(parse_replay(&legacy).is_err());
    }

    #[test]
    fn save_and_load_replay_file() {
        let path = std::env::temp_dir().join(format!(
            "bmz-replay-{}-{}.toml",
            std::process::id(),
            TimeUs(42).0
        ));
        let replay = ReplayFile::new(
            [1; 32],
            1_700_000_050,
            Some(123),
            ArrangeOption::Normal,
            None,
            None,
            vec![ReplayEvent {
                lane: Lane::Key1,
                kind: InputKind::Press,
                time: TimeUs(1_000),
                device_kind: InputDeviceKind::Keyboard,
                scratch_direction: None,
            }],
        );

        save_replay(&path, &replay).unwrap();
        let loaded = load_replay(&path).unwrap();

        assert_eq!(loaded.version, REPLAY_FILE_VERSION);
        assert_eq!(
            loaded.chart_sha256,
            "0101010101010101010101010101010101010101010101010101010101010101"
        );
        assert_eq!(loaded.ln_policy, "ForceLn");
        assert_eq!(loaded.s_random_scheme, S_RANDOM_SCHEME_LM_120HZ_V1);
        assert_eq!(loaded.effective_s_random_scheme().unwrap(), SRandomScheme::Lm120HzV1);
        assert_eq!(loaded.events, replay.events);

        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn replay_v7_round_trip_preserves_playback_metadata_and_scratch_direction() {
        let replay = ReplayFile::new_with_policy(
            [9; 32],
            LnScorePolicy::ForceHcn,
            DoubleOptionScoreBucket::Off,
            1_700_000_090,
            None,
            ArrangeOption::HRandom,
            ArrangeOption::Normal,
            Some(0x12_3456),
            None,
            vec![ReplayEvent {
                lane: Lane::Scratch,
                kind: InputKind::Press,
                time: TimeUs(5_000),
                device_kind: InputDeviceKind::Controller,
                scratch_direction: Some(ScratchDirection::Up),
            }],
        )
        .with_playback_metadata(DoubleOption::Flip, GaugeType::Hard, Some(125));

        let text = toml::to_string(&replay).unwrap();
        let loaded: ReplayFile = toml::from_str(&text).unwrap();

        assert_eq!(loaded.version, REPLAY_FILE_VERSION);
        assert_eq!(loaded.double_option_bucket(), DoubleOptionScoreBucket::Off);
        assert_eq!(loaded.double_option(), DoubleOption::Flip);
        assert_eq!(loaded.recorded_gauge_type(), Some(GaugeType::Hard));
        assert_eq!(loaded.h_random_threshold_ms, Some(125));
        assert_eq!(loaded.events[0].scratch_direction, Some(ScratchDirection::Up));
    }

    #[test]
    fn replay_v5_defaults_new_playback_metadata() {
        let replay: ReplayFile = toml::from_str(
            r#"
version = 5
chart_sha256 = "0101010101010101010101010101010101010101010101010101010101010101"
played_at = 1700000060
double_option = "Battle"
events = []
"#,
        )
        .unwrap();

        assert!(replay.applied_double_option.is_empty());
        assert_eq!(replay.double_option(), DoubleOption::Battle);
        assert_eq!(replay.recorded_gauge_type(), None);
        assert_eq!(replay.h_random_threshold_ms, None);
    }

    #[test]
    fn replay_file_name_uses_hash_and_play_time() {
        assert_eq!(
            replay_file_name([0xab; 32], 12),
            "abababababababababababababababababababababababababababababababab-12.toml"
        );
    }

    #[test]
    fn replay_slot_file_name_uses_hash_policy_and_slot_index() {
        assert_eq!(
            replay_slot_file_name(
                [0xab; 32],
                LnScorePolicy::ForceCn,
                DoubleOptionScoreBucket::Off,
                RuleMode::Beatoraja,
                2
            ),
            "abababababababababababababababababababababababababababababababab-ForceCn-slot2.toml"
        );
        assert_eq!(
            replay_slot_file_name(
                [0xab; 32],
                LnScorePolicy::ForceCn,
                DoubleOptionScoreBucket::Battle,
                RuleMode::Beatoraja,
                2
            ),
            "abababababababababababababababababababababababababababababababab-ForceCn-Battle-slot2.toml"
        );
        assert_eq!(
            replay_slot_file_name(
                [0xab; 32],
                LnScorePolicy::ForceCn,
                DoubleOptionScoreBucket::Battle,
                RuleMode::Dx,
                2
            ),
            "abababababababababababababababababababababababababababababababab-ForceCn-Battle-Dx-slot2.toml"
        );
    }

    #[test]
    fn load_replay_player_builds_replay_player() {
        let path = std::env::temp_dir().join(format!(
            "bmz-replay-player-{}-{}.toml",
            std::process::id(),
            TimeUs(43).0
        ));
        let replay = ReplayFile::new(
            [2; 32],
            1_700_000_051,
            None,
            ArrangeOption::Normal,
            None,
            None,
            vec![ReplayEvent {
                lane: Lane::Key2,
                kind: InputKind::Release,
                time: TimeUs(2_000),
                device_kind: InputDeviceKind::Keyboard,
                scratch_direction: None,
            }],
        );
        save_replay(&path, &replay).unwrap();

        let player = load_replay_player(&path).unwrap();

        assert_eq!(player.next_index, 0);
        assert_eq!(player.events, replay.events);

        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn load_replay_player_for_chart_rejects_mismatched_hash() {
        let path = std::env::temp_dir().join(format!(
            "bmz-replay-player-mismatch-{}-{}.toml",
            std::process::id(),
            TimeUs(44).0
        ));
        let replay = ReplayFile::new(
            [2; 32],
            1_700_000_052,
            None,
            ArrangeOption::Normal,
            None,
            None,
            Vec::new(),
        );
        save_replay(&path, &replay).unwrap();

        assert!(load_replay_player_for_chart(&path, [3; 32]).is_err());
        assert!(load_replay_player_for_chart(&path, [2; 32]).is_ok());

        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn load_replay_for_chart_and_policy_rejects_mismatched_ln_policy() {
        let path = std::env::temp_dir().join(format!(
            "bmz-replay-policy-mismatch-{}-{}.toml",
            std::process::id(),
            TimeUs(45).0
        ));
        let replay = ReplayFile::new_with_policy(
            [4; 32],
            LnScorePolicy::ForceCn,
            DoubleOptionScoreBucket::Off,
            1_700_000_052,
            None,
            ArrangeOption::Normal,
            ArrangeOption::Normal,
            None,
            None,
            Vec::new(),
        );
        save_replay(&path, &replay).unwrap();

        assert!(load_replay_for_chart_and_policy(&path, [4; 32], LnScorePolicy::ForceLn).is_err());
        assert!(load_replay_for_chart_and_policy(&path, [4; 32], LnScorePolicy::ForceCn).is_ok());

        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn load_replay_for_chart_and_policy_accepts_legacy_replay_without_ln_policy() {
        let path = std::env::temp_dir().join(format!(
            "bmz-replay-policy-legacy-{}-{}.toml",
            std::process::id(),
            TimeUs(46).0
        ));
        std::fs::write(
            &path,
            r#"
version = 1
chart_sha256 = "0505050505050505050505050505050505050505050505050505050505050505"
played_at = 1700000053
events = []
"#,
        )
        .unwrap();

        let replay =
            load_replay_for_chart_and_policy(&path, [5; 32], LnScorePolicy::ForceHcn).unwrap();

        assert!(replay.ln_policy.is_empty());

        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn replay_file_round_trip_with_arrange_random() {
        let path = std::env::temp_dir().join(format!(
            "bmz-replay-arrange-{}-{}.toml",
            std::process::id(),
            TimeUs(45).0
        ));
        let pattern = vec![0, 3, 2, 1, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15];
        let replay = ReplayFile::new(
            [5; 32],
            1_700_000_055,
            Some(7777),
            ArrangeOption::Random,
            Some(7777),
            Some(pattern.clone()),
            Vec::new(),
        )
        .with_randomization(Some(1234), vec![2, 1], vec![2_000_000_000_000])
        .with_seed_scheme(SEED_SCHEME_LEGACY_SHARED_V3);

        save_replay(&path, &replay).unwrap();
        let loaded = load_replay(&path).unwrap();

        assert_eq!(loaded.arrange, "Random");
        assert_eq!(loaded.arrange_option(), ArrangeOption::Random);
        assert_eq!(loaded.arrange_seed, Some(7777));
        assert_eq!(loaded.arrange_seed_2p, Some(1234));
        assert_eq!(loaded.bms_random_choices, Some(vec![2, 1]));
        assert_eq!(loaded.bms_switch_choices, Some(vec![2_000_000_000_000]));
        assert_eq!(loaded.effective_seed_scheme(), SEED_SCHEME_LEGACY_SHARED_V3);
        assert_eq!(loaded.lane_shuffle_pattern, Some(pattern));

        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn replay_file_v1_back_compat_defaults_arrange_to_normal() {
        let v1_toml = r#"
version = 1
chart_sha256 = "0101010101010101010101010101010101010101010101010101010101010101"
played_at = 1700000060
events = []
"#;

        let loaded: ReplayFile = toml::from_str(v1_toml).unwrap();

        assert_eq!(loaded.version, 1);
        assert_eq!(loaded.arrange, "Normal");
        assert_eq!(loaded.arrange_seed, None);
        assert_eq!(loaded.lane_shuffle_pattern, None);
        assert_eq!(loaded.random_seed, None);
        assert_eq!(loaded.arrange_seed_2p, None);
        assert_eq!(loaded.bms_random_choices, None);
        assert_eq!(loaded.bms_switch_choices, None);
        assert_eq!(loaded.effective_seed_scheme(), SEED_SCHEME_LEGACY_SHARED_V3);
        assert_eq!(loaded.effective_s_random_scheme().unwrap(), SRandomScheme::Legacy40MsV1);
        assert_eq!(loaded.events.len(), 0);
    }

    #[test]
    fn replay_s_random_scheme_defaults_old_or_missing_fields_to_legacy() {
        for version in [4, 5] {
            let replay: ReplayFile = toml::from_str(&format!(
                r#"
version = {version}
chart_sha256 = "0101010101010101010101010101010101010101010101010101010101010101"
played_at = 1700000060
arrange = "SRandom"
seed_scheme = "beatoraja_24bit_v1"
events = []
"#
            ))
            .unwrap();

            assert!(replay.s_random_scheme.is_empty());
            assert_eq!(replay.effective_s_random_scheme().unwrap(), SRandomScheme::Legacy40MsV1);
        }
    }

    #[test]
    fn replay_s_random_and_rng_schemes_are_independent() {
        for (legacy_rng, s_random_scheme) in [
            (true, SRandomScheme::Legacy40MsV1),
            (true, SRandomScheme::Lm120HzV1),
            (false, SRandomScheme::Legacy40MsV1),
            (false, SRandomScheme::Lm120HzV1),
        ] {
            let replay = ReplayFile::new(
                [7; 32],
                1_700_000_061,
                Some(42),
                ArrangeOption::SRandom,
                Some(42),
                None,
                Vec::new(),
            )
            .with_seed_scheme(if legacy_rng {
                SEED_SCHEME_LEGACY_SHARED_V3
            } else {
                SEED_SCHEME_BEATORAJA_24BIT_V1
            })
            .with_s_random_scheme(s_random_scheme);

            assert_eq!(replay.uses_legacy_seed_scheme(), legacy_rng);
            assert_eq!(replay.effective_s_random_scheme().unwrap(), s_random_scheme);
        }
    }

    #[test]
    fn replay_s_random_scheme_round_trip_preserves_mixed_dp_generations() {
        let path = std::env::temp_dir().join(format!(
            "bmz-replay-s-random-scheme-{}-{}.toml",
            std::process::id(),
            TimeUs(49).0
        ));
        let replay = ReplayFile::new(
            [8; 32],
            1_700_000_062,
            Some(42),
            ArrangeOption::SRandom,
            Some(42),
            None,
            Vec::new(),
        )
        .with_s_random_schemes(SRandomScheme::Lm120HzV1, Some(SRandomScheme::Legacy40MsV1));

        save_replay(&path, &replay).unwrap();
        let loaded = load_replay(&path).unwrap();

        assert_eq!(loaded.version, REPLAY_FILE_VERSION);
        assert_eq!(loaded.s_random_scheme, S_RANDOM_SCHEME_LM_120HZ_V1);
        assert_eq!(loaded.s_random_scheme_2p, S_RANDOM_SCHEME_LEGACY_40MS_V1);
        assert_eq!(loaded.effective_s_random_scheme().unwrap(), SRandomScheme::Lm120HzV1);
        assert_eq!(loaded.effective_s_random_scheme_2p().unwrap(), SRandomScheme::Legacy40MsV1);

        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn replay_rejects_unknown_non_empty_s_random_scheme() {
        let path = std::env::temp_dir().join(format!(
            "bmz-replay-unknown-s-random-scheme-{}-{}.toml",
            std::process::id(),
            TimeUs(50).0
        ));
        let mut replay = ReplayFile::new(
            [9; 32],
            1_700_000_063,
            Some(42),
            ArrangeOption::SRandom,
            Some(42),
            None,
            Vec::new(),
        );
        replay.s_random_scheme = "future_240hz_v2".to_string();
        std::fs::write(&path, toml::to_string(&replay).unwrap()).unwrap();

        let error = load_replay(&path).unwrap_err();
        assert!(error.to_string().contains("unsupported S-RANDOM scheme: future_240hz_v2"));

        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn load_course_replays_loads_all_files_in_position_order() {
        let dir = std::env::temp_dir().join(format!(
            "bmz-course-replays-{}-{}",
            std::process::id(),
            TimeUs(47).0
        ));
        let replay_subdir = dir.join("replay");
        std::fs::create_dir_all(&replay_subdir).unwrap();

        // Two charts: id=1 (sha=[1;32]) at position 0, id=2 (sha=[2;32]) at position 1.
        let r0 = ReplayFile::new(
            [1; 32],
            1,
            None,
            ArrangeOption::Normal,
            None,
            None,
            vec![ReplayEvent {
                lane: Lane::Key1,
                kind: InputKind::Press,
                time: TimeUs(10),
                device_kind: InputDeviceKind::Keyboard,
                scratch_direction: None,
            }],
        );
        let r1 = ReplayFile::new(
            [2; 32],
            2,
            None,
            ArrangeOption::Mirror,
            None,
            None,
            vec![ReplayEvent {
                lane: Lane::Key2,
                kind: InputKind::Release,
                time: TimeUs(20),
                device_kind: InputDeviceKind::Keyboard,
                scratch_direction: None,
            }],
        );
        let p0 = replay_subdir.join("c0.toml");
        let p1 = replay_subdir.join("c1.toml");
        save_replay(&p0, &r0).unwrap();
        save_replay(&p1, &r1).unwrap();

        let entries = vec![
            (0_i64, [1; 32], "replay/c0.toml".to_string()),
            (1_i64, [2; 32], "replay/c1.toml".to_string()),
        ];
        let queued = load_course_replays(&entries, &dir, |chart_sha256| {
            Ok(if chart_sha256 == [1; 32] {
                Some(1)
            } else if chart_sha256 == [2; 32] {
                Some(2)
            } else {
                None
            })
        })
        .unwrap();

        assert_eq!(queued.len(), 2);
        assert_eq!(queued[0].position, 0);
        assert_eq!(queued[0].chart_id, 1);
        assert_eq!(queued[0].chart_sha256, [1; 32]);
        assert_eq!(queued[0].replay.events.len(), 1);
        assert_eq!(queued[1].chart_id, 2);
        assert_eq!(queued[1].replay.arrange_option(), ArrangeOption::Mirror);

        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn load_course_replays_rejects_when_chart_sha_no_longer_matches() {
        let dir = std::env::temp_dir().join(format!(
            "bmz-course-replays-mismatch-{}-{}",
            std::process::id(),
            TimeUs(48).0
        ));
        let replay_subdir = dir.join("replay");
        std::fs::create_dir_all(&replay_subdir).unwrap();
        let replay =
            ReplayFile::new([1; 32], 1, None, ArrangeOption::Normal, None, None, Vec::new());
        let p = replay_subdir.join("c0.toml");
        save_replay(&p, &replay).unwrap();

        // Chart was re-imported and now hashes as [9;32]; verification must fail.
        let entries = vec![(0_i64, [9; 32], "replay/c0.toml".to_string())];
        let err = load_course_replays(&entries, &dir, |_| Ok(Some(9))).unwrap_err();
        assert!(err.to_string().contains("replay chart hash"));

        // And missing chart bails out with a clear error.
        let entries = vec![(0_i64, [1; 32], "replay/c0.toml".to_string())];
        let err = load_course_replays(&entries, &dir, |_| Ok(None)).unwrap_err();
        assert!(err.to_string().contains("no longer in the library"));

        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn load_replay_for_chart_returns_full_replay() {
        let path = std::env::temp_dir().join(format!(
            "bmz-replay-load-full-{}-{}.toml",
            std::process::id(),
            TimeUs(46).0
        ));
        let pattern = vec![1, 0, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15];
        let replay = ReplayFile::new(
            [9; 32],
            1_700_000_070,
            Some(42),
            ArrangeOption::Mirror,
            Some(42),
            Some(pattern.clone()),
            Vec::new(),
        );
        save_replay(&path, &replay).unwrap();

        let loaded = load_replay_for_chart(&path, [9; 32]).unwrap();

        assert_eq!(loaded.arrange, "Mirror");
        assert_eq!(loaded.lane_shuffle_pattern, Some(pattern));

        std::fs::remove_file(path).unwrap();
    }
}

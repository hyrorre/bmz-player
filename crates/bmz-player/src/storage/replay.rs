use std::io::Write;
use std::path::Path;

use anyhow::{Result, bail};
use bmz_core::replay::ReplayEvent;
use bmz_gameplay::replay::ReplayPlayer;
use bmz_gameplay::rule::RuleMode;
use serde::{Deserialize, Serialize};

use crate::ln_policy::LnScorePolicy;
use crate::select_options::{ArrangeOption, DoubleOption, DoubleOptionScoreBucket};

pub const REPLAY_FILE_VERSION: u32 = 3;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplayFile {
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
    #[serde(default)]
    pub arrange_seed: Option<i64>,
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
        events: Vec<ReplayEvent>,
    ) -> Self {
        Self {
            version: REPLAY_FILE_VERSION,
            chart_sha256: hex_encode(&chart_sha256),
            ln_policy: ln_policy.as_str().to_string(),
            played_at,
            random_seed,
            arrange: arrange.to_persistent_str().to_string(),
            arrange_2p: arrange_2p.to_persistent_str().to_string(),
            double_option: double_option.as_str().to_string(),
            arrange_seed,
            lane_shuffle_pattern,
            events,
        }
    }

    pub fn arrange_option(&self) -> ArrangeOption {
        ArrangeOption::from_persistent_str(&self.arrange)
    }

    pub fn arrange_2p_option(&self) -> ArrangeOption {
        ArrangeOption::from_persistent_str(&self.arrange_2p)
    }

    pub fn double_option(&self) -> DoubleOption {
        match DoubleOptionScoreBucket::from_str_or_off(&self.double_option) {
            DoubleOptionScoreBucket::Off => DoubleOption::Off,
            DoubleOptionScoreBucket::Battle => DoubleOption::Battle,
            DoubleOptionScoreBucket::BattleAutoScratch => DoubleOption::BattleAutoScratch,
        }
    }

    pub fn double_option_bucket(&self) -> DoubleOptionScoreBucket {
        DoubleOptionScoreBucket::from_str_or_off(&self.double_option)
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
        file.sync_all()?;
    }
    std::fs::rename(tmp_path, path)?;
    Ok(hash)
}

pub fn load_replay(path: &Path) -> Result<ReplayFile> {
    let text = std::fs::read_to_string(path)?;
    Ok(toml::from_str(&text)?)
}

pub fn load_replay_player(path: &Path) -> Result<ReplayPlayer> {
    let replay = load_replay(path)?;
    Ok(ReplayPlayer { events: replay.events, next_index: 0 })
}

pub fn load_replay_player_for_chart(path: &Path, chart_sha256: [u8; 32]) -> Result<ReplayPlayer> {
    let replay = load_replay_for_chart(path, chart_sha256)?;
    Ok(ReplayPlayer { events: replay.events, next_index: 0 })
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
    for (index, chunk) in value.as_bytes().chunks_exact(2).enumerate() {
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
    use bmz_core::input::{InputDeviceKind, InputKind};
    use bmz_core::lane::Lane;
    use bmz_core::time::TimeUs;

    use super::*;

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
        assert_eq!(loaded.events, replay.events);

        std::fs::remove_file(path).unwrap();
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
        );

        save_replay(&path, &replay).unwrap();
        let loaded = load_replay(&path).unwrap();

        assert_eq!(loaded.arrange, "Random");
        assert_eq!(loaded.arrange_option(), ArrangeOption::Random);
        assert_eq!(loaded.arrange_seed, Some(7777));
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
        assert_eq!(loaded.events.len(), 0);
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

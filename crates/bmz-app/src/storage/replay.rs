use std::io::Write;
use std::path::Path;

use anyhow::Result;
use bmz_core::replay::ReplayEvent;
use bmz_gameplay::replay::ReplayPlayer;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplayFile {
    pub version: u32,
    pub chart_sha256: String,
    pub played_at: i64,
    pub random_seed: Option<i64>,
    pub events: Vec<ReplayEvent>,
}

impl ReplayFile {
    pub fn new(
        chart_sha256: [u8; 32],
        played_at: i64,
        random_seed: Option<i64>,
        events: Vec<ReplayEvent>,
    ) -> Self {
        Self { version: 1, chart_sha256: hex_encode(&chart_sha256), played_at, random_seed, events }
    }
}

pub fn save_replay(path: &Path, replay: &ReplayFile) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let tmp_path = path.with_extension("tmp");
    {
        let mut file = std::fs::File::create(&tmp_path)?;
        file.write_all(toml::to_string_pretty(replay)?.as_bytes())?;
        file.sync_all()?;
    }
    std::fs::rename(tmp_path, path)?;
    Ok(())
}

pub fn load_replay(path: &Path) -> Result<ReplayFile> {
    let text = std::fs::read_to_string(path)?;
    Ok(toml::from_str(&text)?)
}

pub fn load_replay_player(path: &Path) -> Result<ReplayPlayer> {
    let replay = load_replay(path)?;
    Ok(ReplayPlayer { events: replay.events, next_index: 0 })
}

pub fn replay_file_name(chart_sha256: [u8; 32], played_at: i64) -> String {
    format!("{}-{played_at}.toml", hex_encode(&chart_sha256))
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

#[cfg(test)]
mod tests {
    use bmz_core::input::InputKind;
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
            vec![ReplayEvent { lane: Lane::Key1, kind: InputKind::Press, time: TimeUs(1_000) }],
        );

        save_replay(&path, &replay).unwrap();
        let loaded = load_replay(&path).unwrap();

        assert_eq!(loaded.version, 1);
        assert_eq!(
            loaded.chart_sha256,
            "0101010101010101010101010101010101010101010101010101010101010101"
        );
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
            vec![ReplayEvent { lane: Lane::Key2, kind: InputKind::Release, time: TimeUs(2_000) }],
        );
        save_replay(&path, &replay).unwrap();

        let player = load_replay_player(&path).unwrap();

        assert_eq!(player.next_index, 0);
        assert_eq!(player.events, replay.events);

        std::fs::remove_file(path).unwrap();
    }
}

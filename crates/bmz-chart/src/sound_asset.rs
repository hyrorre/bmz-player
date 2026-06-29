use std::path::{Path, PathBuf};

/// beatoraja `AudioDriver.getPaths()` と同じ `#WAV` 拡張子探索順。
pub const BEATORAJA_SOUND_EXTENSIONS: &[&str] = &["wav", "flac", "ogg", "mp3"];

/// `#WAV` 指定パスから、存在する音声候補を beatoraja と同じ順で返す。
///
/// 指定ファイルが存在する場合も先頭候補に入れ、その後に同じ stem の別拡張子を
/// `wav -> flac -> ogg -> mp3` で追加する。
pub fn sound_asset_candidates(path: &Path) -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    if path.exists() {
        candidates.push(path.to_path_buf());
    }

    let original_ext = path.extension().and_then(|ext| ext.to_str());
    for candidate_ext in BEATORAJA_SOUND_EXTENSIONS {
        if original_ext == Some(*candidate_ext) {
            continue;
        }
        let candidate = path.with_extension(candidate_ext);
        if candidate.exists() {
            candidates.push(candidate);
        }
    }

    candidates
}

pub fn sound_asset_exists(path: &Path) -> bool {
    !sound_asset_candidates(path).is_empty()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn candidates_follow_beatoraja_audio_order() {
        let dir = temp_dir("audio-order");
        let requested = dir.join("foo.wav");
        let flac = dir.join("foo.flac");
        let ogg = dir.join("foo.ogg");
        let mp3 = dir.join("foo.mp3");
        std::fs::write(&ogg, b"dummy").unwrap();
        std::fs::write(&flac, b"dummy").unwrap();
        std::fs::write(&mp3, b"dummy").unwrap();

        assert_eq!(sound_asset_candidates(&requested), vec![flac, ogg, mp3]);

        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn candidates_keep_declared_file_first() {
        let dir = temp_dir("declared-first");
        let requested = dir.join("foo.ogg");
        let wav = dir.join("foo.wav");
        std::fs::write(&requested, b"dummy").unwrap();
        std::fs::write(&wav, b"dummy").unwrap();

        assert_eq!(sound_asset_candidates(&requested), vec![requested, wav]);

        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn missing_path_without_fallback_has_no_candidates() {
        let dir = temp_dir("missing");

        assert!(sound_asset_candidates(&dir.join("missing.wav")).is_empty());

        std::fs::remove_dir_all(dir).unwrap();
    }

    fn temp_dir(label: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "bmz-chart-sound-asset-{label}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }
}

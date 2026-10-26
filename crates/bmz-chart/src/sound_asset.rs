use std::path::{Path, PathBuf};

/// beatoraja `AudioDriver.getPaths()` と同じ `#WAV` 拡張子探索順。
pub const BEATORAJA_SOUND_EXTENSIONS: &[&str] = &["wav", "flac", "ogg", "mp3"];

/// `#WAV` 指定パスから、存在する音声候補を beatoraja と同じ順で返す。
///
/// 指定ファイルが存在する場合も先頭候補に入れ、その後に同じ stem の別拡張子を
/// `wav -> flac -> ogg -> mp3` で追加する。書庫内のパスは読み込み済みの書庫一覧で判定する。
pub fn sound_asset_candidates(path: &Path) -> Vec<PathBuf> {
    existing_candidates(path).collect()
}

/// Finds the first candidate without stat-ing later fallbacks. A decoder that
/// fails must then request the full list to retain beatoraja's fallback order.
pub fn first_sound_asset_candidate(path: &Path) -> Option<PathBuf> {
    existing_candidates(path).next()
}

fn existing_candidates(path: &Path) -> impl Iterator<Item = PathBuf> + '_ {
    let original_ext = path.extension().and_then(|ext| ext.to_str());
    std::iter::once(path.to_path_buf())
        .chain(
            BEATORAJA_SOUND_EXTENSIONS
                .iter()
                .copied()
                .filter(move |extension| original_ext != Some(*extension))
                .map(move |extension| path.with_extension(extension)),
        )
        .filter(|candidate| bmz_song_resource::file_exists(candidate))
}

pub fn sound_asset_exists(path: &Path) -> bool {
    first_sound_asset_candidate(path).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn candidates_follow_beatoraja_audio_order() {
        let temp = temp_dir("audio-order");
        let dir = temp.path().to_path_buf();
        let requested = dir.join("foo.wav");
        let flac = dir.join("foo.flac");
        let ogg = dir.join("foo.ogg");
        let mp3 = dir.join("foo.mp3");
        std::fs::write(&ogg, b"dummy").unwrap();
        std::fs::write(&flac, b"dummy").unwrap();
        std::fs::write(&mp3, b"dummy").unwrap();

        assert_eq!(first_sound_asset_candidate(&requested), Some(flac.clone()));
        assert_eq!(sound_asset_candidates(&requested), vec![flac, ogg, mp3]);
    }

    #[test]
    fn candidates_keep_declared_file_first() {
        let temp = temp_dir("declared-first");
        let dir = temp.path().to_path_buf();
        let requested = dir.join("foo.ogg");
        let wav = dir.join("foo.wav");
        std::fs::write(&requested, b"dummy").unwrap();
        std::fs::write(&wav, b"dummy").unwrap();

        assert_eq!(first_sound_asset_candidate(&requested), Some(requested.clone()));
        assert_eq!(sound_asset_candidates(&requested), vec![requested, wav]);
    }

    #[test]
    fn missing_path_without_fallback_has_no_candidates() {
        let temp = temp_dir("missing");
        let dir = temp.path().to_path_buf();

        assert!(sound_asset_candidates(&dir.join("missing.wav")).is_empty());
    }

    fn temp_dir(label: &str) -> tempfile::TempDir {
        tempfile::Builder::new()
            .prefix(&format!("bmz-chart-sound-asset-{label}-"))
            .tempdir()
            .unwrap()
    }
}

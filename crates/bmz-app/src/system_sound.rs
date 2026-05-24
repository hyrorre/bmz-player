//! beatoraja 互換のシステム SE / BGM 管理。
//!
//! beatoraja の `SystemSoundManager` (`.local/beatoraja/src/bms/player/beatoraja/SystemSoundManager.java`)
//! に倣い、以下を扱う。
//!
//! - 起動時に「BGM セット」と「SE セット」のディレクトリツリーをスキャンして候補を集める。
//! - BGM セットは `select.wav` を含むディレクトリ、SE セットは `clear.wav` を含むディレクトリ。
//! - 起動時にランダムに 1 セットずつ選んで、各 [`SoundType`] のファイルパスを解決する。
//! - 解決できないファイルは `defaultsound/<filename>` をフォールバック検索する。
//!
//! 本モジュールは「どのファイルを使うか」までを決めるところまでが責務。
//! 実際の AudioEngine への投入や再生は呼び出し側で行う。

use std::path::{Path, PathBuf};

/// beatoraja の `SystemSoundManager.SoundType` と対応する列挙体。
/// `path` は beatoraja 既定のファイル名、`is_bgm` はループ再生対象かどうか。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SoundType {
    Scratch,
    FolderOpen,
    FolderClose,
    OptionChange,
    OptionOpen,
    OptionClose,
    PlayReady,
    PlayStop,
    ResultClear,
    ResultFail,
    ResultClose,
    CourseClear,
    CourseFail,
    CourseClose,
    GuideSePGreat,
    GuideSeGreat,
    GuideSeGood,
    GuideSeBad,
    GuideSePoor,
    GuideSeMiss,
    /// 選曲画面 BGM(ループ)。
    Select,
    /// Decide シーン BGM(ループ)。
    Decide,
}

impl SoundType {
    pub const ALL: [SoundType; 22] = [
        SoundType::Scratch,
        SoundType::FolderOpen,
        SoundType::FolderClose,
        SoundType::OptionChange,
        SoundType::OptionOpen,
        SoundType::OptionClose,
        SoundType::PlayReady,
        SoundType::PlayStop,
        SoundType::ResultClear,
        SoundType::ResultFail,
        SoundType::ResultClose,
        SoundType::CourseClear,
        SoundType::CourseFail,
        SoundType::CourseClose,
        SoundType::GuideSePGreat,
        SoundType::GuideSeGreat,
        SoundType::GuideSeGood,
        SoundType::GuideSeBad,
        SoundType::GuideSePoor,
        SoundType::GuideSeMiss,
        SoundType::Select,
        SoundType::Decide,
    ];

    /// beatoraja 既定のファイル名(セットディレクトリ直下から探す)。
    pub fn file_name(&self) -> &'static str {
        match self {
            SoundType::Scratch => "scratch.wav",
            SoundType::FolderOpen => "f-open.wav",
            SoundType::FolderClose => "f-close.wav",
            SoundType::OptionChange => "o-change.wav",
            SoundType::OptionOpen => "o-open.wav",
            SoundType::OptionClose => "o-close.wav",
            SoundType::PlayReady => "playready.wav",
            SoundType::PlayStop => "playstop.wav",
            SoundType::ResultClear => "clear.wav",
            SoundType::ResultFail => "fail.wav",
            SoundType::ResultClose => "resultclose.wav",
            SoundType::CourseClear => "course_clear.wav",
            SoundType::CourseFail => "course_fail.wav",
            SoundType::CourseClose => "course_close.wav",
            SoundType::GuideSePGreat => "guide-pg.wav",
            SoundType::GuideSeGreat => "guide-gr.wav",
            SoundType::GuideSeGood => "guide-gd.wav",
            SoundType::GuideSeBad => "guide-bd.wav",
            SoundType::GuideSePoor => "guide-pr.wav",
            SoundType::GuideSeMiss => "guide-ms.wav",
            SoundType::Select => "select.wav",
            SoundType::Decide => "decide.wav",
        }
    }

    /// BGM (ループ再生)対象かどうか。
    pub fn is_bgm(&self) -> bool {
        matches!(self, SoundType::Select | SoundType::Decide)
    }
}

/// スキャンで選ばれた1つの BGM セット / SE セットディレクトリ。
#[derive(Debug, Clone, Default)]
pub struct SoundSetSelection {
    /// BGM セットディレクトリ。`select.wav` を含むディレクトリ。
    pub bgm_dir: Option<PathBuf>,
    /// SE セットディレクトリ。`clear.wav` を含むディレクトリ。
    pub se_dir: Option<PathBuf>,
    /// `defaultsound/` のパス。各ファイルのフォールバック検索に使う。
    pub default_dir: Option<PathBuf>,
}

impl SoundSetSelection {
    /// `sound_type` に対応するファイルパスを解決する。
    ///
    /// 解決順は beatoraja と同じ:
    /// 1. BGM か SE に応じたセットディレクトリ直下の `file_name`。
    /// 2. `default_dir/file_name`。
    /// 3. 上記がいずれも存在しなければ `None`。
    pub fn resolve(&self, sound_type: SoundType) -> Option<PathBuf> {
        let dir =
            if sound_type.is_bgm() { self.bgm_dir.as_deref() } else { self.se_dir.as_deref() };
        if let Some(dir) = dir {
            let candidate = dir.join(sound_type.file_name());
            if candidate.is_file() {
                return Some(candidate);
            }
        }
        if let Some(default) = self.default_dir.as_deref() {
            let candidate = default.join(sound_type.file_name());
            if candidate.is_file() {
                return Some(candidate);
            }
        }
        None
    }
}

/// `root` 配下を再帰的に走査し、`marker_filename` を含むディレクトリのリストを返す。
/// beatoraja の `SystemSoundManager.scan` と同じ振る舞い。
pub fn scan_sound_sets(root: &Path, marker_filename: &str) -> Vec<PathBuf> {
    let mut out = Vec::new();
    scan_sound_sets_into(root, marker_filename, &mut out);
    out
}

fn scan_sound_sets_into(dir: &Path, marker_filename: &str, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut has_marker = false;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            scan_sound_sets_into(&path, marker_filename, out);
        } else if !has_marker && path.file_name().and_then(|n| n.to_str()) == Some(marker_filename)
        {
            has_marker = true;
        }
    }
    if has_marker {
        out.push(dir.to_path_buf());
    }
}

/// `bgms` と `ses` からランダムに 1 セットずつ選んで [`SoundSetSelection`] を作る。
/// 候補が空ならそれぞれ `None`。`default_dir` はそのまま転写する。
pub fn select_random_sound_set(
    bgms: &[PathBuf],
    ses: &[PathBuf],
    default_dir: Option<PathBuf>,
) -> SoundSetSelection {
    SoundSetSelection { bgm_dir: pick_random(bgms), se_dir: pick_random(ses), default_dir }
}

fn pick_random(paths: &[PathBuf]) -> Option<PathBuf> {
    if paths.is_empty() {
        return None;
    }
    // `rand` 依存を増やしたくないので、Unix エポックナノ秒からの剰余で擬似ランダム選択。
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let index = (nanos as usize) % paths.len();
    Some(paths[index].clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(label: &str) -> PathBuf {
        let stamp =
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let path = std::env::temp_dir()
            .join(format!("bmz-system-sound-{label}-{}-{stamp}", std::process::id()));
        std::fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn sound_type_classification() {
        assert!(SoundType::Select.is_bgm());
        assert!(SoundType::Decide.is_bgm());
        assert!(!SoundType::Scratch.is_bgm());
        assert_eq!(SoundType::Scratch.file_name(), "scratch.wav");
        assert_eq!(SoundType::ResultClear.file_name(), "clear.wav");
    }

    #[test]
    fn scan_sound_sets_finds_directories_with_marker_file() {
        let root = temp_dir("scan-root");
        let set_a = root.join("set-a");
        let set_b = root.join("nested").join("set-b");
        let no_marker = root.join("empty");
        std::fs::create_dir_all(&set_a).unwrap();
        std::fs::create_dir_all(&set_b).unwrap();
        std::fs::create_dir_all(&no_marker).unwrap();
        std::fs::write(set_a.join("select.wav"), b"x").unwrap();
        std::fs::write(set_b.join("select.wav"), b"x").unwrap();
        // no marker file in `empty/`

        let mut found = scan_sound_sets(&root, "select.wav");
        found.sort();
        let mut expected = vec![set_a, set_b];
        expected.sort();

        assert_eq!(found, expected);

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn resolve_prefers_set_dir_then_falls_back_to_default_dir() {
        let bgm_dir = temp_dir("resolve-bgm");
        let default_dir = temp_dir("resolve-default");
        std::fs::write(bgm_dir.join("select.wav"), b"x").unwrap();
        std::fs::write(default_dir.join("scratch.wav"), b"x").unwrap();
        std::fs::write(default_dir.join("decide.wav"), b"x").unwrap();

        let selection = SoundSetSelection {
            bgm_dir: Some(bgm_dir.clone()),
            se_dir: None,
            default_dir: Some(default_dir.clone()),
        };

        // BGM (select.wav) は bgm_dir から解決される。
        assert_eq!(selection.resolve(SoundType::Select), Some(bgm_dir.join("select.wav")));
        // SE (scratch.wav) は se_dir が None なので default_dir から解決される。
        assert_eq!(selection.resolve(SoundType::Scratch), Some(default_dir.join("scratch.wav")));
        // BGM (decide.wav) は bgm_dir に無いので default_dir フォールバック。
        assert_eq!(selection.resolve(SoundType::Decide), Some(default_dir.join("decide.wav")));
        // 一切無いものは None。
        assert_eq!(selection.resolve(SoundType::ResultClear), None);

        std::fs::remove_dir_all(bgm_dir).unwrap();
        std::fs::remove_dir_all(default_dir).unwrap();
    }

    #[test]
    fn select_random_returns_none_when_no_candidates() {
        let selection = select_random_sound_set(&[], &[], None);
        assert!(selection.bgm_dir.is_none());
        assert!(selection.se_dir.is_none());
        assert!(selection.default_dir.is_none());
    }

    #[test]
    fn select_random_picks_a_candidate_when_present() {
        let bgm = vec![PathBuf::from("/bgm/set1")];
        let se = vec![PathBuf::from("/se/set1"), PathBuf::from("/se/set2")];
        let default = PathBuf::from("/default");

        let selection = select_random_sound_set(&bgm, &se, Some(default.clone()));

        assert_eq!(selection.bgm_dir.as_deref(), Some(bgm[0].as_path()));
        assert!(se.iter().any(|p| Some(p.as_path()) == selection.se_dir.as_deref()));
        assert_eq!(selection.default_dir.as_deref(), Some(default.as_path()));
    }
}

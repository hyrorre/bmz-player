//! beatoraja 互換のシステム SE / BGM 管理。
//!
//! beatoraja の `SystemSoundManager` (`.local/beatoraja/src/bms/player/beatoraja/SystemSoundManager.java`)
//! に倣い、以下を扱う。
//!
//! - 起動時に「BGM セット」と「SE セット」のディレクトリツリーをスキャンして候補を集める。
//! - BGMセットは対応音源を直接含むディレクトリと、その直下にあるバリエーション。
//! - 初期化時と選曲画面へ戻るたび、rootとバリエーションを二段階で選び、各 [`SoundType`] の
//!   ファイルパスを解決する。
//! - SEはサウンドセット、SEセット、`defaultsound/` の順に解決する。
//!   RESULT入口音はサウンドセットではBGM、SEセットと既定音源では単発SEとして扱う。
//! - RESULT BGMはサウンドセット内だけから解決し、`.loop` 付きならループする。
//!
//! 本モジュールは「どのファイルを使うか」までを決めるところまでが責務。
//! 実際の AudioEngine への投入や再生は呼び出し側で行う。

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use bmz_core::judge::Judge;

/// beatoraja の `SystemSoundManager.SoundType` と対応する列挙体。
/// `path` は beatoraja 既定のファイル名、`is_bgm` は BGM セット探索対象かどうか。
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
    /// Decide シーン BGM(単発)。
    Decide,
    /// 地雷ノーツを踏んだときの固定 SE (beatoraja の `defaultsound/landmine.wav` 相当)。
    Landmine,
    /// サウンドセット内のRESULT専用BGM。従来のclear/fail SEとは別の音種。
    ResultBgmClear,
    ResultBgmFail,
    ResultBgmA,
    ResultBgmAA,
    ResultBgmAAA,
    /// 補完用SEセット・既定音源内のランク別RESULT SE（単発）。
    ResultA,
    ResultAA,
    ResultAAA,
}

impl SoundType {
    pub const ALL: [SoundType; 31] = [
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
        SoundType::Landmine,
        SoundType::ResultBgmClear,
        SoundType::ResultBgmFail,
        SoundType::ResultBgmA,
        SoundType::ResultBgmAA,
        SoundType::ResultBgmAAA,
        SoundType::ResultA,
        SoundType::ResultAA,
        SoundType::ResultAAA,
    ];

    pub const RESULT_BGMS: [SoundType; 5] = [
        SoundType::ResultBgmClear,
        SoundType::ResultBgmFail,
        SoundType::ResultBgmA,
        SoundType::ResultBgmAA,
        SoundType::ResultBgmAAA,
    ];

    /// beatoraja 既定のファイル名(セットディレクトリ直下から探す)。
    /// 戻り値はデフォルトの `.wav` 拡張子付きだが、実ファイルは [`SUPPORTED_EXTENSIONS`]
    /// のいずれの拡張子でも解決される(beatoraja と同じく `.ogg` 等もサポート)。
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
            SoundType::Landmine => "landmine.wav",
            SoundType::ResultBgmClear => "clear.wav",
            SoundType::ResultBgmFail => "fail.wav",
            SoundType::ResultBgmA => "a.wav",
            SoundType::ResultBgmAA => "aa.wav",
            SoundType::ResultBgmAAA => "aaa.wav",
            SoundType::ResultA => "a.wav",
            SoundType::ResultAA => "aa.wav",
            SoundType::ResultAAA => "aaa.wav",
        }
    }

    /// `file_name()` の拡張子を除いたステム(`"scratch.wav"` → `"scratch"`)。
    fn stem(&self) -> &'static str {
        let name = self.file_name();
        match name.rfind('.') {
            Some(idx) => &name[..idx],
            None => name,
        }
    }

    /// BGM セット探索とシーン遷移時の停止対象かどうか。
    pub fn is_bgm(&self) -> bool {
        matches!(self, SoundType::Select | SoundType::Decide) || self.is_result_bgm()
    }

    pub fn is_result_bgm(&self) -> bool {
        Self::RESULT_BGMS.contains(self)
    }

    pub fn loops(&self) -> bool {
        matches!(self, SoundType::Select)
    }
}

/// beatoraja の GUIDE SE と同じく、押下判定の POOR と見逃し MISS を分ける。
pub const fn guide_se_for_judge(judge: Judge) -> SoundType {
    match judge {
        Judge::PGreat => SoundType::GuideSePGreat,
        Judge::Great => SoundType::GuideSeGreat,
        Judge::Good => SoundType::GuideSeGood,
        Judge::Bad => SoundType::GuideSeBad,
        Judge::EmptyPoor => SoundType::GuideSePoor,
        Judge::Poor => SoundType::GuideSeMiss,
    }
}

/// システム SE / BGM の探索対象拡張子。先頭から順に試す。
/// beatoraja の挙動と合わせて `.wav` / `.ogg` / `.flac` / `.mp3` をサポート。
pub const SUPPORTED_EXTENSIONS: &[&str] = &["wav", "ogg", "flac", "mp3"];

/// 1ディレクトリ内の対応音源を、ASCII小文字化したstemごとにまとめた索引。
/// 全音種の解決でディレクトリを1回だけ列挙し、対応拡張子の名前だけをファイル判定する。
#[derive(Debug, Default)]
struct SoundFileIndex {
    by_stem: HashMap<String, Vec<(usize, PathBuf)>>,
}

impl SoundFileIndex {
    fn read(dir: &Path) -> Self {
        let mut by_stem: HashMap<String, Vec<(usize, PathBuf)>> = HashMap::new();
        let Ok(entries) = std::fs::read_dir(dir) else { return Self { by_stem } };
        for entry in entries.flatten() {
            let path = entry.path();
            let (Some(stem), Some(extension)) = (
                path.file_stem().and_then(|name| name.to_str()),
                path.extension().and_then(|extension| extension.to_str()),
            ) else {
                continue;
            };
            let Some(priority) = SUPPORTED_EXTENSIONS
                .iter()
                .position(|supported| extension.eq_ignore_ascii_case(supported))
            else {
                continue;
            };
            // file_typeはsymlinkを辿らないため、linkだけ実体を確認する。
            let is_file = match entry.file_type() {
                Ok(file_type) if file_type.is_symlink() => path.is_file(),
                Ok(file_type) => file_type.is_file(),
                Err(_) => false,
            };
            if is_file {
                by_stem.entry(stem.to_ascii_lowercase()).or_default().push((priority, path));
            }
        }
        Self { by_stem }
    }

    /// `<stem>.<ext>` を拡張子優先順で返す。ASCIIの大小文字はOSに関係なく許容し、
    /// 同名の表記違いが共存する場合は正式な小文字名を優先する。
    fn files(&self, stem: &str) -> Vec<PathBuf> {
        let Some(files) = self.by_stem.get(&stem.to_ascii_lowercase()) else {
            return Vec::new();
        };
        let mut candidates = files.clone();
        sort_sound_candidates(&mut candidates, stem);
        candidates.into_iter().map(|(_, path)| path).collect()
    }
}

/// 同じ拡張子内では期待される小文字名を優先し、残る大小文字違いは名前順に固定する。
fn sort_sound_candidates(candidates: &mut [(usize, PathBuf)], stem: &str) {
    let preferred_names = SUPPORTED_EXTENSIONS
        .iter()
        .map(|extension| format!("{stem}.{extension}"))
        .collect::<Vec<_>>();
    candidates.sort_by(|(left_priority, left_path), (right_priority, right_path)| {
        let left_name = left_path.file_name().and_then(|name| name.to_str()).unwrap_or_default();
        let right_name = right_path.file_name().and_then(|name| name.to_str()).unwrap_or_default();
        let left_preferred = preferred_names.iter().any(|name| name == left_name);
        let right_preferred = preferred_names.iter().any(|name| name == right_name);
        left_priority
            .cmp(right_priority)
            .then_with(|| right_preferred.cmp(&left_preferred))
            .then_with(|| left_name.cmp(right_name))
    });
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedSystemSound {
    pub path: PathBuf,
    pub loop_playback: bool,
}

/// BGMセットrootと、その直下にあるバリエーション候補。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SoundSetRoot {
    pub dir: PathBuf,
    pub variants: Vec<PathBuf>,
}

/// スキャンで選ばれた1つのBGMセット/バリエーションとSEセット。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SoundSetSelection {
    /// サウンドセットroot。対応するシステム音源を1つ以上含む。
    pub bgm_dir: Option<PathBuf>,
    /// root直下から抽選されたバリエーション。全音種で共通して使う。
    pub bgm_variant_dir: Option<PathBuf>,
    /// SE セットディレクトリ。`clear.wav` を含むディレクトリ。
    pub se_dir: Option<PathBuf>,
    /// `defaultsound/` のパス。各ファイルのフォールバック検索に使う。
    pub default_dir: Option<PathBuf>,
}

/// 起動時にスキャンした BGM / SE セット候補。
///
/// beatoraja と同様に候補のスキャンは起動時だけ行い、選曲画面へ戻る際はこの一覧から
/// [`SoundSetSelection`] を再抽選する。
#[derive(Debug, Clone, Default)]
pub struct SoundSetCatalog {
    pub bgm_roots: Vec<SoundSetRoot>,
    pub se_dirs: Vec<PathBuf>,
    pub default_dir: Option<PathBuf>,
}

impl SoundSetCatalog {
    pub fn select_random(&self) -> SoundSetSelection {
        select_random_sound_set(&self.bgm_roots, &self.se_dirs, self.default_dir.clone())
    }
}

impl SoundSetSelection {
    /// `sound_type` に対応するファイルパスを解決する。
    ///
    /// 優先順で最初の既存パスを返す。decode失敗時の補完には [`Self::candidates`] を使う。
    pub fn resolve(&self, sound_type: SoundType) -> Option<PathBuf> {
        self.candidates(sound_type).into_iter().next().map(|sound| sound.path)
    }

    /// 選択されたchild、root、既存fallbackの順で各音源を解決する。
    /// RESULT BGMは各layer内で`.loop`版、通常版の順に解決する。
    /// ただしRESULT入口SEはSEセットとdefaultだけから解決する（セット内の同名音源はBGM）。
    /// 各段階で拡張子優先順を保ち、読み込みに失敗した候補の次を試せるよう全候補を返す。
    pub fn candidates(&self, sound_type: SoundType) -> Vec<ResolvedSystemSound> {
        SoundSetIndex::read(self).candidates(sound_type)
    }

    /// [`SoundType::ALL`] の順に全音種の候補を返す。各ディレクトリの列挙は1回だけ行う。
    pub fn candidates_for_all(&self) -> Vec<Vec<ResolvedSystemSound>> {
        let index = SoundSetIndex::read(self);
        SoundType::ALL.iter().map(|sound_type| index.candidates(*sound_type)).collect()
    }
}

/// [`SoundSetSelection`] の各layerを1回ずつ列挙した索引。
struct SoundSetIndex {
    variant: Option<SoundFileIndex>,
    root: Option<SoundFileIndex>,
    se: Option<SoundFileIndex>,
    default: Option<SoundFileIndex>,
}

impl SoundSetIndex {
    fn read(selection: &SoundSetSelection) -> Self {
        let read = |dir: &Option<PathBuf>| dir.as_deref().map(SoundFileIndex::read);
        Self {
            variant: read(&selection.bgm_variant_dir),
            root: read(&selection.bgm_dir),
            se: read(&selection.se_dir),
            default: read(&selection.default_dir),
        }
    }

    fn candidates(&self, sound_type: SoundType) -> Vec<ResolvedSystemSound> {
        let stem = sound_type.stem();
        let mut candidates = Vec::new();
        let soundset_layers = [self.variant.as_ref(), self.root.as_ref()];
        let include_soundset = !matches!(
            sound_type,
            SoundType::ResultClear
                | SoundType::ResultFail
                | SoundType::ResultA
                | SoundType::ResultAA
                | SoundType::ResultAAA
        );
        if include_soundset {
            for layer in soundset_layers.into_iter().flatten() {
                if sound_type.is_result_bgm() {
                    candidates.extend(
                        layer
                            .files(&format!("{stem}.loop"))
                            .into_iter()
                            .map(|path| ResolvedSystemSound { path, loop_playback: true }),
                    );
                }
                candidates.extend(
                    layer.files(stem).into_iter().map(|path| ResolvedSystemSound {
                        path,
                        loop_playback: sound_type.loops(),
                    }),
                );
            }
        }
        if sound_type.is_result_bgm() {
            return candidates;
        }
        let fallback_layers =
            [(!sound_type.is_bgm()).then_some(self.se.as_ref()).flatten(), self.default.as_ref()];
        for layer in fallback_layers.into_iter().flatten() {
            for path in layer.files(stem) {
                if !candidates.iter().any(|candidate| candidate.path == path) {
                    candidates
                        .push(ResolvedSystemSound { path, loop_playback: sound_type.loops() });
                }
            }
        }
        candidates
    }
}

/// BGM設定root配下から、音源を直接含む最初のディレクトリごとにrootを検出する。
/// 音源を含まない整理用階層だけを再帰し、root直下の音源ディレクトリはvariantsにする。
pub fn scan_bgm_sound_sets(root: &Path) -> Vec<SoundSetRoot> {
    let mut roots = Vec::new();
    scan_bgm_sound_sets_into(
        root,
        &mut roots,
        &mut crate::directory_scan::DirectoryScan::default(),
    );
    roots.sort_by(|left, right| left.dir.cmp(&right.dir));
    roots
}

fn scan_bgm_sound_sets_into(
    dir: &Path,
    roots: &mut Vec<SoundSetRoot>,
    scan: &mut crate::directory_scan::DirectoryScan,
) {
    let Some(entries) = scan.read_dir_once(dir) else {
        return;
    };
    let entries = entries.flatten().map(|entry| entry.path()).collect::<Vec<_>>();
    let mut child_dirs = entries.iter().filter(|path| path.is_dir()).cloned().collect::<Vec<_>>();
    child_dirs.sort();
    if directory_has_supported_sound(&entries) {
        let variants = child_dirs
            .into_iter()
            .filter(|path| {
                scan.read_dir_once(path).is_some_and(|children| {
                    directory_has_supported_sound(
                        &children.flatten().map(|entry| entry.path()).collect::<Vec<_>>(),
                    )
                })
            })
            .collect();
        roots.push(SoundSetRoot { dir: dir.to_path_buf(), variants });
        return;
    }
    for child in child_dirs {
        scan_bgm_sound_sets_into(&child, roots, scan);
    }
}

fn directory_has_supported_sound(entries: &[PathBuf]) -> bool {
    entries.iter().any(|path| {
        path.is_file()
            && SoundType::ALL.iter().any(|sound_type| is_supported_sound_path(path, *sound_type))
    })
}

fn is_supported_sound_path(path: &Path, sound_type: SoundType) -> bool {
    let Some(stem) = path.file_stem().and_then(|name| name.to_str()) else {
        return false;
    };
    let Some(extension) = path.extension().and_then(|extension| extension.to_str()) else {
        return false;
    };
    if !SUPPORTED_EXTENSIONS.iter().any(|supported| extension.eq_ignore_ascii_case(supported)) {
        return false;
    }
    stem.eq_ignore_ascii_case(sound_type.stem())
        || (sound_type.is_result_bgm()
            && stem.eq_ignore_ascii_case(&format!("{}.loop", sound_type.stem())))
}

/// `root` 配下を再帰的に走査し、`marker_filename` を含むディレクトリのリストを返す。
/// beatoraja の `SystemSoundManager.scan` と同じ振る舞い。`marker_filename` は
/// `.wav` 拡張子付きで渡し、実ファイルは [`SUPPORTED_EXTENSIONS`] のいずれでも
/// マーカーとして認識する。
pub fn scan_sound_sets(root: &Path, marker_filename: &str) -> Vec<PathBuf> {
    let marker_stem = match marker_filename.rfind('.') {
        Some(idx) => &marker_filename[..idx],
        None => marker_filename,
    };
    let mut out = Vec::new();
    scan_sound_sets_into(
        root,
        marker_stem,
        &mut out,
        &mut crate::directory_scan::DirectoryScan::default(),
    );
    out
}

fn scan_sound_sets_into(
    dir: &Path,
    marker_stem: &str,
    out: &mut Vec<PathBuf>,
    scan: &mut crate::directory_scan::DirectoryScan,
) {
    let Some(entries) = scan.read_dir_once(dir) else {
        return;
    };
    let mut has_marker = false;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            scan_sound_sets_into(&path, marker_stem, out, scan);
        } else if !has_marker && is_marker_file(&path, marker_stem) {
            has_marker = true;
        }
    }
    if has_marker {
        out.push(dir.to_path_buf());
    }
}

/// `path` のファイル名がステム `marker_stem` + [`SUPPORTED_EXTENSIONS`] のいずれか
/// に一致するか。ASCIIの大文字小文字は拡張子を含めて区別しない。
fn is_marker_file(path: &Path, marker_stem: &str) -> bool {
    let Some(stem) = path.file_stem().and_then(|n| n.to_str()) else {
        return false;
    };
    if !stem.eq_ignore_ascii_case(marker_stem) {
        return false;
    }
    let Some(ext) = path.extension().and_then(|e| e.to_str()) else {
        return false;
    };
    let ext_lower = ext.to_ascii_lowercase();
    SUPPORTED_EXTENSIONS.iter().any(|supported| *supported == ext_lower)
}

/// `bgm_roots` と `ses` からランダムに1セットずつ選んで [`SoundSetSelection`] を作る。
/// 候補が空ならそれぞれ `None`。`default_dir` はそのまま転写する。
pub fn select_random_sound_set(
    bgm_roots: &[SoundSetRoot],
    ses: &[PathBuf],
    default_dir: Option<PathBuf>,
) -> SoundSetSelection {
    let (bgm_dir, bgm_variant_dir) =
        select_bgm_set_with(bgm_roots, crate::random_index::random_index);
    SoundSetSelection { bgm_dir, bgm_variant_dir, se_dir: pick_random(ses), default_dir }
}

fn select_bgm_set_with(
    roots: &[SoundSetRoot],
    mut choose_index: impl FnMut(usize) -> Option<usize>,
) -> (Option<PathBuf>, Option<PathBuf>) {
    let Some(root_index) = choose_index(roots.len()).filter(|&index| index < roots.len()) else {
        return (None, None);
    };
    let root = &roots[root_index];
    let variant = choose_index(root.variants.len())
        .filter(|&index| index < root.variants.len())
        .map(|index| root.variants[index].clone());
    (Some(root.dir.clone()), variant)
}

fn pick_random(paths: &[PathBuf]) -> Option<PathBuf> {
    if paths.is_empty() {
        return None;
    }
    let index = crate::random_index::random_index(paths.len())?;
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
        assert!(SoundType::Select.loops());
        assert!(!SoundType::Decide.loops());
        assert!(!SoundType::Scratch.is_bgm());
        assert_eq!(SoundType::Scratch.file_name(), "scratch.wav");
        assert_eq!(SoundType::ResultClear.file_name(), "clear.wav");
        assert_eq!(SoundType::Landmine.file_name(), "landmine.wav");
        assert!(!SoundType::Landmine.is_bgm());
        for sound in SoundType::RESULT_BGMS {
            assert!(sound.is_bgm());
            assert!(!sound.loops());
        }
        for sound in [SoundType::ResultA, SoundType::ResultAA, SoundType::ResultAAA] {
            assert!(!sound.is_bgm());
            assert!(!sound.loops());
        }
    }

    #[test]
    fn guide_se_distinguishes_empty_poor_from_missed_note() {
        assert_eq!(guide_se_for_judge(Judge::PGreat), SoundType::GuideSePGreat);
        assert_eq!(guide_se_for_judge(Judge::Great), SoundType::GuideSeGreat);
        assert_eq!(guide_se_for_judge(Judge::Good), SoundType::GuideSeGood);
        assert_eq!(guide_se_for_judge(Judge::Bad), SoundType::GuideSeBad);
        assert_eq!(guide_se_for_judge(Judge::EmptyPoor), SoundType::GuideSePoor);
        assert_eq!(guide_se_for_judge(Judge::Poor), SoundType::GuideSeMiss);
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
    #[cfg(any(unix, windows))]
    fn scan_sound_sets_scans_directory_links_once_and_accepts_linked_root() {
        let fixture = crate::directory_scan::test_support::LinkedDirectories::new();
        std::fs::write(fixture.root.join("SELECT.MP3"), b"x").unwrap();
        std::fs::write(fixture.root.join("clear.wav"), b"x").unwrap();
        std::fs::write(fixture.nested.join("SeLeCt.FLAC"), b"x").unwrap();
        std::fs::write(fixture.nested.join("CLEAR.OGG"), b"x").unwrap();
        let mut expected =
            vec![fixture.root.canonicalize().unwrap(), fixture.nested.canonicalize().unwrap()];
        expected.sort();

        for root in [&fixture.root, &fixture.linked_root] {
            for marker in ["select.wav", "clear.wav", "clear"] {
                let found = scan_sound_sets(root, marker);
                assert_eq!(found.len(), 2);
                assert!(found.iter().all(|path| path.starts_with(root)));
                let mut canonical: Vec<_> =
                    found.iter().map(|path| path.canonicalize().unwrap()).collect();
                canonical.sort();
                assert_eq!(canonical, expected);
            }

            let bgm_sets = scan_bgm_sound_sets(root);
            assert_eq!(bgm_sets.len(), 1);
            assert_eq!(bgm_sets[0].dir, *root);
            assert_eq!(bgm_sets[0].variants.len(), 1);
            let variant = bgm_sets[0].variants[0].canonicalize().unwrap();
            assert_eq!(variant, fixture.nested.canonicalize().unwrap());
            assert_ne!(variant, root.canonicalize().unwrap());
        }
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
            bgm_variant_dir: None,
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
    fn resolve_matches_ascii_case_insensitive_names_on_every_platform() {
        let root = temp_dir("case-insensitive-name");
        let path = root.join("SCRATCH.WAV");
        std::fs::write(&path, b"fixture").unwrap();
        let selection = SoundSetSelection { bgm_dir: Some(root.clone()), ..Default::default() };
        assert_eq!(selection.resolve(SoundType::Scratch), Some(path));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn sound_candidates_prefer_lowercase_then_sort_names_stably() {
        // Synthetic paths exercise case-colliding names on filesystems that cannot
        // represent these entries together in a real directory.
        let mut candidates = [
            (1, PathBuf::from("scratch.ogg")),
            (0, PathBuf::from("Scratch.wav")),
            (0, PathBuf::from("scratch.WAV")),
            (0, PathBuf::from("SCRATCH.WAV")),
            (0, PathBuf::from("scratch.wav")),
        ];

        sort_sound_candidates(&mut candidates, "scratch");

        assert_eq!(
            candidates.into_iter().map(|(_, path)| path).collect::<Vec<_>>(),
            [
                PathBuf::from("scratch.wav"),
                PathBuf::from("SCRATCH.WAV"),
                PathBuf::from("Scratch.wav"),
                PathBuf::from("scratch.WAV"),
                PathBuf::from("scratch.ogg"),
            ]
        );
    }

    #[test]
    fn scan_sound_sets_matches_alternative_extensions() {
        // 代替拡張子とASCII大小文字の違いを、リンク権限なしで確認する。
        let root = temp_dir("scan-ogg");
        let set = root.join("modernchic");
        std::fs::create_dir_all(&set).unwrap();
        std::fs::write(set.join("CLEAR.OGG"), b"x").unwrap();
        std::fs::write(set.join("SeLeCt.MP3"), b"x").unwrap();

        assert_eq!(scan_sound_sets(&root, "clear.wav"), vec![set.clone()]);
        assert_eq!(scan_sound_sets(&root, "select.wav"), vec![set]);

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn scan_bgm_sound_sets_finds_se_roots_and_only_direct_variants() {
        let root = temp_dir("bgm-roots");
        let organize = root.join("organize");
        let nested_root = organize.join("deep").join("result-only-root");
        let se_root = organize.join("se-only-root");
        let result_variant = se_root.join("ResultOnly");
        let grandchild = result_variant.join("grandchild");
        let invalid_variant = se_root.join("invalid");
        let sound_named_directory = se_root.join("select.wav");
        std::fs::create_dir_all(&nested_root).unwrap();
        std::fs::create_dir_all(&grandchild).unwrap();
        std::fs::create_dir_all(&sound_named_directory).unwrap();
        std::fs::create_dir_all(invalid_variant.join("select.wav")).unwrap();
        std::fs::write(nested_root.join("AAA.FLAC"), b"x").unwrap();
        std::fs::write(se_root.join("SCRATCH.OGG"), b"x").unwrap();
        std::fs::write(result_variant.join("CLEAR.LOOP.OGG"), b"x").unwrap();
        std::fs::write(grandchild.join("select.wav"), b"x").unwrap();
        std::fs::write(invalid_variant.join("readme.ogg"), b"x").unwrap();
        std::fs::write(invalid_variant.join("scratch.loop.ogg"), b"x").unwrap();

        let sets = scan_bgm_sound_sets(&root);

        assert_eq!(
            sets,
            vec![
                SoundSetRoot { dir: nested_root, variants: vec![] },
                SoundSetRoot { dir: se_root, variants: vec![result_variant] },
            ]
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn selected_variant_precedes_parent_without_cross_variant_fallback() {
        let root = temp_dir("variant-priority");
        let parent = root.join("set");
        let child = parent.join("chosen");
        let sibling = parent.join("other");
        let se = root.join("se");
        let default = root.join("default");
        for dir in [&child, &sibling, &se, &default] {
            std::fs::create_dir_all(dir).unwrap();
        }
        for (dir, name) in [
            (&child, "clear.ogg"),
            (&parent, "clear.loop.ogg"),
            (&parent, "aaa.wav"),
            (&sibling, "decide.wav"),
            (&se, "clear.wav"),
            (&se, "scratch.wav"),
            (&default, "clear.wav"),
            (&default, "scratch.wav"),
        ] {
            std::fs::write(dir.join(name), b"fixture").unwrap();
        }
        let selection = SoundSetSelection {
            bgm_dir: Some(parent.clone()),
            bgm_variant_dir: Some(child.clone()),
            se_dir: Some(se.clone()),
            default_dir: Some(default.clone()),
        };

        assert_eq!(
            selection.candidates(SoundType::ResultBgmClear),
            vec![
                ResolvedSystemSound { path: child.join("clear.ogg"), loop_playback: false },
                ResolvedSystemSound { path: parent.join("clear.loop.ogg"), loop_playback: true },
            ]
        );
        assert_eq!(
            selection.candidates(SoundType::ResultBgmAAA),
            vec![ResolvedSystemSound { path: parent.join("aaa.wav"), loop_playback: false }]
        );
        assert_eq!(
            selection.candidates(SoundType::ResultClear),
            vec![
                ResolvedSystemSound { path: se.join("clear.wav"), loop_playback: false },
                ResolvedSystemSound { path: default.join("clear.wav"), loop_playback: false },
            ]
        );
        assert_eq!(
            selection.candidates(SoundType::Scratch),
            vec![
                ResolvedSystemSound { path: se.join("scratch.wav"), loop_playback: false },
                ResolvedSystemSound { path: default.join("scratch.wav"), loop_playback: false },
            ]
        );
        assert!(
            !selection
                .candidates(SoundType::Decide)
                .iter()
                .any(|candidate| candidate.path.starts_with(&sibling))
        );
        // 一括解決は、各layerの列挙を共有しても音種ごとの解決と同じ順序を返す。
        assert_eq!(
            selection.candidates_for_all(),
            SoundType::ALL.map(|sound| selection.candidates(sound)).to_vec()
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn resolve_falls_back_through_supported_extensions() {
        // se_dir に scratch.ogg がある場合、SoundType::Scratch.file_name() = "scratch.wav"
        // でも解決できること。
        let se_dir = temp_dir("resolve-ogg");
        std::fs::write(se_dir.join("scratch.ogg"), b"x").unwrap();
        std::fs::write(se_dir.join("clear.flac"), b"x").unwrap();

        let selection = SoundSetSelection {
            bgm_dir: None,
            bgm_variant_dir: None,
            se_dir: Some(se_dir.clone()),
            default_dir: None,
        };

        assert_eq!(selection.resolve(SoundType::Scratch), Some(se_dir.join("scratch.ogg")));
        assert_eq!(selection.resolve(SoundType::ResultClear), Some(se_dir.join("clear.flac")));
        assert_eq!(selection.resolve(SoundType::Select), None);

        std::fs::remove_dir_all(se_dir).unwrap();
    }

    #[test]
    fn select_random_returns_none_when_no_candidates() {
        let selection = select_random_sound_set(&[], &[], None);
        assert!(selection.bgm_dir.is_none());
        assert!(selection.bgm_variant_dir.is_none());
        assert!(selection.se_dir.is_none());
        assert!(selection.default_dir.is_none());
    }

    #[test]
    fn select_random_picks_a_candidate_when_present() {
        let bgm = vec![SoundSetRoot {
            dir: PathBuf::from("/bgm/set1"),
            variants: vec![PathBuf::from("/bgm/set1/one")],
        }];
        let se = vec![PathBuf::from("/se/set1"), PathBuf::from("/se/set2")];
        let default = PathBuf::from("/default");

        let selection = select_random_sound_set(&bgm, &se, Some(default.clone()));

        assert_eq!(selection.bgm_dir.as_deref(), Some(bgm[0].dir.as_path()));
        assert_eq!(selection.bgm_variant_dir.as_deref(), Some(bgm[0].variants[0].as_path()));
        assert!(se.iter().any(|p| Some(p.as_path()) == selection.se_dir.as_deref()));
        assert_eq!(selection.default_dir.as_deref(), Some(default.as_path()));
    }

    #[test]
    fn bgm_root_and_variant_are_selected_in_two_independent_draws() {
        let roots = vec![
            SoundSetRoot {
                dir: PathBuf::from("/bgm/a"),
                variants: vec![PathBuf::from("/bgm/a/only")],
            },
            SoundSetRoot {
                dir: PathBuf::from("/bgm/b"),
                variants: vec![
                    PathBuf::from("/bgm/b/one"),
                    PathBuf::from("/bgm/b/two"),
                    PathBuf::from("/bgm/b/three"),
                ],
            },
        ];
        let mut requested_bounds = Vec::new();
        let (root, variant) = select_bgm_set_with(&roots, |len| {
            requested_bounds.push(len);
            Some(if len == 2 { 1 } else { 2 })
        });

        assert_eq!(requested_bounds, [2, 3]);
        assert_eq!(root, Some(PathBuf::from("/bgm/b")));
        assert_eq!(variant, Some(PathBuf::from("/bgm/b/three")));
    }

    #[test]
    fn catalog_selects_from_cached_candidates() {
        let bgm = PathBuf::from("/bgm/set1");
        let se = PathBuf::from("/se/set1");
        let default = PathBuf::from("/default");
        let catalog = SoundSetCatalog {
            bgm_roots: vec![SoundSetRoot { dir: bgm.clone(), variants: vec![] }],
            se_dirs: vec![se.clone()],
            default_dir: Some(default.clone()),
        };

        let selection = catalog.select_random();

        assert_eq!(selection.bgm_dir.as_deref(), Some(bgm.as_path()));
        assert_eq!(selection.se_dir.as_deref(), Some(se.as_path()));
        assert_eq!(selection.default_dir.as_deref(), Some(default.as_path()));
    }

    #[test]
    fn soundset_overrides_se_and_missing_files_keep_legacy_fallback() {
        let root = temp_dir("se-overrides");
        let bgm = root.join("bgm");
        let se = root.join("se");
        let default = root.join("default");
        for dir in [&bgm, &se, &default] {
            std::fs::create_dir_all(dir).unwrap();
        }
        let selection = SoundSetSelection {
            bgm_dir: Some(bgm.clone()),
            bgm_variant_dir: None,
            se_dir: Some(se.clone()),
            default_dir: Some(default.clone()),
        };
        for sound in SoundType::ALL.into_iter().filter(|sound| !sound.is_bgm()) {
            let name = sound.file_name();
            for dir in [&bgm, &se, &default] {
                std::fs::write(dir.join(name), b"fixture").unwrap();
            }
            let result_bgm = match sound {
                SoundType::ResultClear => Some(SoundType::ResultBgmClear),
                SoundType::ResultFail => Some(SoundType::ResultBgmFail),
                SoundType::ResultA => Some(SoundType::ResultBgmA),
                SoundType::ResultAA => Some(SoundType::ResultBgmAA),
                SoundType::ResultAAA => Some(SoundType::ResultBgmAAA),
                _ => None,
            };
            if let Some(bgm_sound) = result_bgm {
                assert_eq!(selection.resolve(bgm_sound), Some(bgm.join(name)));
                assert_eq!(selection.resolve(sound), Some(se.join(name)));
            } else {
                assert_eq!(selection.resolve(sound), Some(bgm.join(name)));
            }
            std::fs::remove_file(bgm.join(name)).unwrap();
            if let Some(bgm_sound) = result_bgm {
                assert_eq!(selection.resolve(bgm_sound), None);
            }
            assert_eq!(selection.resolve(sound), Some(se.join(name)));
            std::fs::remove_file(se.join(name)).unwrap();
            assert_eq!(selection.resolve(sound), Some(default.join(name)));
        }
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn result_bgm_loop_variant_precedes_plain_files_and_stays_in_selected_set() {
        let root = temp_dir("result-bgm");
        let bgm = root.join("bgm");
        let se = root.join("se");
        let default = root.join("default");
        for dir in [&bgm, &se, &default] {
            std::fs::create_dir_all(dir).unwrap();
        }
        std::fs::write(bgm.join("CLEAR.WAV"), b"fixture").unwrap();
        std::fs::write(bgm.join("CLEAR.LOOP.OGG"), b"fixture").unwrap();
        std::fs::write(bgm.join("AAA.FLAC"), b"fixture").unwrap();
        std::fs::write(bgm.join("scratch.loop.wav"), b"fixture").unwrap();
        for dir in [&se, &default] {
            std::fs::write(dir.join("fail.wav"), b"fixture").unwrap();
        }
        let selection = SoundSetSelection {
            bgm_dir: Some(bgm.clone()),
            bgm_variant_dir: None,
            se_dir: Some(se),
            default_dir: Some(default),
        };
        assert_eq!(
            selection.candidates(SoundType::ResultBgmClear),
            vec![
                ResolvedSystemSound { path: bgm.join("CLEAR.LOOP.OGG"), loop_playback: true },
                ResolvedSystemSound { path: bgm.join("CLEAR.WAV"), loop_playback: false },
            ]
        );
        assert_eq!(
            selection.candidates(SoundType::ResultBgmAAA),
            vec![ResolvedSystemSound { path: bgm.join("AAA.FLAC"), loop_playback: false }]
        );
        assert!(selection.candidates(SoundType::ResultBgmFail).is_empty());
        assert!(selection.candidates(SoundType::Scratch).is_empty());
        std::fs::remove_dir_all(root).unwrap();
    }
}

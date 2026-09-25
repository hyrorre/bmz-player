use std::cell::OnceCell;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};

use ab_glyph::{Font, FontVec};
use font_kit::family_name::FamilyName;
use font_kit::handle::Handle;
use font_kit::properties::{Properties, Style, Weight};
use font_kit::source::{Source, SystemSource};
use font_kit::sources::fs::FsSource;
use font_kit::sources::multi::MultiSource;

/// font-kit が解決した OS フォントの実ファイル位置またはメモリ上のバイト列。
///
/// macOS Core Text はファイルパスではなくメモリ handle を返すことが多い。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedFont {
    pub path: Option<PathBuf>,
    memory: Option<Arc<[u8]>>,
    pub font_index: u32,
}

/// OS フォントへ要求する言語別のグリフ coverage。
///
/// 中国語の各値は Unicode の収録範囲だけでなく、候補ファミリの地域別字形を
/// 選ぶためにも使う。`HongKong` は繁体字のうち香港で一般的な字形・広東語の
/// 文字を優先する。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum FontCoverage {
    #[default]
    Japanese,
    Korean,
    SimplifiedChinese,
    TraditionalChinese,
    HongKong,
}

pub const ALL_FONT_COVERAGES: [FontCoverage; 5] = [
    FontCoverage::Japanese,
    FontCoverage::Korean,
    FontCoverage::SimplifiedChinese,
    FontCoverage::TraditionalChinese,
    FontCoverage::HongKong,
];

/// 日本語表示を優先する OS フォントファミリ名。
///
/// 同梱sourceで候補を一巡してから、OS sourceで同じ候補を探索する。
const JAPANESE_FONT_FAMILIES: &[&str] = &[
    "Hiragino Sans",
    "Hiragino Kaku Gothic ProN",
    "Hiragino Kaku Gothic Pro",
    "ヒラギノ角ゴシック",
    "Yu Gothic",
    "YuGothic",
    "Meiryo",
    "Noto Sans CJK JP",
    "Noto Sans JP",
    "MS Gothic",
    "IPAGothic",
    "IPAMincho",
    "Arial Unicode MS",
];

const KOREAN_FONT_FAMILIES: &[&str] = &[
    "Apple SD Gothic Neo",
    "Malgun Gothic",
    "맑은 고딕",
    "Noto Sans CJK KR",
    "Noto Sans KR",
    "NanumGothic",
    "UnDotum",
    "Arial Unicode MS",
];

const SIMPLIFIED_CHINESE_FONT_FAMILIES: &[&str] = &[
    "PingFang SC",
    "Microsoft YaHei UI",
    "Microsoft YaHei",
    "DengXian",
    "SimHei",
    "Noto Sans CJK SC",
    "Noto Sans SC",
    "WenQuanYi Zen Hei",
    "Arial Unicode MS",
];

const TRADITIONAL_CHINESE_FONT_FAMILIES: &[&str] = &[
    "PingFang TC",
    "Microsoft JhengHei UI",
    "Microsoft JhengHei",
    "Noto Sans CJK TC",
    "Noto Sans TC",
    "Heiti TC",
    "LiHei Pro",
    "Arial Unicode MS",
];

const HONG_KONG_FONT_FAMILIES: &[&str] = &[
    "PingFang HK",
    "Noto Sans CJK HK",
    "Noto Sans HK",
    "Microsoft JhengHei UI",
    "Microsoft JhengHei",
    "Arial Unicode MS",
];

impl FontCoverage {
    /// coverage の判定に使う代表グリフ。
    pub const fn glyph_probes(self) -> &'static [char] {
        match self {
            Self::Japanese => &['あ', '日'],
            Self::Korean => &['한', '글'],
            Self::SimplifiedChinese => &['汉', '语'],
            Self::TraditionalChinese => &['繁', '體'],
            Self::HongKong => &['嘅', '喺'],
        }
    }

    /// coverage の地域別字形を優先する OS フォントファミリ候補。
    pub const fn font_families(self) -> &'static [&'static str] {
        match self {
            Self::Japanese => JAPANESE_FONT_FAMILIES,
            Self::Korean => KOREAN_FONT_FAMILIES,
            Self::SimplifiedChinese => SIMPLIFIED_CHINESE_FONT_FAMILIES,
            Self::TraditionalChinese => TRADITIONAL_CHINESE_FONT_FAMILIES,
            Self::HongKong => HONG_KONG_FONT_FAMILIES,
        }
    }
}

/// OS フォント DB から最適なフォントファイルを解決する。
///
/// `require_japanese` が true のときは CJK グリフ検証を通過した face のみ返す。
/// false のときは `SansSerif` 系の一般フォントを返す（日本語非対応でも可）。
pub fn resolve_system_font(require_japanese: bool) -> Option<ResolvedFont> {
    if require_japanese {
        return resolve_system_font_for_coverage(FontCoverage::Japanese);
    }

    let source = SystemSource::new();
    let properties =
        Properties { weight: Weight::NORMAL, style: Style::Normal, stretch: Default::default() };
    resolve_family(&source, FamilyName::SansSerif, properties)
}

/// OS フォント DB から指定 coverage と地域別字形に適した face を解決する。
pub fn resolve_system_font_for_coverage(coverage: FontCoverage) -> Option<ResolvedFont> {
    let source = SystemSource::new();
    resolve_font_for_coverage_from_source(&source, coverage)
}

/// アプリ同梱フォントを OS フォントより先に見て、指定 coverage に適した face を解決する。
///
/// `font_roots` はアプリの resource directory 配下など、再配布するフォントを置く
/// ディレクトリを渡す。存在しないディレクトリは無視するため、開発時・旧配布物では
/// 既存の OS フォント解決へそのまま fallback する。
pub fn resolve_font_for_coverage(
    coverage: FontCoverage,
    font_roots: &[PathBuf],
) -> Option<ResolvedFont> {
    FontSources::new(font_roots).resolve(coverage)
}

fn resolve_font_for_coverage_from_source<S>(
    source: &S,
    coverage: FontCoverage,
) -> Option<ResolvedFont>
where
    S: Source + ?Sized,
{
    let properties =
        Properties { weight: Weight::NORMAL, style: Style::Normal, stretch: Default::default() };

    for family in coverage.font_families() {
        if let Some(resolved) =
            resolve_family(source, FamilyName::Title((*family).to_string()), properties)
            && resolved_font_supports_coverage(&resolved, coverage)
        {
            return Some(resolved);
        }
    }
    None
}

/// 優先 coverage を先頭にして、利用可能な CJK face を重複なしで返す。
///
/// 同じ face が複数 coverage を満たす場合は先に解決した coverage だけを残す。
pub fn resolve_system_font_fallbacks(preferred: FontCoverage) -> Vec<(FontCoverage, ResolvedFont)> {
    resolve_font_fallbacks(preferred, &[])
}

/// 同梱フォントを先頭にして、利用可能な全 CJK fallback face を返す。
///
/// 同じ face が複数 coverage を満たす場合は先に解決した coverage だけを残す。
pub fn resolve_font_fallbacks(
    preferred: FontCoverage,
    font_roots: &[PathBuf],
) -> Vec<(FontCoverage, ResolvedFont)> {
    // OS source 自体は共有しない（Linux等では Send/Sync ではない）。
    // renderer/egui間で、パス・face index・Arcのbytesだけを共有する。
    static CACHE: OnceLock<Mutex<FallbackCache>> = OnceLock::new();
    let roots: Vec<_> = font_roots
        .iter()
        .map(|root| {
            root.canonicalize().unwrap_or_else(|_| {
                std::env::current_dir().map(|cwd| cwd.join(root)).unwrap_or_else(|_| root.clone())
            })
        })
        .collect();
    let mut cache = CACHE.get_or_init(Mutex::default).lock().unwrap_or_else(|e| e.into_inner());
    let fonts = cache.resolve(&roots, || {
        let sources = FontSources::new(&roots);
        ALL_FONT_COVERAGES
            .into_iter()
            .filter_map(|coverage| sources.resolve(coverage).map(|font| (coverage, font)))
            .collect()
    });
    order_font_fallbacks(preferred, fonts)
}

/// 最後に使ったresource rootsの解決結果だけを保持する。
/// OSフォントの追加・変更は次回のプロセス起動で反映する。
/// preferredの変更は再探索せず並び替え、異なるrootsへの切替時は再探索する。
#[derive(Default)]
struct FallbackCache {
    entry: Option<(Vec<PathBuf>, Vec<(FontCoverage, ResolvedFont)>)>,
}

impl FallbackCache {
    fn resolve(
        &mut self,
        roots: &[PathBuf],
        load: impl FnOnce() -> Vec<(FontCoverage, ResolvedFont)>,
    ) -> &[(FontCoverage, ResolvedFont)] {
        if self.entry.as_ref().is_none_or(|(cached_roots, _)| cached_roots != roots) {
            self.entry = Some((roots.to_vec(), load()));
        }
        &self.entry.as_ref().unwrap().1
    }
}

fn order_font_fallbacks(
    preferred: FontCoverage,
    fonts: &[(FontCoverage, ResolvedFont)],
) -> Vec<(FontCoverage, ResolvedFont)> {
    std::iter::once(preferred)
        .chain(ALL_FONT_COVERAGES.into_iter().filter(|coverage| *coverage != preferred))
        .filter_map(|coverage| fonts.iter().find(|(candidate, _)| *candidate == coverage).cloned())
        .fold(Vec::new(), |mut fonts, candidate| {
            if !fonts.iter().any(|(_, font)| font == &candidate.1) {
                fonts.push(candidate);
            }
            fonts
        })
}

struct FontSources {
    bundled: MultiSource,
    system: OnceCell<SystemSource>,
}

impl FontSources {
    fn new(font_roots: &[PathBuf]) -> Self {
        let sources = font_roots
            .iter()
            .filter(|root| root.is_dir())
            .map(|root| Box::new(FsSource::in_path(root)) as Box<dyn Source>)
            .collect();
        Self { bundled: MultiSource::from_sources(sources), system: OnceCell::new() }
    }

    fn resolve(&self, coverage: FontCoverage) -> Option<ResolvedFont> {
        // MultiSourceにOSも含めると、同梱Notoより前のOS固有familyを探索してしまう。
        // 同梱sourceでcoverageを満たせない場合だけOS sourceを初期化・探索する。
        resolve_font_for_coverage_from_source(&self.bundled, coverage).or_else(|| {
            resolve_font_for_coverage_from_source(
                self.system.get_or_init(SystemSource::new),
                coverage,
            )
        })
    }
}

/// 解決済みフォントの生バイト列を読み込む。
pub fn read_resolved_font_bytes(resolved: &ResolvedFont) -> std::io::Result<Vec<u8>> {
    if let Some(memory) = &resolved.memory {
        return Ok(memory.to_vec());
    }
    if let Some(path) = &resolved.path {
        return std::fs::read(path);
    }
    Err(std::io::Error::new(
        std::io::ErrorKind::NotFound,
        "resolved font has neither path nor memory bytes",
    ))
}

/// ログや診断向けのソース説明文字列。
pub fn resolved_font_source(resolved: &ResolvedFont) -> String {
    match &resolved.path {
        Some(path) => path.display().to_string(),
        None => format!(
            "memory({} bytes, index {})",
            resolved.memory.as_ref().map(|bytes| bytes.len()).unwrap_or(0),
            resolved.font_index
        ),
    }
}

/// フォントバイト列がひらがな「あ」と漢字「日」を描画できるか判定する。
pub fn font_supports_japanese(bytes: &[u8], font_index: u32) -> bool {
    font_supports_coverage(bytes, font_index, FontCoverage::Japanese)
}

/// フォントバイト列が指定 coverage の代表グリフをすべて描画できるか判定する。
pub fn font_supports_coverage(bytes: &[u8], font_index: u32, coverage: FontCoverage) -> bool {
    FontVec::try_from_vec_and_index(bytes.to_vec(), font_index)
        .ok()
        .is_some_and(|font| coverage.glyph_probes().iter().all(|ch| font.glyph_id(*ch).0 != 0))
}

/// 解決済み face が指定 coverage の代表グリフをすべて描画できるか判定する。
pub fn resolved_font_supports_coverage(resolved: &ResolvedFont, coverage: FontCoverage) -> bool {
    let Ok(bytes) = read_resolved_font_bytes(resolved) else {
        return false;
    };
    font_supports_coverage(&bytes, resolved.font_index, coverage)
}

fn resolve_family<S>(source: &S, family: FamilyName, properties: Properties) -> Option<ResolvedFont>
where
    S: Source + ?Sized,
{
    let handle = source.select_best_match(&[family], &properties).ok()?;
    handle_to_resolved(&handle)
}

fn handle_to_resolved(handle: &Handle) -> Option<ResolvedFont> {
    match handle {
        Handle::Path { path, font_index } => {
            Some(ResolvedFont { path: Some(path.clone()), memory: None, font_index: *font_index })
        }
        Handle::Memory { bytes, font_index } => Some(ResolvedFont {
            path: None,
            memory: Some(Arc::from(bytes.as_slice())),
            font_index: *font_index,
        }),
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;

    use font_kit::sources::fs::FsSource;

    use super::*;

    #[test]
    fn cached_fallbacks_reorder_and_deduplicate_without_resolving_again() {
        let shared = ResolvedFont { path: Some("shared.ttc".into()), memory: None, font_index: 0 };
        let korean = ResolvedFont { path: Some("korean.ttf".into()), memory: None, font_index: 0 };
        let mut cache = FallbackCache::default();
        let fonts = cache.resolve(&[], || {
            vec![
                (FontCoverage::Japanese, shared.clone()),
                (FontCoverage::Korean, korean.clone()),
                (FontCoverage::SimplifiedChinese, shared.clone()),
            ]
        });
        assert_eq!(
            order_font_fallbacks(FontCoverage::Japanese, fonts),
            vec![(FontCoverage::Japanese, shared.clone()), (FontCoverage::Korean, korean.clone())]
        );
        let fonts = cache.resolve(&[], || panic!("same roots must reuse the resolved fonts"));
        assert_eq!(
            order_font_fallbacks(FontCoverage::SimplifiedChinese, fonts),
            vec![(FontCoverage::SimplifiedChinese, shared), (FontCoverage::Korean, korean)]
        );
    }

    #[test]
    fn switching_font_roots_replaces_cached_faces() {
        let mut cache = FallbackCache::default();
        let memory: Arc<[u8]> = Arc::from([1, 2, 3].as_slice());
        let roots = vec![PathBuf::from("first")];
        let fonts = cache.resolve(&roots, || {
            vec![(
                FontCoverage::Japanese,
                ResolvedFont { path: None, memory: Some(memory.clone()), font_index: 2 },
            )]
        });
        let copied = order_font_fallbacks(FontCoverage::Japanese, fonts);
        assert!(Arc::ptr_eq(copied[0].1.memory.as_ref().unwrap(), &memory));
        assert_eq!(copied[0].1.font_index, 2);
        drop(copied);
        assert!(cache.resolve(&[PathBuf::from("second")], Vec::new).is_empty());
        assert_eq!(Arc::strong_count(&memory), 1, "old cached memory must be released");
        let mut reloaded = false;
        cache.resolve(&roots, || {
            reloaded = true;
            Vec::new()
        });
        assert!(reloaded);
    }

    #[test]
    fn font_supports_japanese_rejects_empty_bytes() {
        assert!(!font_supports_japanese(&[], 0));
    }

    #[test]
    fn all_coverages_have_family_candidates_and_distinct_probes() {
        for coverage in ALL_FONT_COVERAGES {
            assert!(!coverage.font_families().is_empty());
            assert!(coverage.glyph_probes().len() >= 2);
            assert!(
                coverage.glyph_probes().iter().all(|probe| !probe.is_ascii() && *probe != '\0')
            );
        }
        assert_ne!(
            FontCoverage::TraditionalChinese.glyph_probes(),
            FontCoverage::HongKong.glyph_probes()
        );
    }

    #[test]
    fn all_coverages_reject_empty_bytes() {
        for coverage in ALL_FONT_COVERAGES {
            assert!(!font_supports_coverage(&[], 0, coverage));
        }
    }

    #[test]
    fn resolve_from_filesystem_source_finds_font_by_path() {
        let Some(resolved) = resolve_system_font(false) else {
            return;
        };
        assert!(
            resolved.path.as_ref().is_some_and(|path| path.is_file())
                || read_resolved_font_bytes(&resolved).is_ok()
        );
    }

    #[test]
    fn japanese_resolution_returns_cjk_capable_font_when_available() {
        let Some(resolved) = resolve_system_font(true) else {
            return;
        };
        assert!(resolved_font_supports_coverage(&resolved, FontCoverage::Japanese));
    }

    #[test]
    fn coverage_resolution_returns_a_matching_face_when_available() {
        for coverage in ALL_FONT_COVERAGES {
            let Some(resolved) = resolve_system_font_for_coverage(coverage) else {
                continue;
            };
            assert!(resolved_font_supports_coverage(&resolved, coverage));
        }
    }

    #[test]
    fn fallback_resolution_prioritizes_requested_coverage() {
        for preferred in ALL_FONT_COVERAGES {
            let fonts = resolve_system_font_fallbacks(preferred);
            let Some((coverage, _)) = fonts.first() else {
                continue;
            };
            if resolve_system_font_for_coverage(preferred).is_some() {
                assert_eq!(*coverage, preferred);
            }
            for (index, (_, font)) in fonts.iter().enumerate() {
                assert!(!fonts[..index].iter().any(|(_, previous)| previous == font));
            }
        }
    }

    #[test]
    fn bundled_noto_cjk_resolves_every_supported_coverage() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data/fonts/noto-cjk");
        assert!(root.is_dir(), "bundled font directory should exist: {}", root.display());

        let roots = vec![root.clone()];
        let sources = FontSources::new(&roots);
        let mut faces = Vec::new();
        for coverage in ALL_FONT_COVERAGES {
            let resolved = sources
                .resolve(coverage)
                .unwrap_or_else(|| panic!("bundled font should resolve {coverage:?}"));
            assert!(resolved.path.as_ref().is_some_and(|path| path.starts_with(&root)));
            assert!(!faces.contains(&resolved), "regional CJK faces must remain distinct");
            assert!(
                resolved_font_supports_coverage(&resolved, coverage),
                "bundled font should support {coverage:?}"
            );
            faces.push(resolved);
        }
        assert!(sources.system.get().is_none(), "bundled CJK must not query the OS source");
    }

    #[test]
    fn missing_bundled_fonts_fall_back_to_system_source() {
        let sources = FontSources::new(&[]);
        let resolved = sources.resolve(FontCoverage::Japanese);
        assert!(sources.system.get().is_some());
        assert_eq!(resolved, resolve_system_font_for_coverage(FontCoverage::Japanese));
    }

    /// `FsSource` 経由の fixture で path/index 抽出を検証する。
    #[test]
    fn filesystem_source_resolves_fixture_font() {
        let Some(resolved) = resolve_system_font(false) else {
            return;
        };
        let Ok(bytes) = read_resolved_font_bytes(&resolved) else {
            return;
        };

        let temp_dir = std::env::temp_dir().join(format!("bmz-font-test-{}", std::process::id()));
        let _ = fs::create_dir_all(&temp_dir);
        let fixture_path = temp_dir.join("fixture.ttf");
        if fs::write(&fixture_path, &bytes).is_err() {
            let _ = fs::remove_dir_all(&temp_dir);
            return;
        }

        let source = FsSource::in_path(&temp_dir);
        let handle = source.all_fonts().ok().and_then(|handles| {
            handles
                .into_iter()
                .find(|handle| matches!(handle, Handle::Path { path, .. } if path == &fixture_path))
        });

        if let Some(handle) = handle {
            let fixture = handle_to_resolved(&handle).expect("fixture handle should resolve");
            assert_eq!(fixture.path.as_deref(), Some(fixture_path.as_path()));
            let fixture_bytes = read_resolved_font_bytes(&fixture).expect("fixture bytes");
            assert_eq!(fixture_bytes.len(), bytes.len());
        }

        let _ = fs::remove_file(&fixture_path);
        let _ = fs::remove_dir_all(&temp_dir);
    }
}

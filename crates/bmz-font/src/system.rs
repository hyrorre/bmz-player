use std::path::PathBuf;
use std::sync::Arc;

use ab_glyph::{Font, FontVec};
use font_kit::family_name::FamilyName;
use font_kit::handle::Handle;
use font_kit::properties::{Properties, Style, Weight};
use font_kit::source::SystemSource;

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
/// 将来 `data/fonts/` 同梱フォントを `FsSource` + `Multi` で先に見る場合は、
/// この列の前段に bundled source を差し込む。
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
    let properties =
        Properties { weight: Weight::NORMAL, style: Style::Normal, stretch: Default::default() };

    for family in coverage.font_families() {
        if let Some(resolved) =
            resolve_family(&source, FamilyName::Title((*family).to_string()), properties)
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
    std::iter::once(preferred)
        .chain(ALL_FONT_COVERAGES.into_iter().filter(|coverage| *coverage != preferred))
        .filter_map(|coverage| {
            resolve_system_font_for_coverage(coverage).map(|font| (coverage, font))
        })
        .fold(Vec::new(), |mut fonts, candidate| {
            if !fonts.iter().any(|(_, font)| font == &candidate.1) {
                fonts.push(candidate);
            }
            fonts
        })
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

fn resolve_family(
    source: &SystemSource,
    family: FamilyName,
    properties: Properties,
) -> Option<ResolvedFont> {
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

    use font_kit::sources::fs::FsSource;

    use super::*;

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

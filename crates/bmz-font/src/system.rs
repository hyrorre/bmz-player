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

/// OS フォント DB から最適なフォントファイルを解決する。
///
/// `require_japanese` が true のときは CJK グリフ検証を通過した face のみ返す。
/// false のときは `SansSerif` 系の一般フォントを返す（日本語非対応でも可）。
pub fn resolve_system_font(require_japanese: bool) -> Option<ResolvedFont> {
    let source = SystemSource::new();
    let properties =
        Properties { weight: Weight::NORMAL, style: Style::Normal, stretch: Default::default() };

    if require_japanese {
        for family in JAPANESE_FONT_FAMILIES {
            if let Some(resolved) =
                resolve_family(&source, FamilyName::Title(family.to_string()), properties)
                && font_supports_resolved(&resolved)
            {
                return Some(resolved);
            }
        }
        return None;
    }

    resolve_family(&source, FamilyName::SansSerif, properties)
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
    FontVec::try_from_vec_and_index(bytes.to_vec(), font_index)
        .ok()
        .is_some_and(|font| font.glyph_id('あ').0 != 0 && font.glyph_id('日').0 != 0)
}

fn font_supports_resolved(resolved: &ResolvedFont) -> bool {
    let Ok(bytes) = read_resolved_font_bytes(resolved) else {
        return false;
    };
    font_supports_japanese(&bytes, resolved.font_index)
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
        assert!(font_supports_resolved(&resolved));
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

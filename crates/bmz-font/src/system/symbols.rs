//! 記号・絵文字の欠字補完に使う OS フォント。
//!
//! CJK fallback と異なり、スキン描画で文字が見つからなかったときだけ参照する。
//! 大きな絵文字フォント（macOS の Apple Color Emoji は100MB超）を丸ごと読み込まないよう、
//! ファイルは読み取り専用で memory map し、プロセス内で一度だけ解決する。

use std::fs::File;

use ttf_parser::{Face, name_id};

use super::*;

/// 記号を先、カラー絵文字フォントを後に並べた OS フォント候補。
///
/// どれも存在を前提にせず、解決できた face だけを使う。ファミリ名が一致しても
/// 対象文字を持つとは限らないため、描画側で文字ごとにグリフの有無を確認する。
pub const SYMBOL_FONT_FAMILIES: &[&str] = &[
    // Windows
    "Segoe UI Symbol",
    // macOS
    "Apple Symbols",
    "Zapf Dingbats",
    // Linux 等
    "Noto Sans Symbols 2",
    "Noto Sans Symbols",
    "Noto Sans Math",
    "DejaVu Sans",
    "FreeSerif",
    "Symbola",
    "Arial Unicode MS",
    // カラー絵文字
    "Segoe UI Emoji",
    "Apple Color Emoji",
    "Noto Color Emoji",
    "Twemoji Mozilla",
    "Noto Emoji",
];

/// 解決済みの記号・絵文字フォント。
pub struct SymbolFont {
    family: String,
    path: Option<PathBuf>,
    font_index: u32,
    data: SymbolFontData,
}

enum SymbolFontData {
    Mapped(memmap2::Mmap),
    Memory(Arc<[u8]>),
}

impl SymbolFont {
    /// 要求したファミリ名。
    pub fn family(&self) -> &str {
        &self.family
    }

    /// フォントファイル全体のバイト列。TTC では `font_index` で face を選ぶ。
    pub fn data(&self) -> &[u8] {
        match &self.data {
            SymbolFontData::Mapped(map) => map,
            SymbolFontData::Memory(bytes) => bytes,
        }
    }

    pub fn font_index(&self) -> u32 {
        self.font_index
    }

    /// ログ向けのソース説明。
    pub fn source(&self) -> String {
        match &self.path {
            Some(path) => path.display().to_string(),
            None => format!("memory({} bytes, index {})", self.data().len(), self.font_index),
        }
    }
}

impl std::fmt::Debug for SymbolFont {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SymbolFont")
            .field("family", &self.family)
            .field("source", &self.source())
            .finish()
    }
}

/// OS の記号・絵文字フォントをプロセス内で一度だけ解決して返す。
///
/// OS フォントの追加・変更は CJK fallback と同じく次回起動時に反映する。
pub fn symbol_font_fallbacks() -> &'static [SymbolFont] {
    static FONTS: OnceLock<Vec<SymbolFont>> = OnceLock::new();
    FONTS.get_or_init(|| resolve_symbol_fonts(&SystemSource::new(), SYMBOL_FONT_FAMILIES))
}

pub(super) fn resolve_symbol_fonts<S>(source: &S, families: &[&str]) -> Vec<SymbolFont>
where
    S: Source + ?Sized,
{
    let properties =
        Properties { weight: Weight::NORMAL, style: Style::Normal, stretch: Default::default() };
    let mut fonts: Vec<SymbolFont> = Vec::new();
    for family in families {
        let Some(resolved) =
            resolve_family(source, FamilyName::Title((*family).to_string()), properties)
        else {
            continue;
        };
        let duplicate = fonts.iter().any(|font| {
            font.font_index == resolved.font_index
                && match (&font.path, &resolved.path) {
                    (Some(a), Some(b)) => a == b,
                    _ => false,
                }
        });
        if duplicate {
            continue;
        }
        let Some(data) = load_symbol_font_data(&resolved) else { continue };
        let font = SymbolFont {
            family: (*family).to_string(),
            path: resolved.path,
            font_index: resolved.font_index,
            data,
        };
        // fontconfig 等は存在しないファミリに代替フォントを返すため、実際の名前を確かめる。
        if face_has_family(font.data(), font.font_index, family) {
            fonts.push(font);
        }
    }
    fonts
}

fn load_symbol_font_data(resolved: &ResolvedFont) -> Option<SymbolFontData> {
    if let Some(memory) = &resolved.memory {
        return Some(SymbolFontData::Memory(memory.clone()));
    }
    let file = File::open(resolved.path.as_ref()?).ok()?;
    // SAFETY: OS がインストールしたフォントを読み取り専用で map する。書き込みは行わず、
    // map は static に保持してプロセス終了まで解放しない。実行中のフォント差し替えは
    // OS 自身のフォント描画と同じく想定しない。
    let map = unsafe { memmap2::Mmap::map(&file) }.ok()?;
    Some(SymbolFontData::Mapped(map))
}

/// face の family / typographic family / full name のいずれかが一致するか。
pub(super) fn face_has_family(data: &[u8], font_index: u32, family: &str) -> bool {
    let Ok(face) = Face::parse(data, font_index) else {
        return false;
    };
    face.names()
        .into_iter()
        .filter(|name| {
            matches!(
                name.name_id,
                name_id::FAMILY | name_id::TYPOGRAPHIC_FAMILY | name_id::FULL_NAME
            )
        })
        .filter_map(|name| name.to_string())
        .any(|name| name.eq_ignore_ascii_case(family))
}

//! OS フォントのパス解決を font-kit 経由で共通化する薄い crate。
//!
//! `bmz-render`（ゲーム描画）と `bmz-player`（egui UI）が同じ解決ロジックを
//! 共有する。描画自体は ab_glyph が担当し、本 crate はパス/index 解決と
//! 言語別 CJK グリフ検証だけを行う。

mod system;

pub use system::{
    ALL_FONT_COVERAGES, FontCoverage, ResolvedFont, font_supports_coverage, font_supports_japanese,
    read_resolved_font_bytes, resolve_system_font, resolve_system_font_fallbacks,
    resolve_system_font_for_coverage, resolved_font_source, resolved_font_supports_coverage,
};

//! OS フォントのパス解決を font-kit 経由で共通化する薄い crate。
//!
//! `bmz-render`（ゲーム描画）と `bmz-app`（egui UI）が同じ解決ロジックを
//! 共有する。描画自体は ab_glyph が担当し、本 crate はパス/index 解決と
//! 日本語グリフ検証だけを行う。

mod system;

pub use system::{
    ResolvedFont, font_supports_japanese, read_resolved_font_bytes, resolve_system_font,
    resolved_font_source,
};

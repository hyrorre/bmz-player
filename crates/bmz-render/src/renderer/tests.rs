use crate::scene::{AppSceneSnapshot, SelectSnapshot};

use super::*;

#[test]
fn skipped_surface_does_not_arm_a_window_frame_callback() {
    let mut renderer = Renderer::default();
    let mut notified = false;
    assert_eq!(
        renderer.render_last_plan_with_pre_present_notify(|| notified = true).unwrap(),
        RenderSurfaceStatus::SkippedNoSurface
    );
    assert!(!notified);
}

fn test_surface_size() -> SurfaceSize {
    SurfaceSize { width: 16, height: 9 }
}

fn test_bitmap_font() -> BitmapFont {
    let mut pages = HashMap::new();
    pages.insert(
        0,
        crate::bitmap_font::BitmapFontPage {
            id: 0,
            path: std::path::PathBuf::from("page.png"),
            image: crate::assets::RgbaImageAsset {
                width: 1,
                height: 1,
                pixels: vec![255, 255, 255, 255],
            },
        },
    );
    let mut glyphs = HashMap::new();
    glyphs.insert(
        'A',
        crate::bitmap_font::BitmapFontGlyph {
            id: 'A',
            x: 0,
            y: 0,
            width: 1,
            height: 1,
            xoffset: 0,
            yoffset: 0,
            xadvance: 1,
            page: 0,
        },
    );
    BitmapFont {
        size: 10,
        line_height: 10,
        base: 8,
        ascent: 7.0,
        scale_width: 1,
        scale_height: 1,
        pages: pages.into(),
        glyphs: glyphs.into(),
        fallback_style: None,
    }
}

fn assert_approx(actual: f32, expected: f32) {
    assert!((actual - expected).abs() < 0.0001, "expected {expected}, got {actual}");
}

fn font_supports_japanese(font: &TextFace) -> bool {
    font.has_glyph('あ') && font.has_glyph('日')
}

fn faces_only(faces: &[TextFace]) -> FaceList<'_> {
    FaceList { preferred: faces, system: &[] }
}

fn single_font_set<'a>(id: &'a str, face: &'a TextFace) -> VectorFontSet<'a> {
    VectorFontSet { id, faces: faces_only(std::slice::from_ref(face)) }
}

fn bitmap_entries(fonts: &[&BitmapFont]) -> Vec<BitmapFontEntry> {
    fonts
        .iter()
        .enumerate()
        .map(|(index, font)| BitmapFontEntry {
            id: Arc::from(if index == 0 { "bitmap".to_string() } else { format!("bitmap{index}") }),
            font: (*font).clone(),
        })
        .collect()
}

/// フォールバック無しのビットマップフォントセット。
fn bitmap_set(entries: &[BitmapFontEntry]) -> BitmapFontSet<'_> {
    BitmapFontSet { id: "bitmap", fonts: entries, faces: faces_only(&[]) }
}

/// 単一フォントで解決した文字列の px 幅。
fn shaped_width_px(text: &str, face: &TextFace, px: f32) -> f32 {
    let shaped = shape_vector_text(text, single_font_set("width", face));
    clusters_width_px(&shaped.clusters, px)
}

fn sample_image(texture: u32, blend: BlendMode) -> DrawCommand {
    DrawCommand::Image {
        rect: crate::plan::Rect { x: 0.1, y: 0.2, width: 0.3, height: 0.4 },
        uv: crate::plan::UvRect { x: 0.0, y: 0.0, width: 1.0, height: 1.0 },
        source_size: None,
        texture: crate::plan::TextureId(texture),
        tint: Color::rgb(1.0, 1.0, 1.0),
        blend,
        linear_filter: false,
    }
}

fn sample_rect() -> DrawCommand {
    DrawCommand::Rect {
        rect: crate::plan::Rect { x: 0.0, y: 0.0, width: 0.1, height: 0.1 },
        color: Color::rgb(1.0, 1.0, 1.0),
    }
}

fn sample_text() -> DrawCommand {
    DrawCommand::Text {
        origin: Point { x: 0.1, y: 0.1 },
        text: "x".to_string(),
        caret: None,
        post_scale: Point { x: 1.0, y: 1.0 },
        style: TextStyle {
            gradient: None,
            font_id: None,
            size: 0.1,
            bitmap_size: None,
            color: Color::rgb(1.0, 1.0, 1.0),
            layer: crate::plan::TextLayer::Skin,
            align: TextAlign::Left,
            max_width: 0.0,
            overflow: TextOverflow::Overflow,
            wrapping: false,
            outline: None,
            shadow: None,
        },
    }
}

#[path = "tests/ambient.rs"]
mod ambient;
#[path = "tests/bitmap_fallback_style.rs"]
mod bitmap_fallback_style;
#[path = "tests/cases_01.rs"]
mod cases_01;
#[path = "tests/cases_02.rs"]
mod cases_02;
#[path = "tests/cases_03.rs"]
mod cases_03;
#[path = "tests/cases_04.rs"]
mod cases_04;
#[path = "tests/cases_05.rs"]
mod cases_05;
#[path = "tests/clip.rs"]
mod clip;
#[path = "tests/shrink_raster.rs"]
mod shrink_raster;
#[path = "tests/text_atlas.rs"]
mod text_atlas;
#[path = "tests/text_atlas_upload.rs"]
mod text_atlas_upload;
#[path = "tests/text_gradient.rs"]
mod text_gradient;
#[path = "tests/text_layout_cache.rs"]
mod text_layout_cache;
#[path = "tests/text_non_finite.rs"]
mod text_non_finite;
#[path = "tests/unicode_fallback.rs"]
mod unicode_fallback;
#[path = "tests/unicode_regression.rs"]
mod unicode_regression;
#[path = "tests/unicode_sequences.rs"]
mod unicode_sequences;
#[path = "tests/unicode_text.rs"]
mod unicode_text;

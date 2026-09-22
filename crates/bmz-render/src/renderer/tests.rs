use crate::scene::{AppSceneSnapshot, SelectSnapshot};

use super::*;

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
    }
}

fn assert_approx(actual: f32, expected: f32) {
    assert!((actual - expected).abs() < 0.0001, "expected {expected}, got {actual}");
}

fn font_supports_japanese<F: Font>(font: &F) -> bool {
    font.glyph_id('あ').0 != 0 && font.glyph_id('日').0 != 0
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

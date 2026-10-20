//! 文字 atlas の幅・高さ・GPU テクスチャ上限の回帰テスト。

use super::unicode_text::*;
use super::*;

/// quad の atlas 領域が atlas 内に収まり、テクスチャ座標が 0..=1 になること。
fn assert_quads_inside_atlas(quads: &[TextQuad], atlas: &TextAtlasCache) {
    let size = atlas.size();
    for quad in quads {
        assert!(quad.atlas_origin.0 + quad.glyph_width <= size.width, "{quad:?} / {size:?}");
        assert!(quad.atlas_origin.1 + quad.glyph_height <= size.height, "{quad:?} / {size:?}");
    }
    let instances = encode_text_quads(quads, size.width, size.height);
    for instance in instances.chunks_exact(TEXT_INSTANCE_BYTES) {
        let f: Vec<f32> = instance
            .chunks_exact(4)
            .map(|bytes| f32::from_le_bytes(bytes.try_into().unwrap()))
            .collect();
        assert!(f[4] >= 0.0 && f[4] + f[6] <= 1.0 + f32::EPSILON, "u {:?}", &f[4..8]);
        assert!(f[5] >= 0.0 && f[5] + f[7] <= 1.0 + f32::EPSILON, "v {:?}", &f[4..8]);
    }
}

fn atlas_pixel(atlas: &TextAtlasCache, x: u32, y: u32) -> [u8; 4] {
    let size = atlas.size();
    let pixels = atlas.pixels_for_size(size);
    let index = ((y * size.width + x) * 4) as usize;
    pixels[index..index + 4].try_into().unwrap()
}

#[test]
fn glyph_wider_than_the_atlas_is_downscaled_but_keeps_its_display_size() {
    let face = color_font();
    let mut atlas = TextAtlasCache::new(TEXT_ATLAS_WIDTH);
    atlas.begin_frame();
    // 1500px 相当のカラー絵文字（ラスタライズは2倍の3000px）。
    let scale = PxScale::from(1500.0 * face.metrics().height() / face.metrics().units_per_em);
    let glyph = face.glyph_id('😀');
    let cached = atlas.cached_color_glyph(&face, glyph, scale, false).expect("stored");
    let max = TEXT_ATLAS_WIDTH - TEXT_ATLAS_PADDING * 2;
    assert!(cached.width <= max && cached.height <= max, "{cached:?}");
    // 表示寸法は通常サイズの同じグリフと比例する。
    let small_scale = PxScale::from(scale.x / 50.0);
    let small = atlas.cached_color_glyph(&face, glyph, small_scale, false).unwrap();
    assert!((cached.display_width / small.display_width - 50.0).abs() < 1.5, "{cached:?}");
    assert!((cached.display_height / small.display_height - 50.0).abs() < 1.5, "{cached:?}");
    assert!((cached.offset_y / small.offset_y - 50.0).abs() < 1.5, "{cached:?}");
    // 縮小しても絵柄は残る（中央は顔の黄色）。
    let center = atlas_pixel(
        &atlas,
        cached.atlas_origin.0 + cached.width / 2,
        cached.atlas_origin.1 + cached.height * 2 / 3,
    );
    assert_eq!(center[3], 255);
    assert!(center[0] > 200 && center[2] < 50, "{center:?}");
}

#[test]
fn oversized_outline_bitmap_and_placeholder_glyphs_fit_the_atlas() {
    let mut atlas = TextAtlasCache::with_limits(64, 256);
    atlas.begin_frame();
    let max = 64 - TEXT_ATLAS_PADDING * 2;
    let face = latin();
    let outline = atlas.cached_vector_glyph(&face, face.glyph_id('W'), PxScale::from(400.0));
    let outline = outline.expect("outline glyph stored");
    assert!(outline.width <= max && outline.height <= max, "{outline:?}");
    // W は 0.72em 幅なので、400px では約290px で表示する。
    assert!((outline.display_width - 288.0).abs() < 8.0, "{outline:?}");
    let tofu = atlas.cached_tofu_glyph(300.0, 400.0).expect("placeholder stored");
    assert!(tofu.width <= max && tofu.height <= max, "{tofu:?}");
    assert_eq!((tofu.display_width, tofu.display_height), (300.0, 400.0));

    let font = test_bitmap_font();
    let (ch, glyph) = font.glyphs.iter().find(|(_, glyph)| glyph.width > 0).unwrap();
    let page = &font.pages[&glyph.page];
    let bitmap = atlas
        .cached_bitmap_glyph(Arc::from("fnt"), *ch, *glyph, page, &font, PxScale::from(100.0))
        .expect("bitmap glyph stored");
    assert!(bitmap.width <= max && bitmap.height <= max, "{bitmap:?}");
    assert_eq!(bitmap.display_width, (glyph.width as f32 * 100.0).ceil());
    let quads: Vec<_> = [outline, tofu, bitmap]
        .into_iter()
        .map(|cached| TextQuad {
            x: 0.0,
            y: 0.0,
            width: 1.0,
            height: 1.0,
            atlas_origin: cached.atlas_origin,
            glyph_width: cached.width,
            glyph_height: cached.height,
            color: Color::rgb(1.0, 1.0, 1.0),
        })
        .collect();
    assert_quads_inside_atlas(&quads, &atlas);
}

#[test]
fn reserve_accepts_exact_fit_and_rejects_one_pixel_over() {
    let mut atlas = TextAtlasCache::with_limits(64, 64);
    let fit = 64 - TEXT_ATLAS_PADDING * 2;
    assert_eq!(atlas.reserve(fit, fit), Some((TEXT_ATLAS_PADDING, TEXT_ATLAS_PADDING)));
    assert_eq!(atlas.size(), AtlasSize { width: 64, height: 64 });

    let mut atlas = TextAtlasCache::with_limits(64, 64);
    assert_eq!(atlas.reserve(fit + 1, 4), None, "wider than the atlas with padding");
    assert_eq!(atlas.reserve(4, fit + 1), None, "taller than the atlas with padding");
    assert_eq!((atlas.pen_x, atlas.pen_y), (0, 0), "a rejected glyph must not move the pen");
    assert_eq!(atlas.reserve(4, 4), Some((TEXT_ATLAS_PADDING, TEXT_ATLAS_PADDING)));
}

#[test]
fn small_atlas_wraps_rows_inside_its_width() {
    let mut atlas = TextAtlasCache::with_limits(32, 128);
    let mut origins = Vec::new();
    for _ in 0..6 {
        origins.push(atlas.reserve(10, 10).unwrap());
    }
    // パディング込み12pxなので1行に2つ（24px）、3つ目は次の行。
    assert_eq!(origins[..3], [(1, 1), (13, 1), (1, 13)]);
    for (x, y) in origins {
        assert!(x + 10 + TEXT_ATLAS_PADDING <= 32 && y + 10 + TEXT_ATLAS_PADDING <= 128);
    }
}

#[test]
fn full_atlas_is_rebuilt_at_the_next_frame_with_valid_positions() {
    let face = latin();
    let mut atlas = TextAtlasCache::with_limits(64, 64);
    atlas.begin_frame();
    let mut stored = Vec::new();
    let mut size = 30.0;
    // atlas が埋まるまでグリフを追加する。溢れたグリフは格納せずに None を返す。
    let overflowed = loop {
        match atlas.cached_vector_glyph(&face, face.glyph_id('A'), PxScale::from(size)) {
            Some(cached) => stored.push(cached),
            None => break PxScale::from(size),
        }
        size += 1.0;
        assert!(stored.len() < 64, "a 64x64 atlas cannot hold this many glyphs");
    };
    assert!(atlas.atlas_height() <= 64);
    let key_count = atlas.glyphs.len();
    // 同じフレームで再要求してもラスタライズし直さない。
    assert!(atlas.cached_vector_glyph(&face, face.glyph_id('A'), overflowed).is_none());
    assert_eq!(atlas.glyphs.len(), key_count);

    // 次のフレームで作り直し、溢れたグリフも atlas 内の位置に入る。
    atlas.begin_frame();
    assert!(atlas.glyphs.is_empty());
    assert_eq!(atlas.layouts.len(), 0);
    let cached = atlas.cached_vector_glyph(&face, face.glyph_id('A'), overflowed).unwrap();
    assert_eq!(cached.atlas_origin, (TEXT_ATLAS_PADDING, TEXT_ATLAS_PADDING));
    assert_eq!(atlas.dirty_regions.len(), 1);
}

#[test]
fn frame_larger_than_the_atlas_does_not_rebuild_every_frame() {
    let face = latin();
    let mut atlas = TextAtlasCache::with_limits(64, 64);
    let sizes: Vec<f32> = (0..40).map(|index| 30.0 + index as f32).collect();
    let draw = |atlas: &mut TextAtlasCache| {
        atlas.begin_frame();
        for size in &sizes {
            let _ = atlas.cached_vector_glyph(&face, face.glyph_id('A'), PxScale::from(*size));
        }
        atlas.dirty_regions.len()
    };
    // 1フレームでは収まらない量。初回と、作り直した直後の1回だけラスタライズする。
    assert!(draw(&mut atlas) > 0);
    assert!(draw(&mut atlas) > 0);
    for _ in 0..10 {
        assert_eq!(draw(&mut atlas), 0, "the overflowing frame must not re-rasterize");
    }
}

#[test]
fn atlas_limits_follow_the_gpu_texture_limit() {
    for (limit, width, height) in [
        (4096, TEXT_ATLAS_WIDTH, 4096),
        (8192, TEXT_ATLAS_WIDTH, TEXT_ATLAS_MAX_HEIGHT),
        (16384, TEXT_ATLAS_WIDTH, TEXT_ATLAS_MAX_HEIGHT),
        (512, 512, 512),
    ] {
        let mut atlas = TextAtlasCache::for_texture_limit(limit);
        assert_eq!((atlas.width(), atlas.max_height()), (width, height), "limit {limit}");
        // 上限まで積んでも GPU テクスチャの上限を超えない。
        while atlas.reserve(width / 4, 60).is_some() {}
        assert!(atlas.atlas_height() <= height.min(limit), "limit {limit}");
        assert!(atlas.size().width <= limit && atlas.size().height <= limit);
    }
}

#[test]
fn high_dpi_color_emoji_keeps_layout_proportions() {
    let faces = [latin(), color_font()];
    let layout = |surface: SurfaceSize| {
        let mut atlas = TextAtlasCache::new(TEXT_ATLAS_WIDTH);
        atlas.begin_frame();
        let mut builder = CachedTextFrameBuilder::new(&mut atlas);
        let mut style = text_style(None);
        style.size = 0.5;
        style.color = Color::rgb(1.0, 0.0, 0.0);
        builder.push_text(
            &Point { x: 0.1, y: 0.1 },
            "A😀",
            style,
            VectorFontSet { id: "chain", faces: faces_only(&faces) },
            surface,
        );
        let quads = builder.quads.clone();
        assert_quads_inside_atlas(&quads, &atlas);
        assert_eq!(quads.len(), 2, "{surface:?}");
        // 絵文字は文字色ではなくカラーグリフ（白×透明度）で描く。
        assert_eq!(quads[1].color, Color::rgba(1.0, 1.0, 1.0, 1.0), "{surface:?}");
        quads.iter().map(|quad| (quad.x, quad.y, quad.width, quad.height)).collect::<Vec<_>>()
    };
    // 4K 相当の高さで em が1080px（ラスタライズは2160px）になり、atlas 幅を超える。
    let normal = layout(SurfaceSize { width: 1280, height: 720 });
    let high = layout(SurfaceSize { width: 3840, height: 2160 });
    for (a, b) in normal.iter().zip(&high) {
        assert!((a.0 - b.0).abs() < 0.003 && (a.1 - b.1).abs() < 0.003, "{a:?} {b:?}");
        assert!((a.2 - b.2).abs() < 0.003 && (a.3 - b.3).abs() < 0.003, "{a:?} {b:?}");
    }
}

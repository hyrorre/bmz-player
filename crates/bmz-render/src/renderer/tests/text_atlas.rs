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
    for instance in instances.as_chunks::<TEXT_INSTANCE_BYTES>().0 {
        let f: Vec<f32> =
            instance.as_chunks::<4>().0.iter().map(|bytes| f32::from_le_bytes(*bytes)).collect();
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
    // 格納できたグリフは作り直しを待つ間も描ける。
    let stored = atlas.glyphs.len();
    assert!(stored > 0);
    atlas.begin_frame();
    let drawable = sizes
        .iter()
        .filter_map(|size| {
            atlas.cached_vector_glyph(&face, face.glyph_id('A'), PxScale::from(*size))
        })
        .count();
    assert_eq!(drawable, stored);
    assert!(atlas.dirty_regions.is_empty());
    // 待つのは一定のフレーム数だけで、溢れが続く限り同じ間隔で作り直す。
    // 1回目は上で待った11フレーム、以降は作り直した直後の1フレームを差し引く。
    for expected_wait in [11, 1, 1].map(|elapsed| TEXT_ATLAS_OVERFLOW_RETRY_FRAMES - elapsed) {
        let mut waited = 0;
        while draw(&mut atlas) == 0 {
            waited += 1;
            assert!(waited <= TEXT_ATLAS_OVERFLOW_RETRY_FRAMES, "the rebuild never came");
        }
        assert_eq!(waited, expected_wait);
        assert!(atlas.take_rebuilt());
        // 作り直した直後に溢れても、続けて作り直さない。
        assert_eq!(draw(&mut atlas), 0);
        // メモリは atlas の上限と要求したグリフ数を超えない。
        assert!(atlas.atlas_height() <= 64);
        assert!(atlas.glyphs.len() + atlas.deferred.len() <= sizes.len());
        assert!(atlas.layouts.len() <= 1 && atlas.failed.is_empty());
    }
}

/// 2倍でラスタライズした62px、パディング込みで64×64px になる欠字枠。
const EXACT_FIT_TOFU: f32 = 31.0;

/// 1つの欧文フォントで文字列を組んだ1フレーム。
fn latin_text_frame(atlas: &mut TextAtlasCache, text: &str) -> (TextFrame, Vec<TextQuad>) {
    let face = latin();
    atlas.begin_frame();
    let mut builder = CachedTextFrameBuilder::new(atlas);
    builder.push_text(
        &Point { x: 0.1, y: 0.1 },
        text,
        text_style(None),
        single_font_set("skin:atlas", &face),
        SURFACE,
    );
    let quads = builder.quads.clone();
    (builder.finish(Vec::new(), Vec::new()), quads)
}

#[test]
fn exactly_full_atlas_is_reused_without_rebuilding() {
    let mut atlas = TextAtlasCache::with_limits(64, 64);
    atlas.begin_frame();
    let first = atlas.cached_tofu_glyph(EXACT_FIT_TOFU, EXACT_FIT_TOFU).expect("exact fit");
    assert_eq!((first.width, first.height), (62, 62));
    assert_eq!(atlas.size(), AtlasSize { width: 64, height: 64 });
    assert_eq!(atlas.atlas_height(), atlas.max_height());
    assert_eq!(atlas.dirty_regions.len(), 1);
    let pixels = atlas.pixels_for_size(atlas.size());
    for frame in 0..12 {
        atlas.begin_frame();
        let cached = atlas.cached_tofu_glyph(EXACT_FIT_TOFU, EXACT_FIT_TOFU).expect("reused");
        assert_eq!(cached.atlas_origin, first.atlas_origin, "frame {frame}");
        assert_eq!((cached.width, cached.height), (first.width, first.height));
        assert!(atlas.dirty_regions.is_empty(), "frame {frame} re-rasterized");
        assert!(!atlas.take_rebuilt(), "frame {frame} rebuilt the atlas");
        assert_eq!(atlas.glyphs.len(), 1);
        assert_eq!(atlas.pixels_for_size(atlas.size()), pixels, "frame {frame}");
    }
}

#[test]
fn text_filling_the_atlas_height_keeps_its_layout_and_upload() {
    // 文字列の格納でちょうど高さの上限に達する atlas。
    let mut probe = TextAtlasCache::with_limits(64, TEXT_ATLAS_MAX_HEIGHT);
    latin_text_frame(&mut probe, "AB");
    let height = probe.atlas_height();
    let mut atlas = TextAtlasCache::with_limits(64, height);
    let (first, quads) = latin_text_frame(&mut atlas, "AB");
    assert_eq!(quads.len(), 2);
    assert_eq!(atlas.atlas_height(), atlas.max_height());
    assert!(!first.dirty_regions.is_empty());
    assert_eq!(atlas.layouts.len(), 1);
    let glyphs = atlas.glyphs.len();
    for frame in 0..12 {
        let (next, _) = latin_text_frame(&mut atlas, "AB");
        assert!(next.dirty_regions.is_empty(), "frame {frame} re-rasterized");
        assert!(!next.atlas_rebuilt, "frame {frame} rebuilt the atlas");
        assert_eq!(next.size, first.size, "frame {frame}");
        assert_eq!(next.instances, first.instances, "frame {frame} drew differently");
        assert_eq!(atlas.layouts.len(), 1, "frame {frame} dropped the layout cache");
        assert_eq!(atlas.glyphs.len(), glyphs);
    }
}

#[test]
fn new_glyph_in_an_exactly_full_atlas_is_stored_after_one_rebuild() {
    let mut atlas = TextAtlasCache::with_limits(64, 64);
    // 30px（表示15px）の枠4つで、パディング込み 64×64px がちょうど埋まる。
    let old = [15.0, 15.1, 15.2, 14.9];
    atlas.begin_frame();
    let origins: Vec<_> = old
        .iter()
        .map(|width| atlas.cached_tofu_glyph(*width, 15.0).unwrap().atlas_origin)
        .collect();
    assert_eq!(origins, [(1, 1), (33, 1), (1, 33), (33, 33)]);
    assert_eq!(atlas.atlas_height(), 64);

    atlas.begin_frame();
    for (width, origin) in old.iter().zip(&origins) {
        assert_eq!(atlas.cached_tofu_glyph(*width, 15.0).unwrap().atlas_origin, *origin);
    }
    // 空きが無いので新しいグリフは格納できない。失敗扱いにせず、作り直しを待つ。
    let misses = atlas.capacity_misses();
    assert!(atlas.cached_tofu_glyph(10.0, 10.0).is_none());
    assert!(atlas.cached_tofu_glyph(10.0, 10.0).is_none());
    assert_eq!(atlas.capacity_misses(), misses + 2);
    assert_eq!(atlas.deferred.len(), 1);
    assert!(atlas.failed.is_empty());
    assert!(atlas.dirty_regions.is_empty());
    assert_eq!(atlas.glyphs.len(), 4);

    // 次のフレームで作り直し、新しいグリフと必要な古いグリフを入れ直す。
    atlas.begin_frame();
    assert!(atlas.take_rebuilt());
    assert!(atlas.glyphs.is_empty() && atlas.deferred.is_empty());
    let new = atlas.cached_tofu_glyph(10.0, 10.0).expect("stored after the rebuild");
    assert_eq!(new.atlas_origin, (1, 1));
    let kept: Vec<_> =
        old[..2].iter().map(|width| atlas.cached_tofu_glyph(*width, 15.0).unwrap()).collect();
    assert_eq!(atlas.dirty_regions.len(), 3);
    let quads: Vec<_> = std::iter::once(&new)
        .chain(&kept)
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
    // 作り直した atlas のテクスチャには古いグリフが残るので、パディングも透明で書き直す。
    let region = &atlas.dirty_regions[0];
    assert_eq!((region.origin, region.size), ((0, 0), AtlasSize { width: 22, height: 22 }));
    let alpha = |x: u32, y: u32| region.pixels[((y * 22 + x) * 4 + 3) as usize];
    assert!((0..22).all(|i| alpha(i, 0) == 0 && alpha(i, 21) == 0));
    assert!((0..22).all(|i| alpha(0, i) == 0 && alpha(21, i) == 0));
    assert_eq!(alpha(1, 1), 255, "the tofu frame starts inside the padding");

    // 以降は同じ配置を使い続ける。
    for _ in 0..10 {
        atlas.begin_frame();
        assert_eq!(atlas.cached_tofu_glyph(10.0, 10.0).unwrap().atlas_origin, new.atlas_origin);
        for (width, cached) in old[..2].iter().zip(&kept) {
            assert_eq!(
                atlas.cached_tofu_glyph(*width, 15.0).unwrap().atlas_origin,
                cached.atlas_origin
            );
        }
        assert!(atlas.dirty_regions.is_empty() && !atlas.take_rebuilt());
    }
}

#[test]
fn layout_missing_glyphs_for_lack_of_space_is_not_cached() {
    let mut atlas = TextAtlasCache::with_limits(64, 64);
    atlas.begin_frame();
    atlas.cached_tofu_glyph(EXACT_FIT_TOFU, EXACT_FIT_TOFU).expect("exact fit");
    // 同じフレームで新しい文字を組む。空き不足のグリフは欠字枠で代用せず、描かない。
    let face = latin();
    for _ in 0..2 {
        let mut builder = CachedTextFrameBuilder::new(&mut atlas);
        builder.push_text(
            &Point { x: 0.1, y: 0.1 },
            "A",
            text_style(None),
            single_font_set("skin:atlas", &face),
            SURFACE,
        );
        assert!(builder.quads.is_empty(), "{:?}", builder.quads);
    }
    assert_eq!(atlas.layouts.len(), 0, "an incomplete layout was cached");
    assert_eq!(atlas.dirty_regions.len(), 1, "the missing glyph was rasterized again");
    assert!(atlas.failed.is_empty());

    // 作り直した atlas で組み直し、GPU にはテクスチャ全体を書き直させる。
    let (frame, quads) = latin_text_frame(&mut atlas, "A");
    assert!(frame.atlas_rebuilt);
    assert_eq!(quads.len(), 1);
    assert_eq!(atlas.layouts.len(), 1);
    assert_quads_inside_atlas(&quads, &atlas);
    assert_eq!(frame.dirty_regions.len(), 1);
    let (next, _) = latin_text_frame(&mut atlas, "A");
    assert!(!next.atlas_rebuilt && next.dirty_regions.is_empty());
    assert_eq!(next.instances, frame.instances);
}

#[test]
fn every_glyph_kind_waits_for_space_and_is_stored_after_the_rebuild() {
    let vector = latin();
    let color = color_font();
    let font = test_bitmap_font();
    let (ch, bitmap_glyph) = font.glyphs.iter().next().map(|(ch, glyph)| (*ch, *glyph)).unwrap();
    let page = &font.pages[&bitmap_glyph.page];
    let lookup = |atlas: &mut TextAtlasCache| {
        [
            atlas.cached_vector_glyph(&vector, vector.glyph_id('A'), PxScale::from(20.0)),
            atlas.cached_color_glyph(&color, color.glyph_id('😀'), PxScale::from(10.0), false),
            atlas.cached_bitmap_glyph(
                Arc::from("fnt"),
                ch,
                bitmap_glyph,
                page,
                &font,
                PxScale::from(20.0),
            ),
            atlas.cached_tofu_glyph(10.0, 10.0),
        ]
    };
    let mut atlas = TextAtlasCache::with_limits(128, 128);
    atlas.begin_frame();
    atlas.cached_tofu_glyph(63.0, 63.0).expect("exact fit");
    assert_eq!(atlas.atlas_height(), 128);
    for _ in 0..2 {
        assert!(lookup(&mut atlas).iter().all(Option::is_none));
    }
    assert_eq!(atlas.deferred.len(), 4);
    assert!(atlas.failed.is_empty());
    assert_eq!(atlas.dirty_regions.len(), 1);

    atlas.begin_frame();
    let stored = lookup(&mut atlas);
    assert!(stored.iter().all(Option::is_some), "{stored:?}");
    assert_eq!(atlas.dirty_regions.len(), 4);
}

#[test]
fn atlas_with_space_adds_every_glyph_kind_and_reuses_it() {
    let vector = latin();
    let color = color_font();
    let font = test_bitmap_font();
    let (ch, bitmap_glyph) = font.glyphs.iter().next().map(|(ch, glyph)| (*ch, *glyph)).unwrap();
    let page = &font.pages[&bitmap_glyph.page];
    let mut atlas = TextAtlasCache::with_limits(64, 256);
    let mut origins = Vec::new();
    for frame in 0..3 {
        atlas.begin_frame();
        let cached = [
            atlas.cached_vector_glyph(&vector, vector.glyph_id('A'), PxScale::from(20.0)),
            atlas.cached_color_glyph(&color, color.glyph_id('😀'), PxScale::from(10.0), false),
            atlas.cached_bitmap_glyph(
                Arc::from("fnt"),
                ch,
                bitmap_glyph,
                page,
                &font,
                PxScale::from(20.0),
            ),
            atlas.cached_tofu_glyph(10.0, 10.0),
        ]
        .map(|cached| cached.expect("space remains"));
        let found: Vec<_> = cached.iter().map(|cached| cached.atlas_origin).collect();
        if frame == 0 {
            // 幅64px では1行に収まらず、次の行へ折り返す。
            assert_eq!(atlas.dirty_regions.len(), 4);
            assert!(found.iter().any(|origin| origin.0 == TEXT_ATLAS_PADDING && origin.1 > 1));
            origins = found;
        } else {
            assert_eq!(found, origins, "frame {frame}");
            assert!(atlas.dirty_regions.is_empty(), "frame {frame}");
        }
        assert!(atlas.atlas_height() < atlas.max_height());
        assert!(!atlas.take_rebuilt());
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

#[test]
fn texture_grows_by_doubling_and_keeps_its_size_across_rebuilds() {
    // 30px（表示15px）の枠は、パディング込みで1行に2つ、32px ごとに並ぶ。
    let tofu = |atlas: &mut TextAtlasCache, index: u32| {
        atlas.cached_tofu_glyph(15.0 - index as f32 * 0.001, 15.0)
    };
    let mut atlas = TextAtlasCache::with_limits(64, 1024);
    atlas.begin_frame();
    tofu(&mut atlas, 0).unwrap();
    // 格納済みは1行でも、テクスチャは最小の高さで確保する。
    assert_eq!(atlas.atlas_height(), 32);
    assert_eq!(atlas.size(), AtlasSize { width: 64, height: TEXT_ATLAS_MIN_TEXTURE_HEIGHT });

    // 行が増えてもテクスチャの高さを超えるまでは同じ寸法で、超えたら倍に広げる。
    let mut heights = vec![atlas.size().height];
    for index in 1..64 {
        tofu(&mut atlas, index).unwrap();
        let height = atlas.size().height;
        assert!(height >= atlas.atlas_height());
        if heights.last() != Some(&height) {
            heights.push(height);
        }
    }
    assert_eq!(heights, [256, 512, 1024]);
    assert_eq!(atlas.atlas_height(), 1024);

    // 溢れて作り直しても縮めない。GPU テクスチャを作り直して全体を転送し直さないため。
    assert!(tofu(&mut atlas, 64).is_none());
    atlas.begin_frame();
    assert!(atlas.take_rebuilt());
    assert!(atlas.glyphs.is_empty());
    tofu(&mut atlas, 64).unwrap();
    assert_eq!(atlas.atlas_height(), 32);
    assert_eq!(atlas.size().height, 1024);
}

#[test]
fn texture_height_stops_at_the_atlas_limit() {
    let mut atlas = TextAtlasCache::with_limits(64, 300);
    atlas.begin_frame();
    // 9行（288px）で最小の高さを超えるが、倍の512px ではなく上限の300px に留める。
    for index in 0..17 {
        atlas.cached_tofu_glyph(15.0 - index as f32 * 0.001, 15.0).unwrap();
    }
    assert_eq!(atlas.atlas_height(), 288);
    assert_eq!(atlas.size().height, 300);
}

/// 縁取り付きの文字列を `origin` に組んだ1フレームの quad。
fn outlined_latin_quads(atlas: &mut TextAtlasCache, origin: Point) -> Vec<TextQuad> {
    let face = latin();
    let mut style = text_style(None);
    style.outline =
        Some(crate::plan::TextOutline { color: Color::rgb(0.0, 0.0, 0.0), width: 0.01 });
    atlas.begin_frame();
    let mut builder = CachedTextFrameBuilder::new(atlas);
    builder.push_text(&origin, "AB", style, single_font_set("skin:atlas", &face), SURFACE);
    builder.quads.clone()
}

#[test]
fn moving_outlined_text_reuses_its_layout() {
    let mut atlas = TextAtlasCache::new(TEXT_ATLAS_WIDTH);
    let first = outlined_latin_quads(&mut atlas, Point { x: 0.1, y: 0.1 });
    // 縁取り8回と本体で9回分。縁取りは位置だけが違うので1つのレイアウトを共有する。
    assert_eq!(first.len(), 18);
    assert_eq!(atlas.layouts.len(), 2);
    let glyphs = atlas.glyphs.len();

    // スクロールで動いた文字列は、組み直さずにキャッシュを動かして並べる。
    let moved = outlined_latin_quads(&mut atlas, Point { x: 0.3, y: 0.25 });
    assert_eq!(atlas.layouts.len(), 2);
    assert_eq!(atlas.glyphs.len(), glyphs);
    assert!(atlas.dirty_regions.is_empty());
    let mut uncached = TextAtlasCache::new(TEXT_ATLAS_WIDTH);
    let expected = outlined_latin_quads(&mut uncached, Point { x: 0.3, y: 0.25 });
    assert_eq!(moved.len(), expected.len());
    for (moved, expected) in moved.iter().zip(&expected) {
        assert!((moved.x - expected.x).abs() < 1e-6 && (moved.y - expected.y).abs() < 1e-6);
        assert_eq!((moved.atlas_origin, moved.width), (expected.atlas_origin, expected.width));
        assert_eq!(moved.color, expected.color);
    }

    // 同じ位置に戻れば、最初のフレームと同じ値になる。
    let back = outlined_latin_quads(&mut atlas, Point { x: 0.1, y: 0.1 });
    let positions = |quads: &[TextQuad]| quads.iter().map(|q| (q.x, q.y)).collect::<Vec<_>>();
    assert_eq!(positions(&back), positions(&first));
}

#[test]
fn shrunk_text_reuses_glyphs_across_widths_and_keeps_the_fitted_width() {
    let face = latin();
    let mut atlas = TextAtlasCache::new(TEXT_ATLAS_WIDTH);
    for step in 0..40 {
        let mut style = text_style(None);
        style.overflow = TextOverflow::Shrink;
        style.max_width = 0.2 + step as f32 * 0.002;
        let max_width = style.max_width;
        atlas.begin_frame();
        let mut builder = CachedTextFrameBuilder::new(&mut atlas);
        builder.push_text(
            &Point { x: 0.0, y: 0.1 },
            "WIDE TEXT",
            style,
            single_font_set("skin:atlas", &face),
            SURFACE,
        );
        let quads = builder.quads.clone();
        let right = quads.iter().map(|q| q.x + q.width).fold(0.0, f32::max);
        // 刻んだ倍率でラスタライズしても、表示は指定した幅に収める。
        assert!(right <= max_width + 0.004, "step {step}: {right} > {max_width}");
        assert!(right >= max_width * 0.95, "step {step}: {right} < {max_width}");
    }
    // 40通りの幅でも、横の倍率は16段階までしか増えない（7文字 × 16）。
    assert!(atlas.glyphs.len() <= 7 * 16, "{} glyphs", atlas.glyphs.len());
}

#[test]
fn shrunk_raster_scale_rounds_up_only_horizontal_shrink() {
    let same = PxScale { x: 40.0, y: 40.0 };
    assert_eq!(shrunk_raster_scale(same), (same, 1.0));
    let (raster, stretch) = shrunk_raster_scale(PxScale { x: 30.1, y: 40.0 });
    assert_eq!(raster, PxScale { x: 32.5, y: 40.0 });
    assert!((raster.x * stretch - 30.1).abs() < 1e-4);
}

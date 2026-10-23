//! atlas の部分転送で、GPU テクスチャの内容が CPU 側の atlas と食い違わないことの回帰テスト。
//!
//! GPU テクスチャを画素配列で再現し、`upload_text_atlas` と同じ `write_text_atlas` で転送を当てる。
//! テクスチャを作り直したときだけ全体を書き、それ以外は `dirty_regions` だけを書く。
//! 使っていない領域に古い画素が残るのは許容し、格納済みのグリフとその周囲1pxを比べる。

use super::unicode_text::*;
use super::*;

/// 古い画素の代わりに、作り直したテクスチャの未使用領域へ入れておく値。
const STALE: [u8; 4] = [7, 70, 140, 210];

struct SimulatedTexture {
    size: AtlasSize,
    pixels: Vec<u8>,
    full_uploads: usize,
    region_uploads: usize,
}

impl SimulatedTexture {
    fn new() -> Self {
        Self { size: AtlasSize::default(), pixels: Vec::new(), full_uploads: 0, region_uploads: 0 }
    }

    /// `WgpuRenderer::upload_text_atlas` と同じく、寸法が変わったらテクスチャを作り直して転送する。
    fn upload(&mut self, frame: &TextFrame, atlas: &TextAtlasCache) -> TextAtlasUploadStats {
        let recreated = frame.size.width != 0 && frame.size.height != 0 && self.size != frame.size;
        if recreated {
            self.size = frame.size;
            self.pixels = vec![0; (frame.size.width * frame.size.height * 4) as usize];
        }
        let texture_width = self.size.width;
        let pixels = &mut self.pixels;
        let stats = write_text_atlas(
            frame,
            recreated,
            || atlas.pixels_for_size(frame.size),
            |origin, size, source| {
                let row = (size.width * 4) as usize;
                for y in 0..size.height {
                    let src = (y * size.width * 4) as usize;
                    let dst = (((origin.1 + y) * texture_width + origin.0) * 4) as usize;
                    pixels[dst..dst + row].copy_from_slice(&source[src..src + row]);
                }
            },
        );
        self.full_uploads += stats.full_uploads;
        self.region_uploads += stats.region_uploads;
        stats
    }

    /// 作り直した atlas では、前の内容が古い画素として残る。比較を厳しくするため、
    /// 格納済みのグリフに属さない画素を目立つ値で埋める。
    fn stain_unused(&mut self, atlas: &TextAtlasCache) {
        let mut used = vec![false; (self.size.width * self.size.height) as usize];
        for glyph in atlas.glyphs.values() {
            for (x, y) in padded_pixels(glyph) {
                used[(y * self.size.width + x) as usize] = true;
            }
        }
        for (index, used) in used.iter().enumerate() {
            if !used {
                self.pixels[index * 4..index * 4 + 4].copy_from_slice(&STALE);
            }
        }
    }
}

/// グリフの領域と、線形補間で読む周囲1pxのパディング。
fn padded_pixels(glyph: &CachedGlyph) -> impl Iterator<Item = (u32, u32)> {
    let (x0, y0) =
        (glyph.atlas_origin.0 - TEXT_ATLAS_PADDING, glyph.atlas_origin.1 - TEXT_ATLAS_PADDING);
    let (width, height) =
        (glyph.width + TEXT_ATLAS_PADDING * 2, glyph.height + TEXT_ATLAS_PADDING * 2);
    (y0..y0 + height).flat_map(move |y| (x0..x0 + width).map(move |x| (x, y)))
}

/// 1フレームぶんの転送内容（`CachedTextFrameBuilder::finish` と同じ取り出し方）。
fn take_frame(atlas: &mut TextAtlasCache) -> TextFrame {
    TextFrame {
        size: atlas.size(),
        atlas_rebuilt: atlas.take_rebuilt(),
        dirty_regions: std::mem::take(&mut atlas.dirty_regions),
        ..TextFrame::default()
    }
}

/// 格納済みの全グリフについて、GPU 側の画素が CPU 側と一致し、パディングが透明であること。
fn assert_texture_matches(texture: &SimulatedTexture, atlas: &TextAtlasCache, stage: &str) {
    let size = atlas.size();
    assert_eq!(texture.size, size, "{stage}: texture size");
    let cpu = atlas.pixels_for_size(size);
    for glyph in atlas.glyphs.values() {
        assert!(glyph.atlas_origin.0 + glyph.width + TEXT_ATLAS_PADDING <= size.width, "{stage}");
        assert!(glyph.atlas_origin.1 + glyph.height + TEXT_ATLAS_PADDING <= size.height, "{stage}");
        for (x, y) in padded_pixels(glyph) {
            let index = ((y * size.width + x) * 4) as usize;
            assert_eq!(
                texture.pixels[index..index + 4],
                cpu[index..index + 4],
                "{stage}: pixel ({x}, {y}) of {glyph:?}"
            );
            let inside = x >= glyph.atlas_origin.0
                && y >= glyph.atlas_origin.1
                && x < glyph.atlas_origin.0 + glyph.width
                && y < glyph.atlas_origin.1 + glyph.height;
            if !inside {
                assert_eq!(
                    texture.pixels[index + 3],
                    0,
                    "{stage}: padding ({x}, {y}) of {glyph:?}"
                );
            }
        }
    }
}

/// 簡単な線形合同法。テストごとに同じ寸法の並びを作る。
struct Sizes(u32);

impl Sizes {
    fn next(&mut self, max: f32) -> f32 {
        self.0 = self.0.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        3.0 + (self.0 >> 8) as f32 / (1u32 << 24) as f32 * (max - 3.0)
    }
}

/// 毎フレーム新しい寸法のグリフを足し、溢れて作り直すまで続ける。
fn run_frames(atlas: &mut TextAtlasCache, frames: usize, glyphs_per_frame: usize, max: f32) {
    let face = latin();
    let mut texture = SimulatedTexture::new();
    let mut sizes = Sizes(42);
    let mut rebuilds = 0;
    let mut previous_size = None;
    for frame in 0..frames {
        atlas.begin_frame();
        for index in 0..glyphs_per_frame {
            let (width, height) = (sizes.next(max), sizes.next(max));
            // 欠字枠（白い枠と透明な内側）とベクターグリフを混ぜる。
            if index % 3 == 0 {
                let _ = atlas.cached_vector_glyph(
                    &face,
                    face.glyph_id('W'),
                    PxScale::from(height * 2.0),
                );
            } else {
                let _ = atlas.cached_tofu_glyph(width, height);
            }
        }
        let frame_data = take_frame(atlas);
        if frame_data.atlas_rebuilt {
            rebuilds += 1;
            // 作り直しではテクスチャを作り直さず、古い画素が残る。
            assert_eq!(Some(frame_data.size), previous_size, "frame {frame}: texture was resized");
            texture.stain_unused(atlas);
        }
        texture.upload(&frame_data, atlas);
        assert_texture_matches(&texture, atlas, &format!("frame {frame}"));
        previous_size = Some(frame_data.size);
    }
    assert!(rebuilds >= 2, "the atlas was rebuilt {rebuilds} times");
    // 全体の転送はテクスチャを作るときだけで、作り直しのたびには起きない。
    assert!(texture.full_uploads < rebuilds, "{} full uploads", texture.full_uploads);
    assert!(texture.region_uploads > 0);
}

#[test]
fn partial_uploads_keep_the_texture_in_sync_across_growth_and_rebuilds() {
    // 256 → 512 → 1024px と倍に広げ、上限で溢れて何度も作り直す。
    let mut atlas = TextAtlasCache::with_limits(128, 1024);
    run_frames(&mut atlas, 120, 6, 40.0);
    assert_eq!(atlas.size().height, 1024);
}

#[test]
fn partial_uploads_stay_in_sync_on_a_gpu_with_a_small_texture_limit() {
    // 上限が最小の高さより小さい GPU。テクスチャは最初から上限の寸法になる。
    let mut atlas = TextAtlasCache::for_texture_limit(96);
    assert_eq!((atlas.width(), atlas.max_height()), (96, 96));
    run_frames(&mut atlas, 80, 4, 30.0);
    assert_eq!(atlas.size(), AtlasSize { width: 96, height: 96 });
}

#[test]
fn glyphs_touching_the_right_and_bottom_edges_keep_their_padding() {
    let mut atlas = TextAtlasCache::with_limits(64, 64);
    let mut texture = SimulatedTexture::new();
    atlas.begin_frame();
    // 30px（表示15px）の枠4つで、パディング込み 64×64px がちょうど埋まる。
    // 隣り合う転送領域はパディングだけが接し、互いに上書きしない。
    let origins: Vec<_> = [15.0, 15.1, 15.2, 14.9]
        .iter()
        .map(|width| atlas.cached_tofu_glyph(*width, 15.0).unwrap().atlas_origin)
        .collect();
    assert_eq!(origins, [(1, 1), (33, 1), (1, 33), (33, 33)]);
    let frame = take_frame(&mut atlas);
    assert_eq!(frame.dirty_regions.len(), 4);
    let regions: Vec<_> = frame.dirty_regions.iter().map(|r| (r.origin, r.size)).collect();
    let padded = AtlasSize { width: 32, height: 32 };
    assert_eq!(
        regions,
        [((0, 0), padded), ((32, 0), padded), ((0, 32), padded), ((32, 32), padded)]
    );
    texture.upload(&frame, &atlas);
    assert_texture_matches(&texture, &atlas, "full");

    // 溢れて作り直し、別の寸法・位置に格納する。右端・下端に接する位置を含む。
    atlas.begin_frame();
    assert!(atlas.cached_tofu_glyph(10.0, 10.0).is_none());
    let _ = take_frame(&mut atlas);
    atlas.begin_frame();
    // 22×22、42×26（右端まで）、32×38（下端まで）の転送領域。
    let new: Vec<_> = [(10.0, 10.0), (20.0, 12.0), (15.0, 18.0)]
        .iter()
        .map(|(width, height)| atlas.cached_tofu_glyph(*width, *height).unwrap())
        .collect();
    let frame = take_frame(&mut atlas);
    assert!(frame.atlas_rebuilt);
    assert_eq!(frame.size, AtlasSize { width: 64, height: 64 });
    assert_eq!(new[1].atlas_origin.0 + new[1].width + TEXT_ATLAS_PADDING, 64);
    assert_eq!(new[2].atlas_origin.1 + new[2].height + TEXT_ATLAS_PADDING, 64);
    texture.stain_unused(&atlas);
    texture.upload(&frame, &atlas);
    assert_eq!(texture.full_uploads, 1, "the rebuild must not upload the whole texture");
    assert_texture_matches(&texture, &atlas, "rebuilt");
}

/// `dirty_regions` を全部書いたときの統計。
fn region_stats(frame: &TextFrame) -> TextAtlasUploadStats {
    TextAtlasUploadStats {
        writes: frame.dirty_regions.len(),
        bytes: frame.dirty_regions.iter().map(|region| region.pixels.len()).sum(),
        full_uploads: 0,
        region_uploads: frame.dirty_regions.len(),
    }
}

fn plan_frame(atlas: &mut TextAtlasCache, text: &str) -> TextFrame {
    let commands =
        if text.is_empty() { Vec::new() } else { vec![text_command(text, text_style(None))] };
    let plan = DrawPlan { clear: Color::rgb(0.0, 0.0, 0.0), commands };
    build_text_frame_with_cache(&plan, &latin(), &HashMap::new(), &HashMap::new(), SURFACE, atlas)
}

#[test]
fn upload_stats_count_only_atlas_texture_writes() {
    let mut atlas = TextAtlasCache::new(TEXT_ATLAS_WIDTH);
    let mut texture = SimulatedTexture::new();

    // 最初のフレームはテクスチャを作るので全体を1回だけ書く。部分転送は送らない。
    let first = plan_frame(&mut atlas, "AW");
    assert!(!first.dirty_regions.is_empty());
    let stats = texture.upload(&first, &atlas);
    let full_bytes = (first.size.width * first.size.height * 4) as usize;
    assert_eq!(
        stats,
        TextAtlasUploadStats { writes: 1, bytes: full_bytes, full_uploads: 1, region_uploads: 0 }
    );
    assert_texture_matches(&texture, &atlas, "full");

    // 同じ文字列ではインスタンスだけが変わり、atlas には何も書かない。
    let same = plan_frame(&mut atlas, "AW");
    assert!(!same.instances.is_empty());
    assert!(same.dirty_regions.is_empty());
    assert_eq!(texture.upload(&same, &atlas), TextAtlasUploadStats::default());

    // 新しいグリフは、その領域だけを書く。
    let more = plan_frame(&mut atlas, "AWm1");
    assert_eq!(more.size, first.size);
    assert!(!more.dirty_regions.is_empty());
    assert_eq!(texture.upload(&more, &atlas), region_stats(&more));
    assert_texture_matches(&texture, &atlas, "partial");

    // 文字が無いフレームでも、atlas に変更が無ければ何も書かない。
    let empty = plan_frame(&mut atlas, "");
    assert!(empty.instances.is_empty());
    assert!(empty.command_quad_counts.is_empty());
    assert_eq!(texture.upload(&empty, &atlas), TextAtlasUploadStats::default());
}

#[test]
fn upload_stats_after_a_rebuild_count_only_the_new_regions() {
    let mut atlas = TextAtlasCache::with_limits(64, 64);
    let mut texture = SimulatedTexture::new();
    atlas.begin_frame();
    for width in [15.0, 15.1, 15.2, 14.9] {
        let _ = atlas.cached_tofu_glyph(width, 15.0);
    }
    let _ = texture.upload(&take_frame(&mut atlas), &atlas);
    atlas.begin_frame();
    assert!(atlas.cached_tofu_glyph(10.0, 10.0).is_none());
    let overflowed = take_frame(&mut atlas);
    assert_eq!(texture.upload(&overflowed, &atlas), region_stats(&overflowed));

    atlas.begin_frame();
    let _ = atlas.cached_tofu_glyph(10.0, 10.0).unwrap();
    let rebuilt = take_frame(&mut atlas);
    assert!(rebuilt.atlas_rebuilt);
    assert_eq!(rebuilt.dirty_regions.len(), 1);
    texture.stain_unused(&atlas);
    let stats = texture.upload(&rebuilt, &atlas);
    assert_eq!(stats, region_stats(&rebuilt));
    assert_eq!(stats.full_uploads, 0);
    assert_texture_matches(&texture, &atlas, "rebuilt");
}

#[test]
fn upload_stats_are_empty_without_an_atlas() {
    // 描けない画面ではフレームが空で、テクスチャを作る扱いでも何も書かない。
    let stats = write_text_atlas(
        &TextFrame::default(),
        true,
        || panic!("an empty atlas must not be read"),
        |_, _, _| panic!("an empty atlas must not be written"),
    );
    assert_eq!(stats, TextAtlasUploadStats::default());
}

#[test]
fn text_instance_buffer_grows_to_the_next_power_of_two() {
    assert_eq!(grown_text_buffer_capacity(0, 0), None);
    assert_eq!(grown_text_buffer_capacity(TEXT_INSTANCE_BYTES, 0), Some(64));
    assert_eq!(grown_text_buffer_capacity(64, 64), None);
    assert_eq!(grown_text_buffer_capacity(32, 64), None);
    assert_eq!(grown_text_buffer_capacity(65, 64), Some(128));
    assert_eq!(grown_text_buffer_capacity(TEXT_INSTANCE_BYTES * 100, 1024), Some(8192));
}

/// UVが指すグリフ画素そのものを読む。CPUとGPUが同じ間違いをしてもfreshとの比較で検出する。
fn drawn_pixels(frame: &TextFrame, texture: &SimulatedTexture) -> Vec<Vec<u8>> {
    frame
        .instances
        .as_chunks::<TEXT_INSTANCE_BYTES>()
        .0
        .iter()
        .map(|bytes| {
            let floats: Vec<_> =
                bytes.as_chunks::<4>().0.iter().map(|v| f32::from_le_bytes(*v)).collect();
            let x = (floats[4] * texture.size.width as f32).round() as u32;
            let y = (floats[5] * texture.size.height as f32).round() as u32;
            let w = (floats[6] * texture.size.width as f32).round() as u32;
            let h = (floats[7] * texture.size.height as f32).round() as u32;
            let mut pixels = Vec::new();
            for row in y..y + h {
                let offset = ((row * texture.size.width + x) * 4) as usize;
                pixels.extend_from_slice(&texture.pixels[offset..offset + (w * 4) as usize]);
            }
            pixels
        })
        .collect()
}

fn scrolling_plan(index: usize, mixed: bool) -> DrawPlan {
    let mut style = text_style(None);
    // ホイールの行ごとのサイズも入れ替わる。横倍率だけの有限集合で飽和させない。
    style.size = 0.08 + (index % 53) as f32 * 0.00031;
    style.overflow = TextOverflow::Shrink;
    style.max_width = 0.04 + (index % 37) as f32 * 0.0035;
    let title = format!("{}W", "A".repeat(3 + index % 41));
    let mut commands = vec![text_command(&title, style)];
    if mixed {
        let mut style = text_style(None);
        style.size = 0.035;
        style.outline =
            Some(crate::plan::TextOutline { color: Color::rgb(0.0, 0.0, 0.0), width: 0.0035 });
        style.shadow = Some(crate::plan::TextShadow {
            color: Color::rgba(0.1, 0.2, 0.3, 0.5),
            offset: Point { x: 0.01, y: 0.01 },
        });
        commands.push(text_command("AW", style.clone()));
        style.font_id = Some("emoji".into());
        commands.push(text_command("😀", style.clone()));
        style.font_id = Some("bitmap".into());
        commands.push(text_command("AW", style));
    }
    DrawPlan { clear: Color::rgb(0.0, 0.0, 0.0), commands }
}

fn scroll_frames(enabled: bool, mixed: bool) -> (u64, u64, usize) {
    let face = latin();
    let fonts = HashMap::from([("emoji".into(), color_font())]);
    let mut bitmap = test_bitmap_font();
    bitmap.fallback_style = Some(Arc::new(crate::bitmap_font::BitmapFallbackStyle {
        fill_top: 0,
        fill_rows: vec![[120, 180, 240]; 10],
        outline: Some(crate::bitmap_font::BitmapFallbackOutline {
            width: 0.7,
            color: [30, 40, 50],
        }),
    }));
    let bitmaps = HashMap::from([("bitmap".into(), bitmap)]);
    let mut atlas = TextAtlasCache::with_limits(128, 256);
    atlas.recycling_enabled = enabled;
    let mut texture = SimulatedTexture::new();
    let mut missing = 0;
    let mut kinds = std::collections::HashSet::new();
    for index in 0..320 {
        let plan = scrolling_plan(index, mixed);
        let frame =
            build_text_frame_with_cache(&plan, &face, &fonts, &bitmaps, SURFACE, &mut atlas);
        texture.upload(&frame, &atlas);
        kinds.extend(atlas.glyphs.keys().map(|key| key.kind));
        assert_texture_matches(&texture, &atlas, &format!("scroll {index}"));
        // 汚れた未使用領域へ再配置することを次のフレームで検証する。
        texture.stain_unused(&atlas);
        let mut fresh = TextAtlasCache::with_limits(128, 8192);
        let expected =
            build_text_frame_with_cache(&plan, &face, &fonts, &bitmaps, SURFACE, &mut fresh);
        let mut expected_texture = SimulatedTexture::new();
        expected_texture.upload(&expected, &fresh);
        let actual = drawn_pixels(&frame, &texture);
        let expected = drawn_pixels(&expected, &expected_texture);
        missing += expected.len().saturating_sub(actual.len());
        if enabled {
            assert_eq!(actual, expected, "scroll {index}: layout points to the wrong pixels");
        }
    }
    if mixed {
        for kind in [
            TextGlyphKind::Vector,
            TextGlyphKind::Bitmap,
            TextGlyphKind::Color,
            TextGlyphKind::ColorSilhouette,
            TextGlyphKind::StyledFallback,
        ] {
            assert!(kinds.contains(&kind), "fixture did not exercise {kind:?}");
        }
    }
    (atlas.full_clears, atlas.recycles, missing)
}

#[test]
fn scrolling_recycles_history_instead_of_clearing_and_does_not_drop_more_glyphs() {
    let legacy = scroll_frames(false, false);
    let recycled = scroll_frames(true, false);
    assert!(legacy.0 >= 3, "legacy must exercise repeated full clears: {legacy:?}");
    assert!(recycled.1 >= 3, "must exercise repeated recycling: {recycled:?}");
    assert!(recycled.0 < legacy.0, "{recycled:?} / {legacy:?}");
    assert!(recycled.2 <= legacy.2, "{recycled:?} / {legacy:?}");
}

#[test]
fn mixed_cached_layouts_and_padding_keep_their_pixels_after_shelf_reuse() {
    let result = scroll_frames(true, true);
    assert!(result.1 >= 3, "must reuse shelves repeatedly: {result:?}");
    assert_eq!(result.2, 0);
}

#[test]
fn retry_keeps_the_delay_when_a_single_frame_exceeds_a_small_gpu_atlas() {
    let face = latin();
    let mut atlas = TextAtlasCache::for_texture_limit(96);
    let mut texture = SimulatedTexture::new();
    let commands = (0..30)
        .map(|index| {
            let mut style = text_style(None);
            style.size = 0.08 + index as f32 * 0.001;
            text_command("AW", style)
        })
        .collect();
    let plan = DrawPlan { clear: Color::rgb(0.0, 0.0, 0.0), commands };
    for index in 0..120 {
        let frame = build_text_frame_with_cache(
            &plan,
            &face,
            &HashMap::new(),
            &HashMap::new(),
            SURFACE,
            &mut atlas,
        );
        texture.upload(&frame, &atlas);
        assert_texture_matches(&texture, &atlas, &format!("oversized frame {index}"));
        assert!(frame.command_quad_counts.iter().sum::<usize>() < 60);
    }
    assert_eq!(atlas.size(), AtlasSize { width: 96, height: 96 });
    assert!(atlas.full_clears <= 3, "{} full clears", atlas.full_clears);
    assert_eq!(atlas.recycles, 0, "all resident glyphs are still in use");
}

#[test]
fn dropping_the_layout_cache_does_not_lose_recent_glyph_references() {
    let mut atlas = TextAtlasCache::with_limits(128, 256);
    let mut texture = SimulatedTexture::new();
    let first = plan_frame(&mut atlas, "AW");
    texture.upload(&first, &atlas);
    let expected = drawn_pixels(&first, &texture);
    let mut key = atlas.layouts.entries.keys().next().unwrap().clone();
    for index in 0..20 {
        atlas.cached_tofu_glyph(3.0 + index as f32 * 0.001, 3.0).unwrap();
    }
    texture.upload(&take_frame(&mut atlas), &atlas);
    for _ in 0..4 {
        let frame = plan_frame(&mut atlas, "AW");
        texture.upload(&frame, &atlas);
    }
    // グリフhitを通らずレイアウトhitだけで使っていた文字。キャッシュ上限で参照を失わせる。
    for index in 0..TEXT_LAYOUT_CACHE_MAX_ENTRIES {
        key.text = format!("empty {index}");
        atlas.insert_layout(key.clone(), Vec::new());
    }
    assert!(atlas.recycle_inactive_shelves());
    assert!(atlas.glyphs.len() < 22, "stale rows must be evicted");
    assert_eq!(atlas.glyphs.keys().filter(|k| k.kind == TextGlyphKind::Vector).count(), 2);
    let moved = take_frame(&mut atlas);
    texture.stain_unused(&atlas);
    texture.upload(&moved, &atlas);
    assert_texture_matches(&texture, &atlas, "layout cache pressure");
    let frame = plan_frame(&mut atlas, "AW");
    texture.upload(&frame, &atlas);
    assert_eq!(drawn_pixels(&frame, &texture), expected);
    assert!(frame.dirty_regions.is_empty(), "retained glyphs must not be rasterized again");
}

#[test]
fn evicted_layout_is_invalidated_and_rebuilt_on_a_small_gpu() {
    let mut atlas = TextAtlasCache::for_texture_limit(96);
    let mut texture = SimulatedTexture::new();
    let seed = plan_frame(&mut atlas, "A");
    texture.upload(&seed, &atlas);
    let a = atlas.glyphs.values().next().unwrap();
    let remaining = 96 - a.atlas_origin.0 - a.width - TEXT_ATLAS_PADDING;
    atlas.cached_tofu_glyph((remaining - 2) as f32 / 2.0, 2.0).unwrap();
    texture.upload(&take_frame(&mut atlas), &atlas);
    let first = plan_frame(&mut atlas, "AW");
    texture.upload(&first, &atlas);
    let expected = drawn_pixels(&first, &texture);
    let old_key = atlas.layouts.entries.keys().find(|k| k.text == "AW").unwrap().clone();
    for _ in 0..4 {
        let frame = plan_frame(&mut atlas, "");
        texture.upload(&frame, &atlas);
    }
    let recent = plan_frame(&mut atlas, "W");
    texture.upload(&recent, &atlas);
    assert!(atlas.recycle_inactive_shelves());
    assert_eq!(atlas.glyphs.len(), 1);
    assert!(!atlas.layouts.entries.contains_key(&old_key), "AW references the evicted A");
    let moved = take_frame(&mut atlas);
    texture.stain_unused(&atlas);
    let stats = texture.upload(&moved, &atlas);
    assert_eq!(stats.full_uploads, 0);
    assert_texture_matches(&texture, &atlas, "small GPU relocation");
    let back = plan_frame(&mut atlas, "AW");
    texture.upload(&back, &atlas);
    assert_eq!(drawn_pixels(&back, &texture), expected);
    assert_eq!(back.dirty_regions.len(), 1, "only the evicted A needs rasterization");
    assert_eq!(atlas.size(), AtlasSize { width: 96, height: 96 });
}

//! atlas の部分転送で、GPU テクスチャの内容が CPU 側の atlas と食い違わないことの回帰テスト。
//!
//! GPU テクスチャを画素配列で再現し、`upload_text_frame` と同じ規則で転送を当てる。
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

    /// `WgpuRenderer::upload_text_frame` と同じ判定で転送する。
    fn upload(&mut self, frame: &TextFrame, atlas: &TextAtlasCache) {
        if frame.size.width == 0 || frame.size.height == 0 {
            return;
        }
        if self.size != frame.size {
            self.size = frame.size;
            self.pixels = atlas.pixels_for_size(frame.size);
            self.full_uploads += 1;
            return;
        }
        for region in &frame.dirty_regions {
            for y in 0..region.size.height {
                let row = (region.size.width * 4) as usize;
                let src = (y * region.size.width * 4) as usize;
                let dst =
                    (((region.origin.1 + y) * self.size.width + region.origin.0) * 4) as usize;
                self.pixels[dst..dst + row].copy_from_slice(&region.pixels[src..src + row]);
            }
            self.region_uploads += 1;
        }
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

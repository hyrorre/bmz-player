//! BGA images that play animated GIF / WebP / APNG frames (BMZ extension).
//!
//! beatoraja shows the sprite sheet of an animated BGA image as one picture. BMZ keeps the
//! decoded frames on the CPU and rewrites the asset's texture when the frame due at the
//! current chart time changes. Channel origins follow video BGA, so seeks and retries need
//! no extra playback state.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use bmz_chart::model::{BgaAssetId, BgaEventKind};
use bmz_core::time::TimeUs;
use bmz_render::assets::{
    AnimatedImage, AnimatedImageLimits, DecodedImage, RgbaImageAsset, load_chart_bga_frames,
    load_chart_image,
};
use bmz_render::plan::TextureId;

use crate::audio::RunningPlaySession;
use crate::screens::play_snapshot::bga_texture_id;
use crate::video_bga::{ActiveBgaChannel, active_bga_channels};

pub type BgaAnimationCatalog = HashMap<BgaAssetId, Arc<AnimatedImage>>;

/// Decoded animation frames kept for one chart's BGA. Later animations fall back to their
/// first frame once the budget is spent.
pub const MAX_CHART_ANIMATED_BGA_RGBA_BYTES: u64 = 512 * 1024 * 1024;

/// Splits [`MAX_CHART_ANIMATED_BGA_RGBA_BYTES`] across one chart's BGA images.
#[derive(Debug, Clone, Copy)]
pub struct AnimatedBgaBudget {
    remaining: u64,
}

impl Default for AnimatedBgaBudget {
    fn default() -> Self {
        Self { remaining: MAX_CHART_ANIMATED_BGA_RGBA_BYTES }
    }
}

impl AnimatedBgaBudget {
    /// A budget of `total_bytes` for one chart (tests use small budgets).
    pub fn with_total(total_bytes: u64) -> Self {
        Self { remaining: total_bytes }
    }

    /// Decodes a BGA image padded like beatoraja, keeping its frames if the budget allows.
    /// The limits apply to the padded frames while they are decoded, so an animation over
    /// the budget is never fully allocated; it falls back to its first frame with a warning.
    pub fn load(&mut self, path: &Path) -> anyhow::Result<DecodedImage> {
        self.load_with(path, load_chart_bga_frames)
    }

    /// Video export keeps its unpadded BGA images.
    pub fn load_unpadded(&mut self, path: &Path) -> anyhow::Result<DecodedImage> {
        self.load_with(path, load_chart_image)
    }

    fn load_with(
        &mut self,
        path: &Path,
        load: fn(&Path, &AnimatedImageLimits) -> anyhow::Result<DecodedImage>,
    ) -> anyhow::Result<DecodedImage> {
        let limits = AnimatedImageLimits::CHART
            .with_max_rgba_bytes(self.remaining.min(AnimatedImageLimits::CHART.max_rgba_bytes));
        let decoded = load(path, &limits)?;
        if let Some(animation) = decoded.animation() {
            self.remaining = self.remaining.saturating_sub(animation.rgba_bytes());
        }
        Ok(decoded)
    }
}

#[derive(Debug, Default, Clone)]
pub struct AnimatedBgaState {
    pub catalog: BgaAnimationCatalog,
    /// Frame index currently in each asset's texture. Missing means unknown.
    uploaded: HashMap<BgaAssetId, usize>,
}

impl AnimatedBgaState {
    /// Textures may hold any frame (for example after a retry), so the first update uploads.
    pub fn from_catalog(catalog: BgaAnimationCatalog) -> Self {
        Self { catalog, uploaded: HashMap::new() }
    }

    /// Registers an animation whose texture was just created from its first frame.
    pub fn insert_uploaded_first(&mut self, asset_id: BgaAssetId, animation: Arc<AnimatedImage>) {
        self.catalog.insert(asset_id, animation);
        self.uploaded.insert(asset_id, 0);
    }

    /// Uploads the frame due at `chart_now` for each active channel. Each asset has one
    /// texture, so the first channel showing an asset decides its frame.
    pub fn update(
        &mut self,
        channels: &[ActiveBgaChannel],
        chart_now: TimeUs,
        mut upload: impl FnMut(BgaAssetId, &RgbaImageAsset) -> bool,
    ) {
        let mut claimed = Vec::with_capacity(channels.len());
        for channel in channels {
            if claimed.contains(&channel.asset_id) {
                continue;
            }
            claimed.push(channel.asset_id);
            let Some(animation) = self.catalog.get(&channel.asset_id) else {
                continue;
            };
            let elapsed_us =
                u64::try_from(chart_now.0.saturating_sub(channel.origin.0)).unwrap_or(0);
            let index = animation.frame_index(elapsed_us);
            if self.uploaded.get(&channel.asset_id) == Some(&index) {
                continue;
            }
            if upload(channel.asset_id, animation.frame(index)) {
                self.uploaded.insert(channel.asset_id, index);
            }
        }
    }
}

/// Rewrites the textures of animated BGA images for `chart_now` (the same clock as video BGA).
pub fn update_animated_bga_frames(
    renderer: &mut bmz_render::renderer::Renderer,
    running: &mut RunningPlaySession,
    chart_now: TimeUs,
) {
    if running.animated_bga.catalog.is_empty()
        || !running.session.bga_enabled
        || !running.session.chart.metadata.has_bga
    {
        return;
    }
    let RunningPlaySession { gameplay, animated_bga, .. } = running;
    let mut channels = active_bga_channels(&gameplay.session, chart_now);
    // POOR covers the other layers while shown, so it decides a shared asset's frame.
    channels.sort_by_key(|channel| channel.kind != BgaEventKind::Poor);
    animated_bga.update(&channels, chart_now, |asset_id, frame| {
        let texture_id = TextureId(bga_texture_id(asset_id));
        match renderer.upsert_rgba_texture_ref(texture_id, frame.width, frame.height, &frame.pixels)
        {
            Ok(()) => true,
            Err(error) => {
                tracing::warn!(asset_id = asset_id.0, %error, "failed to upload animated BGA frame");
                false
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    /// Writes a looping GIF of `count` 16x16 frames, 1 MiB per 4 frames once padded.
    fn write_small_gif(dir: &Path, name: &str, count: usize) -> std::path::PathBuf {
        let path = dir.join(name);
        let mut encoder =
            image::codecs::gif::GifEncoder::new(std::fs::File::create(&path).unwrap());
        encoder.set_repeat(image::codecs::gif::Repeat::Infinite).unwrap();
        for index in 0..count {
            let color = if index % 2 == 0 { [255, 0, 0, 255] } else { [0, 0, 255, 255] };
            encoder
                .encode_frame(image::Frame::from_parts(
                    image::RgbaImage::from_pixel(16, 16, image::Rgba(color)),
                    0,
                    0,
                    image::Delay::from_numer_denom_ms(100, 1),
                ))
                .unwrap();
        }
        path
    }

    #[test]
    fn chart_budget_counts_padded_frames_and_falls_back_once_spent() {
        let dir = tempfile::tempdir().unwrap();
        let paths: Vec<_> = (0..3)
            .map(|index| write_small_gif(dir.path(), &format!("bga{index}.gif"), 4))
            .collect();
        // Each animation holds 4 x 256 KiB padded frames; the budget fits two and a half.
        let mut budget = AnimatedBgaBudget::with_total(5 * 512 * 1024);

        let loaded: Vec<_> = paths.iter().map(|path| budget.load(path).unwrap()).collect();

        for decoded in &loaded[..2] {
            let animation = decoded.animation().expect("within the chart budget");
            assert_eq!(animation.frames().len(), 4);
            assert_eq!(animation.rgba_bytes(), 1024 * 1024);
        }
        assert!(loaded[2].animation().is_none(), "the third animation exceeds the budget");
        let first = loaded[2].first_frame();
        assert_eq!((first.width, first.height), (256, 256));
        assert_eq!(&first.pixels[120 * 4..121 * 4], &[255, 0, 0, 255]);
        assert_eq!(budget.remaining, 512 * 1024);
    }

    #[test]
    fn playback_and_video_export_share_frames_and_timing() {
        let dir = tempfile::tempdir().unwrap();
        let path = write_small_gif(dir.path(), "bga.gif", 3);

        let play = AnimatedBgaBudget::default().load(&path).unwrap();
        let export = AnimatedBgaBudget::default().load_unpadded(&path).unwrap();

        let (play, export) = (play.animation().unwrap(), export.animation().unwrap());
        assert_eq!((play.width(), export.width()), (256, 16));
        assert_eq!(play.frames().len(), export.frames().len());
        assert_eq!(play.first_cycle_us(), export.first_cycle_us());
        for elapsed_us in [0, 99_999, 100_000, 250_000, 1_000_000] {
            let index = play.frame_index(elapsed_us);
            assert_eq!(index, export.frame_index(elapsed_us));
            assert_eq!(
                &play.frame(index).pixels[120 * 4..121 * 4],
                &export.frame(index).pixels[..4]
            );
        }
    }

    fn animation(colors: &[u8]) -> Arc<AnimatedImage> {
        let frames = colors
            .iter()
            .map(|color| {
                let frame = RgbaImageAsset { width: 1, height: 1, pixels: vec![*color; 4] };
                (frame, Duration::from_millis(100))
            })
            .collect();
        Arc::new(AnimatedImage::new(frames, None).unwrap())
    }

    fn channel(kind: BgaEventKind, asset: u32, origin_ms: i64) -> ActiveBgaChannel {
        ActiveBgaChannel { kind, asset_id: BgaAssetId(asset), origin: TimeUs(origin_ms * 1000) }
    }

    fn run(state: &mut AnimatedBgaState, channels: &[ActiveBgaChannel], now_ms: i64) -> Vec<u8> {
        let mut uploads = Vec::new();
        state.update(channels, TimeUs(now_ms * 1000), |_, frame| {
            uploads.push(frame.pixels[0]);
            true
        });
        uploads
    }

    #[test]
    fn uploads_only_when_the_due_frame_changes() {
        let mut state = AnimatedBgaState::default();
        state.insert_uploaded_first(BgaAssetId(1), animation(&[10, 20, 30]));
        let base = [channel(BgaEventKind::Base, 1, 1_000)];

        assert!(run(&mut state, &base, 1_050).is_empty());
        assert_eq!(run(&mut state, &base, 1_100), [20]);
        assert!(run(&mut state, &base, 1_199).is_empty());
        assert_eq!(run(&mut state, &base, 1_200), [30]);
        // Infinite loop wraps to the first frame.
        assert_eq!(run(&mut state, &base, 1_300), [10]);
    }

    #[test]
    fn unknown_texture_contents_upload_once_and_failed_uploads_retry() {
        let mut state = AnimatedBgaState::from_catalog(BgaAnimationCatalog::from([(
            BgaAssetId(1),
            animation(&[10, 20]),
        )]));
        let base = [channel(BgaEventKind::Base, 1, 0)];

        let mut attempts = 0;
        state.update(&base, TimeUs(0), |_, _| {
            attempts += 1;
            false
        });
        assert_eq!(attempts, 1);
        assert_eq!(run(&mut state, &base, 0), [10]);
        assert!(run(&mut state, &base, 0).is_empty());
    }

    #[test]
    fn first_channel_decides_a_shared_asset_and_still_images_are_ignored() {
        let mut state = AnimatedBgaState::default();
        state.insert_uploaded_first(BgaAssetId(1), animation(&[10, 20, 30]));
        let channels = [
            channel(BgaEventKind::Poor, 1, 1_000),
            channel(BgaEventKind::Base, 1, 0),
            channel(BgaEventKind::Layer, 2, 0),
        ];

        // At 1.05 s Base alone would show frame 1, but POOR (started at 1.0 s) shows frame 0.
        assert!(run(&mut state, &channels, 1_050).is_empty());
        assert_eq!(run(&mut state, &channels, 1_100), [20]);
    }
}

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

/// Texture a channel shows an animated asset through when another channel shows the same
/// asset from a different start. Base / Layer / Layer2 / POOR use consecutive IDs.
pub const ANIMATED_BGA_CHANNEL_TEXTURE_BASE: u32 = bmz_render::plan::CHART_BGA_TEXTURE_BASE - 10;

pub fn channel_texture_id(kind: BgaEventKind) -> u32 {
    let offset = match kind {
        BgaEventKind::Base => 0,
        BgaEventKind::Layer => 1,
        BgaEventKind::Layer2 => 2,
        BgaEventKind::Poor => 3,
    };
    ANIMATED_BGA_CHANNEL_TEXTURE_BASE + offset
}

/// Every channel texture ID, for releasing them when a new play starts.
pub fn channel_texture_ids() -> impl Iterator<Item = u32> {
    [BgaEventKind::Base, BgaEventKind::Layer, BgaEventKind::Layer2, BgaEventKind::Poor]
        .into_iter()
        .map(channel_texture_id)
}

/// A channel whose animated asset is drawn from its own channel texture this frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChannelTexture {
    pub kind: BgaEventKind,
    pub asset_id: BgaAssetId,
    pub texture_id: u32,
}

#[derive(Debug, Default, Clone)]
pub struct AnimatedBgaState {
    pub catalog: BgaAnimationCatalog,
    /// `(asset, frame)` currently in each texture this state writes. Missing means unknown.
    uploaded: HashMap<u32, (BgaAssetId, usize)>,
    /// Channel textures holding a frame, released once their channel no longer needs them.
    channel_textures: Vec<u32>,
}

impl AnimatedBgaState {
    /// Textures may hold any frame (for example after a retry), so the first update uploads.
    pub fn from_catalog(catalog: BgaAnimationCatalog) -> Self {
        Self { catalog, ..Self::default() }
    }

    /// Registers an animation whose texture was just created from its first frame.
    pub fn insert_uploaded_first(&mut self, asset_id: BgaAssetId, animation: Arc<AnimatedImage>) {
        self.catalog.insert(asset_id, animation);
        self.uploaded.insert(bga_texture_id(asset_id), (asset_id, 0));
    }

    /// Uploads the frame each channel shows at `chart_now` and returns the channels drawn
    /// from their own texture. `channels` are in priority order: the first channel showing an
    /// asset uses the asset's texture, a later one with the same start shares it, and a later
    /// one with another start gets its channel texture. Channel textures no longer needed are
    /// passed to `release`.
    pub fn update(
        &mut self,
        channels: &[ActiveBgaChannel],
        chart_now: TimeUs,
        mut upload: impl FnMut(u32, &RgbaImageAsset) -> bool,
        mut release: impl FnMut(u32),
    ) -> Vec<ChannelTexture> {
        let mut owners: Vec<(BgaAssetId, TimeUs)> = Vec::with_capacity(channels.len());
        let mut separate = Vec::new();
        let mut used_channel_textures = Vec::new();
        for channel in channels {
            let Some(animation) = self.catalog.get(&channel.asset_id) else {
                continue;
            };
            let texture_id = match owners.iter().find(|(asset, _)| *asset == channel.asset_id) {
                None => {
                    owners.push((channel.asset_id, channel.origin));
                    bga_texture_id(channel.asset_id)
                }
                Some((_, origin)) if *origin == channel.origin => continue,
                Some(_) => {
                    let texture_id = channel_texture_id(channel.kind);
                    used_channel_textures.push(texture_id);
                    separate.push(ChannelTexture {
                        kind: channel.kind,
                        asset_id: channel.asset_id,
                        texture_id,
                    });
                    texture_id
                }
            };
            let elapsed_us =
                u64::try_from(chart_now.0.saturating_sub(channel.origin.0)).unwrap_or(0);
            let index = animation.frame_index(elapsed_us);
            if self.uploaded.get(&texture_id) != Some(&(channel.asset_id, index))
                && upload(texture_id, animation.frame(index))
            {
                self.uploaded.insert(texture_id, (channel.asset_id, index));
            }
        }
        for texture_id in std::mem::replace(&mut self.channel_textures, used_channel_textures) {
            if !self.channel_textures.contains(&texture_id) {
                self.uploaded.remove(&texture_id);
                release(texture_id);
            }
        }
        separate
    }
}

/// Points snapshot BGA slots at their channel textures. A slot showing another asset (such
/// as a #SWBGA key layer) keeps its own texture.
pub fn apply_channel_textures(
    snapshot: &mut bmz_render::snapshot::RenderSnapshot,
    channel_textures: &[ChannelTexture],
) {
    for channel in channel_textures {
        let slot = match channel.kind {
            BgaEventKind::Base => &mut snapshot.bga_base,
            BgaEventKind::Layer => &mut snapshot.bga_layer,
            BgaEventKind::Layer2 => &mut snapshot.bga_layer2,
            BgaEventKind::Poor => &mut snapshot.bga_poor,
        };
        if let Some(frame) = slot
            && frame.texture_id == bga_texture_id(channel.asset_id)
        {
            frame.texture_id = channel.texture_id;
        }
    }
}

/// Channels in priority order: POOR covers the other layers while shown, so it owns the
/// asset texture of an asset it shares.
pub fn prioritized(mut channels: Vec<ActiveBgaChannel>) -> Vec<ActiveBgaChannel> {
    channels.sort_by_key(|channel| channel.kind != BgaEventKind::Poor);
    channels
}

/// Rewrites the textures of animated BGA images for `chart_now` (the same clock as video BGA)
/// and returns the channels to redirect with [`apply_channel_textures`].
pub fn update_animated_bga_frames(
    renderer: &mut bmz_render::renderer::Renderer,
    running: &mut RunningPlaySession,
    chart_now: TimeUs,
) -> Vec<ChannelTexture> {
    if running.animated_bga.catalog.is_empty()
        || !running.session.bga_enabled
        || !running.session.chart.metadata.has_bga
    {
        return Vec::new();
    }
    let RunningPlaySession { gameplay, animated_bga, .. } = running;
    let channels = prioritized(active_bga_channels(&gameplay.session, chart_now));
    let mut released = Vec::new();
    let separate = animated_bga.update(
        &channels,
        chart_now,
        |texture_id, frame| match renderer.upsert_rgba_texture_ref(
            TextureId(texture_id),
            frame.width,
            frame.height,
            &frame.pixels,
        ) {
            Ok(()) => true,
            Err(error) => {
                tracing::warn!(texture_id, %error, "failed to upload animated BGA frame");
                false
            }
        },
        |texture_id| released.push(texture_id),
    );
    for texture_id in released {
        renderer.remove_image_texture(TextureId(texture_id));
    }
    separate
}

/// Frees channel textures left by a previous play.
pub fn release_channel_textures(renderer: &mut bmz_render::renderer::Renderer) {
    for texture_id in channel_texture_ids() {
        renderer.remove_image_texture(TextureId(texture_id));
    }
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

    struct Update {
        /// `(texture, first byte of the uploaded frame)`.
        uploads: Vec<(u32, u8)>,
        released: Vec<u32>,
        separate: Vec<ChannelTexture>,
    }

    fn run(state: &mut AnimatedBgaState, channels: &[ActiveBgaChannel], now_ms: i64) -> Update {
        let mut uploads = Vec::new();
        let mut released = Vec::new();
        let separate = state.update(
            &prioritized(channels.to_vec()),
            TimeUs(now_ms * 1000),
            |texture, frame| {
                uploads.push((texture, frame.pixels[0]));
                true
            },
            |texture| released.push(texture),
        );
        Update { uploads, released, separate }
    }

    const ASSET_1: u32 = bmz_render::plan::CHART_BGA_TEXTURE_BASE + 1;
    const ASSET_2: u32 = bmz_render::plan::CHART_BGA_TEXTURE_BASE + 2;

    fn state_with(assets: &[u32]) -> AnimatedBgaState {
        let mut state = AnimatedBgaState::default();
        for asset in assets {
            state.insert_uploaded_first(BgaAssetId(*asset), animation(&[10, 20, 30]));
        }
        state
    }

    #[test]
    fn uploads_only_when_the_due_frame_changes() {
        let mut state = state_with(&[1]);
        let base = [channel(BgaEventKind::Base, 1, 1_000)];

        assert!(run(&mut state, &base, 1_050).uploads.is_empty());
        assert_eq!(run(&mut state, &base, 1_100).uploads, [(ASSET_1, 20)]);
        assert!(run(&mut state, &base, 1_199).uploads.is_empty());
        assert_eq!(run(&mut state, &base, 1_200).uploads, [(ASSET_1, 30)]);
        // Infinite loop wraps to the first frame.
        assert_eq!(run(&mut state, &base, 1_300).uploads, [(ASSET_1, 10)]);
    }

    #[test]
    fn unknown_texture_contents_upload_once_and_failed_uploads_retry() {
        let mut state = AnimatedBgaState::from_catalog(BgaAnimationCatalog::from([(
            BgaAssetId(1),
            animation(&[10, 20]),
        )]));
        let base = [channel(BgaEventKind::Base, 1, 0)];

        let mut attempts = 0;
        state.update(
            &base,
            TimeUs(0),
            |_, _| {
                attempts += 1;
                false
            },
            |_| {},
        );
        assert_eq!(attempts, 1);
        assert_eq!(run(&mut state, &base, 0).uploads, [(ASSET_1, 10)]);
        assert!(run(&mut state, &base, 0).uploads.is_empty());
    }

    #[test]
    fn same_asset_with_different_starts_plays_independently_per_channel() {
        let mut state = state_with(&[1]);
        let channels = [channel(BgaEventKind::Base, 1, 0), channel(BgaEventKind::Layer, 1, 1_050)];

        let update = run(&mut state, &channels, 1_150);

        // Base: 1150 ms -> frame 2 in the asset texture; Layer: 100 ms -> frame 1 separately.
        let layer_texture = channel_texture_id(BgaEventKind::Layer);
        assert_eq!(update.uploads, [(ASSET_1, 30), (layer_texture, 20)]);
        assert_eq!(
            update.separate,
            [ChannelTexture {
                kind: BgaEventKind::Layer,
                asset_id: BgaAssetId(1),
                texture_id: layer_texture
            }]
        );
        // Each texture only changes when its own frame does.
        assert_eq!(run(&mut state, &channels, 1_200).uploads, [(ASSET_1, 10)]);
        assert_eq!(run(&mut state, &channels, 1_250).uploads, [(layer_texture, 30)]);
    }

    #[test]
    fn same_asset_with_the_same_start_shares_one_texture() {
        let mut state = state_with(&[1]);
        let channels = [
            channel(BgaEventKind::Base, 1, 500),
            channel(BgaEventKind::Layer, 1, 500),
            channel(BgaEventKind::Layer2, 1, 500),
        ];

        let update = run(&mut state, &channels, 600);

        assert_eq!(update.uploads, [(ASSET_1, 20)]);
        assert!(update.separate.is_empty());
        assert!(update.released.is_empty());
    }

    #[test]
    fn poor_owns_the_asset_texture_while_shown_and_releases_the_channel_texture_after() {
        let mut state = state_with(&[1]);
        let base_texture = channel_texture_id(BgaEventKind::Base);
        let with_poor = [channel(BgaEventKind::Base, 1, 0), channel(BgaEventKind::Poor, 1, 1_000)];

        // POOR started at 1.0 s shows frame 0; Base (1.15 s in) shows frame 2 on its own.
        let update = run(&mut state, &with_poor, 1_150);
        assert_eq!(update.uploads, [(ASSET_1, 20), (base_texture, 30)]);
        assert_eq!(update.separate[0].kind, BgaEventKind::Base);
        assert!(run(&mut state, &with_poor, 1_150).uploads.is_empty());

        // POOR ended: Base takes the asset texture back and its channel texture is freed.
        let update = run(&mut state, &[channel(BgaEventKind::Base, 1, 0)], 1_250);
        assert_eq!(update.uploads, [(ASSET_1, 10)]);
        assert!(update.separate.is_empty());
        assert_eq!(update.released, [base_texture]);
        assert!(run(&mut state, &[channel(BgaEventKind::Base, 1, 0)], 1_250).released.is_empty());
    }

    #[test]
    fn switching_assets_and_back_restarts_from_the_new_event() {
        let mut state = state_with(&[1, 2]);

        assert_eq!(
            run(&mut state, &[channel(BgaEventKind::Base, 1, 0)], 150).uploads,
            [(ASSET_1, 20)]
        );
        assert_eq!(
            run(&mut state, &[channel(BgaEventKind::Base, 2, 200)], 350).uploads,
            [(ASSET_2, 20)]
        );
        // Back to asset 1 at 0.5 s: its texture still holds frame 1, the new event needs 0.
        assert_eq!(
            run(&mut state, &[channel(BgaEventKind::Base, 1, 500)], 520).uploads,
            [(ASSET_1, 10)]
        );
    }

    #[test]
    fn seek_and_retry_resolve_frames_from_chart_time_alone() {
        let channels = [channel(BgaEventKind::Base, 1, 0), channel(BgaEventKind::Layer, 1, 1_050)];
        let mut live = state_with(&[1]);
        run(&mut live, &channels, 1_150);
        let layer_texture = channel_texture_id(BgaEventKind::Layer);

        // A seek back before the Layer event drops its channel texture.
        let update = run(&mut live, &[channel(BgaEventKind::Base, 1, 0)], 150);
        assert_eq!(update.uploads, [(ASSET_1, 20)]);
        assert_eq!(update.released, [layer_texture]);

        // A retry starts from unknown texture contents and uploads what is due.
        let mut retry = AnimatedBgaState::from_catalog(live.catalog.clone());
        assert_eq!(run(&mut retry, &channels, 1_150).uploads, [(ASSET_1, 30), (layer_texture, 20)]);
    }

    #[test]
    fn apply_channel_textures_redirects_only_slots_showing_the_asset() {
        use crate::screens::play_snapshot::display_bga_frame;
        let mut snapshot = bmz_render::snapshot::RenderSnapshot {
            bga_base: Some(display_bga_frame(BgaAssetId(1), 256, 256)),
            bga_layer: Some(display_bga_frame(BgaAssetId(1), 256, 256)),
            bga_layer2: Some(display_bga_frame(BgaAssetId(2), 256, 256)),
            ..Default::default()
        };
        let separate = [
            ChannelTexture {
                kind: BgaEventKind::Layer,
                asset_id: BgaAssetId(1),
                texture_id: channel_texture_id(BgaEventKind::Layer),
            },
            ChannelTexture {
                kind: BgaEventKind::Layer2,
                asset_id: BgaAssetId(1),
                texture_id: channel_texture_id(BgaEventKind::Layer2),
            },
        ];

        apply_channel_textures(&mut snapshot, &separate);

        assert_eq!(snapshot.bga_base.unwrap().texture_id, ASSET_1);
        assert_eq!(snapshot.bga_layer.unwrap().texture_id, channel_texture_id(BgaEventKind::Layer));
        assert_eq!(snapshot.bga_layer2.unwrap().texture_id, ASSET_2);
        assert_eq!(snapshot.bga_layer.unwrap().width, 256.0);
    }
}

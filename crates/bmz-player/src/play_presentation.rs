//! Play STAGEFILE presentation shared by live play and deterministic video export.
//!
//! An animated STAGEFILE plays in Play's STAGEFILE slot (runtime source `100`) through its
//! own texture, so Select / Decide / Result keep their STAGEFILE texture. Loading waits for
//! the animation's first cycle, READY restarts it and waits for the longer of the skin's
//! `playstart` and that cycle, and later cycles follow the file's loop count. The two slots
//! leave room for Kaleid `LOADINGFILE` / `READYFILE`; both hold the STAGEFILE for now.

use std::sync::Arc;

use bmz_render::assets::AnimatedImage;
use bmz_render::plan::PLAY_PRESENTATION_TEXTURE;
use bmz_render::renderer::Renderer;
use bmz_render::skin::{SkinBgaFrame, SkinImageSize, SkinTextureId};

#[derive(Default, Clone)]
pub(crate) struct PresentationImages {
    pub loading: Option<Arc<AnimatedImage>>,
    pub ready: Option<Arc<AnimatedImage>>,
}

impl PresentationImages {
    /// Uses an animated STAGEFILE for both Loading and READY. Still images add no waits.
    pub fn from_stagefile(animation: Option<Arc<AnimatedImage>>) -> Self {
        Self { loading: animation.clone(), ready: animation }
    }

    pub fn loading_us(&self) -> i64 {
        cycle_us(&self.loading)
    }

    pub fn ready_us(&self) -> i64 {
        cycle_us(&self.ready)
    }
}

fn cycle_us(image: &Option<Arc<AnimatedImage>>) -> i64 {
    image.as_ref().map_or(0, |image| i64::try_from(image.first_cycle_us()).unwrap_or(i64::MAX))
}

/// READY lasts the longer of the skin's `playstart` and the READY animation's first cycle.
pub(crate) fn ready_wait_us(skin_us: i64, cycle_us: i64) -> i64 {
    skin_us.max(0).max(cycle_us)
}

#[derive(Default)]
pub(crate) struct PresentationPlayback {
    pub images: PresentationImages,
    loading_origin: Option<i64>,
    ready_origin: Option<i64>,
    /// `(ready, frame index)` currently in the presentation texture.
    uploaded: Option<(bool, usize)>,
}

impl PresentationPlayback {
    pub fn new(images: PresentationImages) -> Self {
        Self { images, ..Self::default() }
    }

    pub fn reset(&mut self) {
        self.loading_origin = None;
        self.ready_origin = None;
        self.uploaded = None;
    }

    /// Loading's first cycle has played since it was first drawn.
    pub fn loading_complete(&self, now_us: i64) -> bool {
        let duration = self.images.loading_us();
        duration == 0
            || self.loading_origin.is_some_and(|start| now_us.saturating_sub(start) >= duration)
    }

    pub fn ready_complete(&self, now_us: i64) -> bool {
        let duration = self.images.ready_us();
        duration == 0
            || self.ready_origin.is_some_and(|start| now_us.saturating_sub(start) >= duration)
    }

    /// Uploads the due frame and returns the runtime source `100` replacement. The first
    /// draw of each state starts its clock.
    pub fn draw(
        &mut self,
        renderer: &mut Renderer,
        now_us: i64,
        ready: bool,
    ) -> Option<SkinBgaFrame> {
        let image = if ready { self.images.ready.as_ref() } else { self.images.loading.as_ref() }?;
        let origin = if ready { &mut self.ready_origin } else { &mut self.loading_origin };
        let elapsed_us =
            u64::try_from(now_us.saturating_sub(*origin.get_or_insert(now_us))).unwrap_or_default();
        let index = image.frame_index(elapsed_us);
        let frame = image.frame(index);
        let size = SkinImageSize { width: frame.width as f32, height: frame.height as f32 };
        if self.uploaded != Some((ready, index)) {
            if let Err(error) = renderer.upsert_rgba_texture_ref(
                PLAY_PRESENTATION_TEXTURE,
                frame.width,
                frame.height,
                &frame.pixels,
            ) {
                tracing::warn!(%error, "STAGEFILE animation upload failed; using its still image");
                if ready {
                    self.images.ready = None;
                } else {
                    self.images.loading = None;
                }
                self.uploaded = None;
                return None;
            }
            self.uploaded = Some((ready, index));
        }
        Some(SkinBgaFrame::opaque(SkinTextureId(PLAY_PRESENTATION_TEXTURE.0), size))
    }

    /// Offline media is available at scene entry, with a known READY origin.
    pub fn set_offline_origins(&mut self, ready_us: i64) {
        self.loading_origin = Some(0);
        self.ready_origin = Some(ready_us);
    }

    /// A Viewer seek or reload joins the existing Play clock instead of replaying the intro.
    pub fn join_existing_scene(&mut self) {
        self.loading_origin.get_or_insert(0);
        self.ready_origin.get_or_insert(0);
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use bmz_render::assets::RgbaImageAsset;

    use super::*;

    fn animation(delay_ms: u64) -> Arc<AnimatedImage> {
        let frames = [[255, 0, 0, 255], [0, 0, 255, 255]]
            .map(|color| {
                let frame = RgbaImageAsset { width: 2, height: 1, pixels: color.repeat(2) };
                (frame, Duration::from_millis(delay_ms))
            })
            .into();
        Arc::new(AnimatedImage::new(frames, None).unwrap())
    }

    #[test]
    fn ready_uses_longer_duration_instead_of_adding() {
        assert_eq!(ready_wait_us(1_000_000, 2_000_000), 2_000_000);
        assert_eq!(ready_wait_us(3_000_000, 2_000_000), 3_000_000);
        assert_eq!(ready_wait_us(0, 2_000_000), 2_000_000);
        assert_eq!(ready_wait_us(-1, 0), 0);
    }

    #[test]
    fn still_stagefile_adds_no_waits() {
        let images = PresentationImages::from_stagefile(None);
        assert_eq!(images.loading_us(), 0);
        assert_eq!(images.ready_us(), 0);
        let mut playback = PresentationPlayback::new(images);
        assert!(playback.loading_complete(0));
        assert!(playback.ready_complete(0));
        assert!(playback.draw(&mut Renderer::default(), 0, false).is_none());
    }

    #[test]
    fn presentation_waits_from_first_display_and_resets_on_retry() {
        let images = PresentationImages::from_stagefile(Some(animation(50)));
        assert!(Arc::ptr_eq(images.loading.as_ref().unwrap(), images.ready.as_ref().unwrap()));
        let mut playback = PresentationPlayback::new(images);
        let mut renderer = Renderer::default();

        assert!(!playback.loading_complete(1_000_000));
        let frame = playback.draw(&mut renderer, 1_000_000, false).unwrap();
        assert_eq!(frame.texture, SkinTextureId(PLAY_PRESENTATION_TEXTURE.0));
        assert_eq!(frame.source_size.width, 2.0);
        assert!(!playback.loading_complete(1_099_999));
        assert!(playback.loading_complete(1_100_000));
        playback.draw(&mut renderer, 1_170_000, false);
        assert_eq!(playback.uploaded, Some((false, 1)));

        // READY restarts the animation from its first frame.
        playback.draw(&mut renderer, 1_170_000, true);
        assert_eq!(playback.uploaded, Some((true, 0)));
        assert!(!playback.ready_complete(1_269_999));
        assert!(playback.ready_complete(1_270_000));
        playback.draw(&mut renderer, 1_270_000, true);
        assert_eq!(playback.uploaded, Some((true, 0)));

        playback.reset();
        assert!(!playback.loading_complete(10_000_000));
        playback.draw(&mut renderer, 10_000_000, false);
        assert!(!playback.loading_complete(10_050_000));
        assert!(playback.loading_complete(10_100_000));
    }

    #[test]
    fn seamless_entry_joins_the_play_clock_without_waiting() {
        let mut playback =
            PresentationPlayback::new(PresentationImages::from_stagefile(Some(animation(50))));
        playback.join_existing_scene();
        assert!(playback.loading_complete(100_000));
        assert!(playback.ready_complete(100_000));
        playback.draw(&mut Renderer::default(), 60_000, true);
        assert_eq!(playback.uploaded, Some((true, 1)));
    }
}

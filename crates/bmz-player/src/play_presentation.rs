//! Shared loading/READY presentation for live play and deterministic video export.
use bmz_render::{
    assets::presentation::{PresentationImage, load_presentation_image},
    plan::PLAY_PRESENTATION_TEXTURE,
    renderer::Renderer,
    skin::{SkinBgaFrame, SkinImageSize, SkinTextureId},
};
use std::{path::Path, sync::Arc};

#[derive(Default, Clone)]
pub(crate) struct PresentationImages {
    pub loading: Option<Arc<PresentationImage>>,
    pub ready: Option<Arc<PresentationImage>>,
}

impl PresentationImages {
    pub fn load(folder: &Path, loading: &str, ready: &str) -> Self {
        let load = |name: &str| {
            if name.trim().is_empty() {
                return None;
            }
            let normalized = name.replace('\\', "/");
            let result = crate::chart_asset::resolve_chart_asset_path(
                &folder.to_string_lossy(),
                &normalized,
            )
            .ok_or_else(|| anyhow::anyhow!("presentation image not found: {name}"))
            .and_then(|path| load_presentation_image(&path));
            match result {
                Ok(image) => Some(Arc::new(image)),
                Err(error) => {
                    tracing::warn!(%error, "using STAGEFILE instead of chart presentation");
                    None
                }
            }
        };
        let loading_image = load(loading);
        let ready_image = if loading == ready { loading_image.clone() } else { load(ready) };
        Self { loading: loading_image, ready: ready_image }
    }

    pub fn loading_us(&self) -> i64 {
        cycle_us(&self.loading)
    }
    pub fn ready_us(&self) -> i64 {
        cycle_us(&self.ready)
    }
}

fn cycle_us(image: &Option<Arc<PresentationImage>>) -> i64 {
    image.as_ref().map_or(0, |image| image.first_cycle_us().min(i64::MAX as u64) as i64)
}

pub(crate) fn ready_wait_us(skin_us: i64, gif_us: i64) -> i64 {
    skin_us.max(0).max(gif_us)
}

#[derive(Default)]
pub(crate) struct PresentationPlayback {
    pub images: PresentationImages,
    loading_origin: Option<i64>,
    ready_origin: Option<i64>,
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

    pub fn draw(
        &mut self,
        renderer: &mut Renderer,
        now_us: i64,
        ready: bool,
    ) -> Option<SkinBgaFrame> {
        let image = if ready { self.images.ready.as_ref() } else { self.images.loading.as_ref() }?;
        let origin = if ready { &mut self.ready_origin } else { &mut self.loading_origin };
        let elapsed = now_us.saturating_sub(*origin.get_or_insert(now_us)).max(0) as u64;
        let index = image.frame_index(elapsed);
        let frame = &image.frames[index];
        let size = SkinImageSize { width: frame.width as f32, height: frame.height as f32 };
        if self.uploaded != Some((ready, index)) {
            if let Err(error) = renderer.upsert_rgba_texture_ref(
                PLAY_PRESENTATION_TEXTURE,
                frame.width,
                frame.height,
                &frame.pixels,
            ) {
                tracing::warn!(%error, "presentation upload failed; using STAGEFILE");
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

    pub fn join_existing_scene(&mut self) {
        self.loading_origin.get_or_insert(0);
        self.ready_origin.get_or_insert(0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ready_uses_longer_duration_instead_of_adding() {
        assert_eq!(ready_wait_us(1_000_000, 2_000_000), 2_000_000);
        assert_eq!(ready_wait_us(3_000_000, 2_000_000), 3_000_000);
        assert_eq!(ready_wait_us(0, 2_000_000), 2_000_000);
        assert_eq!(ready_wait_us(-1, 0), 0);
    }
    #[test]
    fn missing_files_do_not_add_waits() {
        let images = PresentationImages::load(Path::new("/nonexistent"), "absent.gif", "");
        assert_eq!(images.loading_us(), 0);
        assert_eq!(images.ready_us(), 0);
        assert!(PresentationPlayback::new(images).loading_complete(0));
    }

    #[test]
    fn presentation_waits_from_first_display_and_resets_on_retry() {
        let stamp =
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let folder =
            std::env::temp_dir().join(format!("bmz-presentation-{}-{stamp}", std::process::id()));
        std::fs::create_dir_all(folder.join("画像")).unwrap();
        let path = folder.join("画像/演出 test.gif");
        {
            let mut encoder =
                image::codecs::gif::GifEncoder::new(std::fs::File::create(&path).unwrap());
            encoder.set_repeat(image::codecs::gif::Repeat::Infinite).unwrap();
            for color in [[255, 0, 0, 255], [0, 0, 255, 255]] {
                encoder
                    .encode_frame(image::Frame::from_parts(
                        image::RgbaImage::from_pixel(2, 1, image::Rgba(color)),
                        0,
                        0,
                        image::Delay::from_numer_denom_ms(50, 1),
                    ))
                    .unwrap();
            }
        }
        let images =
            PresentationImages::load(&folder, "画像\\演出 test.gif", "画像\\演出 test.gif");
        assert!(Arc::ptr_eq(images.loading.as_ref().unwrap(), images.ready.as_ref().unwrap()));
        let mut playback = PresentationPlayback::new(images);
        let mut renderer = Renderer::default();
        assert!(!playback.loading_complete(1_000_000));
        let frame = playback.draw(&mut renderer, 1_000_000, false).unwrap();
        assert_eq!(frame.source_size.width, 2.0);
        assert!(!playback.loading_complete(1_099_999));
        assert!(playback.loading_complete(1_100_000));
        playback.draw(&mut renderer, 1_170_000, false);
        assert_eq!(playback.uploaded, Some((false, 1)));
        playback.draw(&mut renderer, 1_170_000, true);
        assert_eq!(playback.uploaded, Some((true, 0)));
        playback.draw(&mut renderer, 1_270_000, true);
        assert_eq!(playback.uploaded, Some((true, 0)));
        playback.reset();
        assert!(!playback.loading_complete(10_000_000));
        playback.draw(&mut renderer, 10_000_000, false);
        assert!(!playback.loading_complete(10_050_000));
        let fallback = PresentationImages::load(&folder, "missing.gif", "画像/演出 test.gif");
        assert_eq!(fallback.loading_us(), 0);
        assert_eq!(fallback.ready_us(), 100_000);
        std::fs::remove_dir_all(folder).unwrap();
    }
}

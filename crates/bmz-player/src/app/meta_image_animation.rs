//! STAGEFILE / BANNER / BACKBMP animation (BMZ extension).
//!
//! The scenes that upload chart meta images share textures 20..22. Each texture remembers
//! the key of its last writer so a stale animation never overwrites another chart's image.
//! Animated images play from the first frame they become visible, with no waits; the Play
//! STAGEFILE wait is handled separately by the Play presentation.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::thread;
use std::time::Instant;

use bmz_render::assets::{AnimatedImage, AnimatedImageLimits, RgbaImageAsset, load_chart_image};
use bmz_render::plan::TextureId;

use super::WinitApp;
use super::pending_state::AppSceneKind;
use super::skin_video::skin_video_texture_visible_in_plan;

/// Decoded meta image animations kept per image (Select keeps several in its cache).
pub(super) const META_IMAGE_ANIMATION_LIMITS: AnimatedImageLimits =
    AnimatedImageLimits { max_rgba_bytes: 64 * 1024 * 1024, ..AnimatedImageLimits::CHART };

#[derive(Default)]
pub(super) struct MetaImageAnimations {
    /// Key of the image last written to each texture, animated or not.
    keys: HashMap<TextureId, String>,
    slots: HashMap<TextureId, AnimationSlot>,
    pending: HashMap<TextureId, Receiver<Option<Arc<AnimatedImage>>>>,
    scene: Option<AppSceneKind>,
}

struct AnimationSlot {
    image: Arc<AnimatedImage>,
    origin: Option<Instant>,
    /// Frame currently in the texture. `None` uploads on the next visible frame.
    uploaded: Option<usize>,
}

impl MetaImageAnimations {
    pub(super) fn key(&self, texture: TextureId) -> Option<&str> {
        self.keys.get(&texture).map(String::as_str)
    }

    /// Records that `key` was just uploaded to `texture`, with its animation if any.
    pub(super) fn replace(
        &mut self,
        texture: TextureId,
        key: &str,
        animation: Option<Arc<AnimatedImage>>,
    ) {
        self.keys.insert(texture, key.to_string());
        self.pending.remove(&texture);
        match animation {
            Some(image) => {
                self.slots.insert(texture, AnimationSlot { image, origin: None, uploaded: None });
            }
            None => {
                self.slots.remove(&texture);
            }
        }
    }

    /// Decodes the animation of the still image just written as `key` in the background.
    pub(super) fn request(&mut self, texture: TextureId, key: &str, path: PathBuf) {
        self.replace(texture, key, None);
        let (tx, rx) = mpsc::channel();
        let spawned = thread::Builder::new().name("chart-meta-animation".to_string()).spawn(
            move || {
                let animation = match load_chart_image(&path, &META_IMAGE_ANIMATION_LIMITS) {
                    Ok(decoded) => decoded.animation().cloned(),
                    Err(error) => {
                        tracing::debug!(path = %path.display(), %error, "skipping meta image animation");
                        None
                    }
                };
                let _ = tx.send(animation);
            },
        );
        match spawned {
            Ok(_) => {
                self.pending.insert(texture, rx);
            }
            Err(error) => tracing::warn!(%error, "cannot start meta image animation decode"),
        }
    }

    /// Installs finished background decodes.
    pub(super) fn poll(&mut self) {
        let mut finished = Vec::new();
        for (texture, rx) in &self.pending {
            match rx.try_recv() {
                Ok(animation) => finished.push((*texture, animation)),
                Err(TryRecvError::Disconnected) => finished.push((*texture, None)),
                Err(TryRecvError::Empty) => {}
            }
        }
        for (texture, animation) in finished {
            self.pending.remove(&texture);
            if let Some(image) = animation {
                self.slots.insert(texture, AnimationSlot { image, origin: None, uploaded: None });
            }
        }
    }

    pub(super) fn is_pending(&self, texture: TextureId, key: &str) -> bool {
        self.key(texture) == Some(key) && self.pending.contains_key(&texture)
    }

    pub(super) fn animation(&self, texture: TextureId, key: &str) -> Option<&Arc<AnimatedImage>> {
        (self.key(texture) == Some(key))
            .then(|| self.slots.get(&texture))
            .flatten()
            .map(|slot| &slot.image)
    }

    pub(super) fn forget(&mut self, texture: TextureId) {
        self.keys.remove(&texture);
        self.slots.remove(&texture);
        self.pending.remove(&texture);
    }

    /// Uploads the frame due for each visible animation. Playback restarts from the first
    /// visible frame after a scene change.
    pub(super) fn update(
        &mut self,
        scene: AppSceneKind,
        now: Instant,
        visible: impl Fn(TextureId) -> bool,
        mut upload: impl FnMut(TextureId, &RgbaImageAsset) -> bool,
    ) {
        if self.scene != Some(scene) {
            self.scene = Some(scene);
            for slot in self.slots.values_mut() {
                slot.origin = None;
            }
        }
        for (texture, slot) in &mut self.slots {
            if !visible(*texture) {
                continue;
            }
            let origin = *slot.origin.get_or_insert(now);
            let elapsed_us =
                u64::try_from(now.duration_since(origin).as_micros()).unwrap_or(u64::MAX);
            let index = slot.image.frame_index(elapsed_us);
            if slot.uploaded != Some(index) && upload(*texture, slot.image.frame(index)) {
                slot.uploaded = Some(index);
            }
        }
    }
}

impl WinitApp {
    /// Advances visible meta image animations for the plan just prepared for `scene`.
    pub(super) fn update_meta_image_animations(&mut self, scene: AppSceneKind) {
        self.meta_animations.poll();
        let Some(plan) = self.renderer.last_plan() else {
            return;
        };
        let visible: Vec<_> = [
            bmz_render::plan::SELECT_STAGE_TEXTURE,
            bmz_render::plan::PLAY_BACKBMP_TEXTURE,
            bmz_render::plan::SELECT_BANNER_TEXTURE,
        ]
        .into_iter()
        .filter(|texture| {
            skin_video_texture_visible_in_plan(plan, bmz_render::skin::SkinTextureId(texture.0))
        })
        .collect();
        let renderer = &mut self.renderer;
        self.meta_animations.update(
            scene,
            Instant::now(),
            |texture| visible.contains(&texture),
            |texture, frame| match renderer.upsert_rgba_texture_ref(
                texture,
                frame.width,
                frame.height,
                &frame.pixels,
            ) {
                Ok(()) => true,
                Err(error) => {
                    tracing::warn!(texture = texture.0, %error, "failed to upload meta image frame");
                    false
                }
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    const STAGE: TextureId = TextureId(20);

    fn animation() -> Arc<AnimatedImage> {
        let frames = [10, 20, 30]
            .map(|color| {
                let frame = RgbaImageAsset { width: 1, height: 1, pixels: vec![color; 4] };
                (frame, Duration::from_millis(100))
            })
            .into();
        Arc::new(AnimatedImage::new(frames, std::num::NonZeroU32::new(1)).unwrap())
    }

    fn run(
        animations: &mut MetaImageAnimations,
        scene: AppSceneKind,
        now: Instant,
        visible: bool,
    ) -> Vec<u8> {
        let mut uploads = Vec::new();
        animations.update(
            scene,
            now,
            |_| visible,
            |_, frame| {
                uploads.push(frame.pixels[0]);
                true
            },
        );
        uploads
    }

    #[test]
    fn plays_from_first_visible_frame_and_holds_the_last_frame() {
        let start = Instant::now();
        let mut animations = MetaImageAnimations::default();
        animations.replace(STAGE, "a", Some(animation()));

        assert!(run(&mut animations, AppSceneKind::Select, start, false).is_empty());
        let shown = start + Duration::from_secs(5);
        assert_eq!(run(&mut animations, AppSceneKind::Select, shown, true), [10]);
        assert!(run(&mut animations, AppSceneKind::Select, shown, true).is_empty());
        let later = shown + Duration::from_millis(150);
        assert_eq!(run(&mut animations, AppSceneKind::Select, later, true), [20]);
        let end = shown + Duration::from_secs(10);
        assert_eq!(run(&mut animations, AppSceneKind::Select, end, true), [30]);
    }

    #[test]
    fn scene_change_restarts_playback() {
        let start = Instant::now();
        let mut animations = MetaImageAnimations::default();
        animations.replace(STAGE, "a", Some(animation()));
        run(&mut animations, AppSceneKind::Select, start, true);
        let later = start + Duration::from_millis(150);
        assert_eq!(run(&mut animations, AppSceneKind::Select, later, true), [20]);

        assert_eq!(run(&mut animations, AppSceneKind::Decide, later, true), [10]);
    }

    #[test]
    fn replacing_or_forgetting_a_texture_stops_its_animation() {
        let mut animations = MetaImageAnimations::default();
        animations.replace(STAGE, "a", Some(animation()));
        assert!(animations.slots.contains_key(&STAGE));

        animations.replace(STAGE, "b", None);
        assert_eq!(animations.key(STAGE), Some("b"));
        assert!(run(&mut animations, AppSceneKind::Select, Instant::now(), true).is_empty());

        animations.replace(STAGE, "a", Some(animation()));
        animations.forget(STAGE);
        assert_eq!(animations.key(STAGE), None);
        assert!(run(&mut animations, AppSceneKind::Select, Instant::now(), true).is_empty());
    }

    #[test]
    fn background_decode_installs_only_for_the_requested_key() {
        let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../data/songs/bga-compat/animated.gif");
        let mut animations = MetaImageAnimations::default();
        animations.request(STAGE, "gif", fixture.clone());
        assert_eq!(animations.key(STAGE), Some("gif"));
        assert!(animations.pending.contains_key(&STAGE));
        let deadline = Instant::now() + Duration::from_secs(10);
        while animations.pending.contains_key(&STAGE) && Instant::now() < deadline {
            animations.poll();
            thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(animations.slots.get(&STAGE).map(|slot| slot.image.frames().len()), Some(2));

        animations.request(STAGE, "stale", fixture);
        animations.replace(STAGE, "newer", None);
        thread::sleep(Duration::from_millis(50));
        animations.poll();
        assert!(!animations.slots.contains_key(&STAGE));
        assert!(animations.pending.is_empty());
    }
}

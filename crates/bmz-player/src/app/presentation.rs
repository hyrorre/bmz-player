use super::*;
use crate::play_presentation::{PresentationImages, PresentationPlayback};

/// Play STAGEFILE presentation state. The animation itself is decoded by
/// `MetaImageAnimations` together with the STAGEFILE still image.
#[derive(Default)]
pub(super) struct PresentationRuntime {
    /// STAGEFILE key whose decode result is installed in `playback`.
    installed: Option<String>,
    pub(super) playback: PresentationPlayback,
    /// READY time added after the presentation started, before the chart countdown.
    pub(super) extra_ready_hold_us: i64,
    /// Rate to restore once the READY hold ends, and when it ends.
    pub(super) rate_restore: Option<(Instant, u16)>,
}

impl PresentationRuntime {
    pub(super) fn enter(&mut self) {
        self.playback.reset();
        self.extra_ready_hold_us = 0;
        self.rate_restore = None;
    }
}

impl WinitApp {
    /// Installs the Play STAGEFILE animation once its background decode finished.
    /// Returns false while that decode is still pending.
    pub(super) fn sync_play_presentation(&mut self) -> bool {
        self.meta_animations.poll();
        let Some(key) = self.play.play_stagefile_source.clone() else {
            if self.play.presentation.installed.take().is_some() {
                self.play.presentation.playback = PresentationPlayback::default();
            }
            return true;
        };
        if self.meta_animations.is_pending(SELECT_STAGE_TEXTURE, &key) {
            return false;
        }
        if self.play.presentation.installed.as_deref() != Some(key.as_str()) {
            let animation = self.meta_animations.animation(SELECT_STAGE_TEXTURE, &key).cloned();
            self.play.presentation.playback =
                PresentationPlayback::new(PresentationImages::from_stagefile(animation));
            self.play.presentation.installed = Some(key);
        }
        true
    }

    pub(super) fn draw_play_presentation(
        &mut self,
        ready: bool,
    ) -> Option<bmz_render::skin::SkinBgaFrame> {
        if !self.sync_play_presentation() {
            return None;
        }
        let now_us = self.play_elapsed_time().0;
        if self.play.play_entry_presentation.is_seamless() {
            self.play.presentation.playback.join_existing_scene();
        }
        self.play.presentation.playback.draw(&mut self.renderer, now_us, ready)
    }

    pub(super) fn restore_presentation_playback_rate(&mut self) {
        if let Some((deadline, rate)) = self.play.presentation.rate_restore
            && Instant::now() >= deadline
            && self.play.presentation.playback.ready_complete(self.play_elapsed_time().0)
            && self.play.active_play.as_ref().is_some_and(|play| play.running.gameplay.is_running())
        {
            self.play.presentation.rate_restore = None;
            if let Some(active) = &mut self.play.active_play {
                active.running.set_playback_rate_percent(rate);
            }
        }
    }
}

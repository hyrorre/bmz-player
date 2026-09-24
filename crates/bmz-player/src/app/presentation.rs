use super::*;
use crate::play_presentation::{PresentationImages, PresentationPlayback};

#[derive(Default)]
pub(super) struct PresentationRuntime {
    request: Option<(u64, i64)>,
    source: Option<Arc<PlayableChart>>,
    rx: Option<Receiver<PresentationImages>>,
    pub playback: PresentationPlayback,
    pub extra_ready_hold_us: i64,
    pub rate_restore: Option<(Instant, u16)>,
}

impl PresentationRuntime {
    pub fn enter(&mut self) {
        self.playback.reset();
        self.extra_ready_hold_us = 0;
        self.rate_restore = None;
    }
    fn current(&self, generation: u64) -> bool {
        self.request.is_some_and(|(value, _)| value == generation)
    }
    pub fn prepared(&self, generation: u64) -> bool {
        self.current(generation) && self.rx.is_none()
    }
}

impl WinitApp {
    pub(super) fn request_play_presentation(&mut self, chart_id: i64, chart: &Arc<PlayableChart>) {
        let generation = self.play.play_preload_generation;
        let state = &mut self.play.presentation;
        let same_source = state.source.as_ref().is_some_and(|old| Arc::ptr_eq(old, chart));
        let same_headers = state.source.as_ref().is_some_and(|old| {
            old.metadata.loading_file == chart.metadata.loading_file
                && old.metadata.ready_file == chart.metadata.ready_file
        });
        if state.request == Some((generation, chart_id)) && same_headers {
            state.source = Some(chart.clone());
            return;
        }
        let reuse = same_source && state.rx.is_none();
        state.request = Some((generation, chart_id));
        state.source = Some(chart.clone());
        state.rx = None; // Old worker results can no longer be installed.
        state.enter();
        if reuse {
            return;
        }
        state.playback = Default::default();
        let loading = chart.metadata.loading_file.clone();
        let ready = chart.metadata.ready_file.clone();
        if loading.is_empty() && ready.is_empty() {
            return;
        }
        let folder = self
            .boot
            .library_db
            .list_charts_by_ids(&[chart_id])
            .ok()
            .and_then(|charts| charts.into_iter().next())
            .map(|chart| PathBuf::from(chart.folder_path));
        let Some(folder) = folder else {
            tracing::warn!(chart_id, "chart presentation folder unavailable");
            return;
        };
        let (tx, rx) = mpsc::channel();
        match thread::Builder::new().name("chart-presentation".into()).spawn(move || {
            let _ = tx.send(PresentationImages::load(&folder, &loading, &ready));
        }) {
            Ok(_) => state.rx = Some(rx),
            Err(error) => tracing::warn!(%error, "cannot start chart presentation loader"),
        }
    }

    pub(super) fn poll_play_presentation(&mut self) {
        let state = &mut self.play.presentation;
        if !state.current(self.play.play_preload_generation) {
            return;
        }
        if let Some(rx) = &state.rx {
            match rx.try_recv() {
                Ok(images) => {
                    state.playback = PresentationPlayback::new(images);
                    state.rx = None;
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    tracing::warn!("chart presentation loader disconnected");
                    state.rx = None;
                }
                Err(mpsc::TryRecvError::Empty) => {}
            }
        }
    }

    pub(super) fn draw_play_presentation(
        &mut self,
        ready: bool,
    ) -> Option<bmz_render::skin::SkinBgaFrame> {
        if !self.play.presentation.prepared(self.play.play_preload_generation) {
            return None;
        }
        let now_us = self.play_elapsed_time().0;
        if self.play.play_entry_presentation.is_seamless() {
            // A seek/reload joins the existing Play clock instead of replaying the intro.
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

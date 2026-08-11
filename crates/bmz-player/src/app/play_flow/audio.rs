use super::*;

impl WinitApp {
    pub(super) fn ensure_audio_output(&mut self) {
        if self.audio.audio_runtime.is_some() || self.audio.audio_output_open_attempted {
            return;
        }
        self.audio.audio_output_open_attempted = true;

        match AudioRuntime::open(&self.boot.app_config.audio) {
            Ok(runtime) => {
                self.install_system_audio(&runtime, None);
                if let Err(error) = runtime.play() {
                    tracing::warn!(%error, "failed to start shared audio output stream");
                }
                self.audio.audio_runtime = Some(runtime);
                tracing::info!("audio output opened after window initialization");
            }
            Err(error) => {
                tracing::warn!(
                    error = ?error,
                    "failed to open shared audio output; running without audio"
                );
            }
        }
    }

    pub(super) fn log_audio_diagnostics(&mut self) {
        let now = Instant::now();
        if now.duration_since(self.audio.audio_diagnostics_last_log_at)
            < AUDIO_DIAGNOSTICS_LOG_INTERVAL
        {
            return;
        }
        self.audio.audio_diagnostics_last_log_at = now;

        if self.audio.audio_runtime.is_none() {
            self.audio.audio_diagnostics_last = None;
            return;
        };
        let snapshot = self.collect_audio_diagnostics();
        let Some(previous) = self.audio.audio_diagnostics_last.replace(snapshot) else {
            return;
        };
        if snapshot.callback_count < previous.callback_count {
            return;
        }

        let callbacks = snapshot.callback_count - previous.callback_count;
        if callbacks == 0 {
            return;
        }
        let rendered_frames = snapshot.rendered_frames.saturating_sub(previous.rendered_frames);
        let stream_errors = snapshot.stream_error_count.saturating_sub(previous.stream_error_count);
        let source_lock_misses =
            snapshot.source_lock_miss_count.saturating_sub(previous.source_lock_miss_count);
        let engine_lock_misses =
            snapshot.engine_lock_miss_count.saturating_sub(previous.engine_lock_miss_count);
        let engine_lock_miss_callbacks = snapshot
            .engine_lock_miss_callback_count
            .saturating_sub(previous.engine_lock_miss_callback_count);
        let system_engine_lock_misses = snapshot
            .system_engine_lock_miss_count
            .saturating_sub(previous.system_engine_lock_miss_count);
        let play_engine_lock_misses = snapshot
            .play_engine_lock_miss_count
            .saturating_sub(previous.play_engine_lock_miss_count);
        let draining_engine_lock_misses = snapshot
            .draining_engine_lock_miss_count
            .saturating_sub(previous.draining_engine_lock_miss_count);
        let other_engine_lock_misses = snapshot
            .other_engine_lock_miss_count
            .saturating_sub(previous.other_engine_lock_miss_count);
        let clipped_samples =
            snapshot.clipped_sample_count.saturating_sub(previous.clipped_sample_count);
        let command_drops =
            snapshot.command_dropped_count.saturating_sub(previous.command_dropped_count);
        let command_drain_lock_misses = snapshot
            .command_drain_lock_miss_count
            .saturating_sub(previous.command_drain_lock_miss_count);
        let command_engine_lock_misses = snapshot
            .command_engine_lock_miss_count
            .saturating_sub(previous.command_engine_lock_miss_count);
        let commands_submitted =
            snapshot.command_submitted_count.saturating_sub(previous.command_submitted_count);
        let commands_drained =
            snapshot.command_drained_count.saturating_sub(previous.command_drained_count);
        let commands_coalesced =
            snapshot.command_coalesced_count.saturating_sub(previous.command_coalesced_count);

        let sample_rate =
            self.audio.audio_runtime.as_ref().map(AudioRuntime::sample_rate).unwrap_or(1).max(1);
        let avg_callback_frames = rendered_frames as f64 / callbacks as f64;
        let callback_budget_ns =
            ((avg_callback_frames / f64::from(sample_rate)) * 1_000_000_000.0).round() as u64;
        let callback_over_budget =
            callback_budget_ns > 0 && snapshot.max_callback_ns > callback_budget_ns;
        let suspected_cause = classify_audio_output_issue(AudioOutputIssueMetrics {
            stream_errors,
            source_lock_misses,
            engine_lock_misses,
            command_drops,
            command_engine_lock_misses,
            callback_over_budget,
            clipped_samples,
            generated_preview_loading: self.select.select_assets.generated_preview_loading(),
        });

        if stream_errors == 0
            && source_lock_misses == 0
            && engine_lock_misses == 0
            && command_drops == 0
            && command_engine_lock_misses == 0
            && clipped_samples == 0
            && !callback_over_budget
        {
            return;
        }

        tracing::warn!(
            callbacks,
            rendered_frames,
            avg_callback_frames,
            sample_rate,
            stream_errors,
            source_lock_misses,
            engine_lock_misses,
            engine_lock_miss_callbacks,
            system_engine_lock_misses,
            play_engine_lock_misses,
            draining_engine_lock_misses,
            other_engine_lock_misses,
            commands_submitted,
            commands_drained,
            commands_coalesced,
            command_drops,
            command_drain_lock_misses,
            command_engine_lock_misses,
            command_queue_max_depth = snapshot.command_queue_max_depth,
            suspected_cause = suspected_cause.as_str(),
            generated_preview_loading = self.select.select_assets.generated_preview_loading(),
            select_preview_playing = self.select.select_assets.preview_playing(),
            select_preview_fade =
                select_preview_fade_name(self.select.select_assets.preview_fade()),
            select_preview_factor =
                select_preview_fade_factor(self.select.select_assets.preview_fade(), now),
            clipped_samples,
            peak_abs = snapshot.peak_abs,
            max_callback_us = snapshot.max_callback_ns / 1_000,
            callback_budget_us = callback_budget_ns / 1_000,
            "audio output diagnostics reported possible dropout or clipping",
        );
    }

    pub(super) fn log_input_diagnostics(&mut self) {
        let diagnostics = last_input_collection_diagnostics();
        if diagnostics.sequence == 0
            || diagnostics.sequence == self.audio.input_diagnostics_last_sequence
        {
            return;
        }
        self.audio.input_diagnostics_last_sequence = diagnostics.sequence;
        if diagnostics.drained_events == 0 {
            return;
        }

        tracing::debug!(
            target: "bmz_player::input_profile",
            sequence = diagnostics.sequence,
            drained_events = diagnostics.drained_events,
            translated_events = diagnostics.translated_events,
            dropped_events = diagnostics.dropped_events,
            timestamped_events = diagnostics.timestamped_events,
            min_event_age_us = ?diagnostics.min_event_age_us,
            max_event_age_us = ?diagnostics.max_event_age_us,
            max_future_event_us = ?diagnostics.max_future_event_us,
            "play input collection diagnostics"
        );
    }

    pub(super) fn collect_audio_diagnostics(&self) -> AudioOutputDiagnostics {
        let mut snapshot = self
            .audio
            .audio_runtime
            .as_ref()
            .map(AudioRuntime::take_diagnostics)
            .unwrap_or_default();
        if let Some(system_audio) = &self.audio.system_audio {
            snapshot.add_command_queue(system_audio.command_diagnostics());
        }
        if let Some(active_play) = &self.play.active_play {
            snapshot.add_command_queue(active_play.running.audio.command_diagnostics());
        }
        if let Some(draining_audio) = &self.audio.draining_audio {
            snapshot.add_command_queue(draining_audio.command_diagnostics());
        }
        snapshot
    }

    pub(super) fn install_system_audio(
        &mut self,
        runtime: &AudioRuntime,
        system_engine: Option<bmz_audio::command::AudioEngineHandle>,
    ) {
        let system_audio = match system_engine {
            Some(engine) => crate::audio::SystemAudio::reattach(runtime, engine),
            None => crate::audio::SystemAudio::open(runtime),
        };

        if self.audio.system_sound.is_none() {
            self.audio.system_sound = Some(system_sound_manager_from_catalog(
                &self.audio.system_sound_catalog,
                &system_audio,
            ));
        }
        if !self.select.select_assets.has_preview() {
            self.select
                .select_assets
                .install_preview(SelectChartPreview::new(system_audio.engine()));
        }
        self.audio.system_audio = Some(system_audio);
    }

    pub(super) fn reopen_audio_output(&mut self) {
        if self.play.active_play.is_some() || self.play.pending_play_start.is_some() {
            tracing::warn!("ignoring audio apply while a play session is active");
            return;
        }

        let system_engine = self.audio.system_audio.as_ref().map(crate::audio::SystemAudio::engine);
        self.audio.draining_audio = None;
        self.audio.system_audio = None;
        self.audio.audio_runtime = None;

        match AudioRuntime::open(&self.boot.app_config.audio) {
            Ok(runtime) => {
                self.install_system_audio(&runtime, system_engine);
                if let Err(error) = runtime.play() {
                    tracing::warn!(%error, "failed to start shared audio output stream");
                }
                self.audio.audio_runtime = Some(runtime);
                tracing::info!("audio output reopened with current settings");
            }
            Err(error) => {
                tracing::error!(
                    %error,
                    "failed to reopen audio output; audio disabled until restart"
                );
            }
        }
    }
}

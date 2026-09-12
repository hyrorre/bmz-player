use super::*;

impl WinitApp {
    pub(super) fn ensure_audio_output(&mut self) {
        if self.audio.audio_runtime.is_some() || self.audio.audio_output_open_attempted {
            return;
        }
        self.audio.audio_output_open_attempted = true;

        let audio_open_started_at = Instant::now();
        match AudioRuntime::open(&self.boot.app_config.audio).and_then(|runtime| {
            runtime.play()?;
            Ok(runtime)
        }) {
            Ok(runtime) => {
                // Viewer は譜面音声だけを必要とする。Select BGM / system SE の
                // engine と decode worker を作らず、direct Play の開始を優先する。
                if !self.viewer_mode {
                    self.install_system_audio(&runtime, None);
                }
                self.audio.audio_runtime = Some(runtime);
                tracing::info!(
                    audio_open_ms = audio_open_started_at.elapsed().as_millis(),
                    "audio output opened after window initialization"
                );
            }
            Err(error) => {
                tracing::warn!(%error, "failed to open audio output; running without audio");
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
        let timeline_catch_ups =
            snapshot.timeline_catch_up_count.saturating_sub(previous.timeline_catch_up_count);
        let timeline_catch_up_frames =
            snapshot.timeline_catch_up_frames.saturating_sub(previous.timeline_catch_up_frames);
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
            && timeline_catch_ups == 0
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
            timeline_catch_ups,
            timeline_catch_up_frames,
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

        if !self.select.select_assets.has_preview() {
            self.select
                .select_assets
                .install_preview(SelectChartPreview::new(system_audio.engine()));
        }
        self.audio.system_audio = Some(system_audio);
        if !self.viewer_mode
            && self.audio.system_sound.is_none()
            && self.audio.pending_system_sound.is_none()
        {
            self.start_system_sound_load();
        }
    }

    pub(super) fn start_system_sound_load(&mut self) {
        let Some(system_audio) = self.audio.system_audio.as_ref() else {
            return;
        };
        let output_sample_rate = system_audio.engine().output_sample_rate();
        let selection = system_sound_selection_from_catalog(&self.audio.system_sound_catalog);
        let normalize_bgm_volume = self.boot.profile_config.audio_mix.normalize_system_bgm_volume;
        let cache_dir = self.boot.app_paths.cache_dir.clone();
        self.audio.system_sound_generation = self.audio.system_sound_generation.wrapping_add(1);
        let generation = self.audio.system_sound_generation;
        let (tx, rx) = mpsc::channel();
        let event_proxy = self.event_proxy.clone();
        let spawn = thread::Builder::new().name("system-sound-load".to_string()).spawn(move || {
            let prepared = crate::system_sound_manager::SystemSoundManager::prepare(
                &selection,
                normalize_bgm_volume,
                output_sample_rate,
                Some(&cache_dir),
            );
            let _ = tx.send(SystemSoundLoadWorkerResult { generation, prepared });
            let _ = event_proxy.send_event(AppUserEvent::SystemSoundReady { generation });
        });
        match spawn {
            Ok(_) => {
                self.audio.pending_system_sound = Some(PendingSystemSoundLoad {
                    generation,
                    started_at: Instant::now(),
                    finished: rx,
                });
                tracing::info!(generation, normalize_bgm_volume, "started system sound worker");
            }
            Err(error) => {
                tracing::warn!(%error, generation, "failed to start system sound worker");
            }
        }
    }

    pub(super) fn poll_system_sound_load(&mut self) {
        let Some(pending) = self.audio.pending_system_sound.take() else {
            return;
        };
        let result = match pending.finished.try_recv() {
            Ok(result) => result,
            Err(mpsc::TryRecvError::Empty) => {
                self.audio.pending_system_sound = Some(pending);
                return;
            }
            Err(mpsc::TryRecvError::Disconnected) => {
                tracing::warn!(generation = pending.generation, "system sound worker disconnected");
                if self.first_frame_startup_completed && self.deferred_boot.is_some() {
                    self.start_deferred_boot();
                    self.sync_select_maintenance_gate();
                }
                return;
            }
        };
        if result.generation != self.audio.system_sound_generation {
            tracing::info!(
                generation = result.generation,
                current_generation = self.audio.system_sound_generation,
                "ignored stale system sound worker result"
            );
            return;
        }
        let normalize_bgm_volume = self.boot.profile_config.audio_mix.normalize_system_bgm_volume;
        if normalize_bgm_volume && !result.prepared.normalization_analysis_enabled {
            tracing::info!(
                generation = result.generation,
                "restarting system sound worker after normalization was enabled"
            );
            self.start_system_sound_load();
            return;
        }
        let stats = result.prepared.stats;
        let Some(system_audio) = self.audio.system_audio.as_ref() else {
            return;
        };
        if let Some(manager) = &self.audio.system_sound {
            manager.stop_all_bgm();
        }
        self.audio.system_sound =
            Some(crate::system_sound_manager::SystemSoundManager::from_prepared(
                system_audio.engine(),
                result.prepared,
                normalize_bgm_volume,
            ));
        self.sync_realtime_profile_settings();
        if matches!(self.view_state(), AppViewState::Select)
            && should_play_select_bgm_on_enter(
                self.select.select_assets.preview_playing(),
                self.audio.pending_system_sound.is_some(),
            )
        {
            self.play_system_sound(crate::system_sound::SoundType::Select);
        }
        tracing::info!(
            generation = result.generation,
            startup_to_system_sound_ready_ms = self.smoke.startup_started_at.elapsed().as_millis(),
            worker_elapsed_ms = pending.started_at.elapsed().as_millis(),
            decoded_count = stats.decoded_count,
            cache_hit_count = stats.cache_hit_count,
            analysis_count = stats.analysis_count,
            decode_ms = stats.decode_ms,
            analysis_ms = stats.analysis_ms,
            prepare_total_ms = stats.total_ms,
            normalize_bgm_volume,
            "system sounds ready"
        );
        if self.first_frame_startup_completed && self.deferred_boot.is_some() {
            self.start_deferred_boot();
            self.sync_select_maintenance_gate();
        }
    }

    pub(super) fn system_sound_load_blocks_deferred_boot(&self) -> bool {
        self.audio.system_audio.is_some() && self.audio.pending_system_sound.is_some()
    }

    pub(super) fn reopen_audio_output(&mut self) -> bool {
        if self.play.active_play.is_some() || self.play.pending_play_start.is_some() {
            tracing::warn!("ignoring audio apply while a play session is active");
            return false;
        }

        let previous_config =
            self.audio.audio_runtime.as_ref().map(|runtime| runtime.config().clone());
        let system_engine = self.audio.system_audio.as_ref().map(crate::audio::SystemAudio::engine);
        self.audio.draining_audio = None;
        self.audio.system_audio = None;
        self.audio.audio_runtime = None;

        match AudioRuntime::open(&self.boot.app_config.audio).and_then(|runtime| {
            runtime.play()?;
            Ok(runtime)
        }) {
            Ok(runtime) => {
                self.install_system_audio(&runtime, system_engine.clone());
                self.audio.audio_runtime = Some(runtime);
                tracing::info!("audio output reopened with current settings");
                true
            }
            Err(error) => {
                tracing::error!(error = %format!("{error:#}"), "failed to reopen audio output with current settings");
                let Some(previous_config) = previous_config else {
                    tracing::error!("no previous audio settings are available for recovery");
                    return false;
                };
                self.boot.app_config.audio = previous_config.clone();
                if let Err(save_error) =
                    save_app_config(&self.boot.app_paths.config_toml, &self.boot.app_config)
                {
                    tracing::error!(%save_error, "failed to restore previous audio settings on disk");
                }
                match AudioRuntime::open(&previous_config).and_then(|runtime| {
                    runtime.play()?;
                    Ok(runtime)
                }) {
                    Ok(runtime) => {
                        self.install_system_audio(&runtime, system_engine);
                        self.audio.audio_runtime = Some(runtime);
                        tracing::warn!("restored previous audio output after apply failure");
                    }
                    Err(restore_error) => {
                        tracing::error!(
                            restore_error = %format!("{restore_error:#}"),
                            "failed to restore previous audio output; audio disabled until restart"
                        );
                    }
                }
                false
            }
        }
    }
}

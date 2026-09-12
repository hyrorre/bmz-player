use super::*;

const VIEWER_SHORT_SEEK_US: i64 = 3_000_000;
const VIEWER_SNAPPED_SEEK_US: i64 = 5_000_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ViewerSeek {
    Measures(i32),
    Seconds(i64),
    SnappedSeconds(i64),
    Home,
}

impl WinitApp {
    pub(super) fn stop_viewer_playback(&mut self) {
        if self.play.active_play.is_some() {
            self.commit_active_play_lane_state_to_profile();
            let pause_result = {
                let active = self.play.active_play.as_mut().expect("checked above");
                let chart_time = active.running.audio.clock().now();
                active.running.pause_viewer_playback(chart_time)
            };
            if let Err(error) = pause_result {
                tracing::warn!(%error, "failed to pause viewer audio while waiting");
            }
            self.invalidate_play_preload();
            self.play.pending_play_start = None;
            self.play.play_ending = None;
            self.result.finished_play = None;
            self.result.result_exit = None;
            self.audio.draining_audio = None;
            self.viewer_paused = false;
            self.viewer_paused_play_elapsed = None;
            self.clear_play_control_holds();
            self.stop_system_sound(crate::system_sound::SoundType::PlayReady);
        } else {
            self.reset_viewer_playback();
        }
        self.viewer_waiting = true;
        if let Some(snapshot) = &mut self.play.last_play_snapshot {
            // 通常の Result 遷移用 fade を解除し、最終 Play frame を待機画面にする。
            snapshot.fadeout_elapsed_ms = None;
        }
    }

    pub(super) fn play_viewer_chart(
        &mut self,
        path: &Path,
        measure: u32,
        battle: bool,
    ) -> Result<()> {
        let path = path
            .canonicalize()
            .with_context(|| format!("failed to resolve viewer chart: {}", path.display()))?;
        if !crate::storage::scan::is_chart_file(&path) {
            bail!("unsupported viewer chart extension: {}", path.display());
        }

        // RANDOM分岐と実プレイで同じ譜面を使うため、importとPlayStartOptionsへ同じseedを渡す。
        // importが失敗した場合は現在の再生を維持し、成功してから差し替える。
        let bms_random_seed = crate::random_option_seed::fresh_bms_random_seed();
        self.load_viewer_chart(&path, measure, bms_random_seed, battle, None, None)
    }

    fn load_viewer_chart(
        &mut self,
        path: &Path,
        measure: u32,
        bms_random_seed: u64,
        battle: bool,
        preserved_options: Option<PlayStartOptions>,
        paused_play_elapsed: Option<TimeUs>,
    ) -> Result<()> {
        let imported = crate::storage::import::import_chart_file(
            &mut self.boot.library_db,
            path,
            None,
            Some(bms_random_seed),
            now_unix_seconds(),
        )?;
        let start_time = bootstrap::viewer_measure_start_time(&imported.chart, measure)?;

        tracing::info!(
            chart_id = imported.chart_id,
            path = %path.display(),
            measure,
            start_time_us = start_time.0,
            "replacing external viewer chart"
        );
        self.reset_viewer_playback();
        self.select.session_mode = viewer_session_mode(battle);
        self.viewer_waiting = false;
        let mut options = preserved_options.unwrap_or_else(|| self.play_start_options());
        options.session_mode = viewer_session_mode(battle);
        options.autoplay = true;
        options.score_save_disabled = true;
        options.bms_random_seed = Some(bms_random_seed);
        if !self.prepare_session_mode_or_show_error(imported.chart_id, &mut options) {
            self.viewer_waiting = true;
            bail!("viewer chart is unavailable for the active session mode");
        }
        self.play.practice_chart_zero_time = Some(start_time);
        self.start_play_preload(imported.chart_id, options.clone());
        self.begin_preloaded_play_scene_with_presentation(
            imported.chart_id,
            options,
            PlayEntryPresentation::ViewerSeek,
        );
        self.viewer_chart_path = Some(path.to_path_buf());
        self.viewer_bms_random_seed = Some(bms_random_seed);
        self.viewer_paused = paused_play_elapsed.is_some();
        self.viewer_paused_play_elapsed = paused_play_elapsed;
        Ok(())
    }

    pub(super) fn route_viewer_keyboard(&mut self, event: &winit::event::KeyEvent) -> bool {
        if !self.viewer_mode
            || self.viewer_waiting
            || self.play.active_play.is_none()
            || event.state != ElementState::Pressed
        {
            return false;
        }

        let shift = self.viewer_modifier_held(["LShift", "RShift"]);
        let control = self.viewer_modifier_held(["LControl", "RControl"]);
        let seek = viewer_keyboard_seek(event.physical_key, shift, control);
        if let Some(seek) = seek {
            if let Err(error) = self.seek_active_viewer(seek) {
                tracing::warn!(%error, ?seek, "viewer keyboard seek failed");
                self.show_left_overlay_toast(format!("Viewer seek failed: {error:#}"));
            }
            return true;
        }

        if event.repeat {
            return matches!(
                event.physical_key,
                PhysicalKey::Code(KeyCode::Space | KeyCode::F5 | KeyCode::Escape)
            );
        }
        match event.physical_key {
            PhysicalKey::Code(KeyCode::Space) => {
                if let Err(error) = self.toggle_viewer_pause() {
                    tracing::warn!(%error, "viewer pause toggle failed");
                    self.show_left_overlay_toast(format!("Viewer pause failed: {error:#}"));
                }
                true
            }
            PhysicalKey::Code(KeyCode::F5) => {
                if let Err(error) = self.reload_active_viewer(shift) {
                    tracing::warn!(%error, reroll = shift, "viewer reload failed");
                    self.show_left_overlay_toast(format!("Viewer reload failed: {error:#}"));
                }
                true
            }
            PhysicalKey::Code(KeyCode::Escape) => {
                self.begin_viewer_exit_transition("escape pressed during viewer play");
                true
            }
            _ => false,
        }
    }

    pub(super) fn route_waiting_viewer_keyboard(&mut self, event: &winit::event::KeyEvent) -> bool {
        if !self.viewer_mode || !self.viewer_waiting {
            return false;
        }
        if self.play.play_ending.is_some() {
            return true;
        }
        if event.state != ElementState::Pressed || self.play.active_play.is_none() {
            return false;
        }

        let shift = self.viewer_modifier_held(["LShift", "RShift"]);
        let control = self.viewer_modifier_held(["LControl", "RControl"]);
        if let Some(seek) = viewer_keyboard_seek(event.physical_key, shift, control) {
            if let Err(error) = self.seek_active_viewer(seek) {
                tracing::warn!(%error, ?seek, "viewer waiting seek failed");
                self.show_left_overlay_toast(format!("Viewer seek failed: {error:#}"));
            }
            return true;
        }
        if event.repeat {
            return matches!(
                event.physical_key,
                PhysicalKey::Code(KeyCode::Space | KeyCode::F5 | KeyCode::Escape)
            );
        }
        match event.physical_key {
            PhysicalKey::Code(KeyCode::Space) => {
                if let Err(error) = self.seek_active_viewer(ViewerSeek::Seconds(0)) {
                    tracing::warn!(%error, "viewer waiting resume failed");
                    self.show_left_overlay_toast(format!("Viewer resume failed: {error:#}"));
                }
                true
            }
            PhysicalKey::Code(KeyCode::F5) => {
                if let Err(error) = self.reload_active_viewer(shift) {
                    tracing::warn!(%error, reroll = shift, "viewer waiting reload failed");
                    self.show_left_overlay_toast(format!("Viewer reload failed: {error:#}"));
                }
                true
            }
            PhysicalKey::Code(KeyCode::Escape) => {
                self.begin_viewer_exit_transition("escape pressed while viewer waiting");
                true
            }
            _ => false,
        }
    }

    pub(super) fn route_viewer_mouse_wheel(&mut self, delta: MouseScrollDelta) -> bool {
        if !self.viewer_mode || self.play.active_play.is_none() {
            return false;
        }
        if self.play.play_ending.is_some() {
            return true;
        }
        let Some(direction) = viewer_wheel_direction(delta) else {
            return true;
        };
        let shift = self.viewer_modifier_held(["LShift", "RShift"]);
        let control = self.viewer_modifier_held(["LControl", "RControl"]);
        let seek = if control {
            ViewerSeek::SnappedSeconds(i64::from(direction) * VIEWER_SNAPPED_SEEK_US)
        } else if shift {
            ViewerSeek::Measures(direction * 4)
        } else {
            ViewerSeek::Measures(direction)
        };
        if let Err(error) = self.seek_active_viewer(seek) {
            tracing::warn!(%error, ?seek, "viewer mouse-wheel seek failed");
            self.show_left_overlay_toast(format!("Viewer seek failed: {error:#}"));
        }
        true
    }

    pub(super) fn begin_viewer_exit_transition(&mut self, reason: &'static str) -> bool {
        if !self.viewer_mode
            || (!self.viewer_waiting
                && self.play.active_play.is_none()
                && self.play.pending_play_start.is_none())
        {
            return false;
        }
        if self.play.play_ending.is_some() {
            return true;
        }

        tracing::info!(reason, "started viewer fadeout before app exit");
        if let Some(active) = &mut self.play.active_play {
            let chart_time = active.running.audio.clock().now();
            if let Err(error) = active.running.pause_viewer_playback(chart_time) {
                tracing::warn!(%error, "failed to pause viewer audio during exit");
            }
        }
        self.invalidate_play_preload();
        self.clear_play_control_holds();
        self.stop_system_sound(crate::system_sound::SoundType::PlayReady);
        self.play.play_ending = Some(viewer_exit_ending(Instant::now()));
        self.update_play_ending_snapshot();
        true
    }

    fn viewer_modifier_held<const N: usize>(&self, names: [&str; N]) -> bool {
        names.into_iter().any(|name| self.input.pressed_controls.contains(name))
    }

    fn seek_active_viewer(&mut self, seek: ViewerSeek) -> Result<()> {
        self.commit_active_play_lane_state_to_profile();
        let (
            chart,
            play_config_key_mode,
            input,
            sample_rate,
            normalization_gain,
            assist_runtime,
            playback_rate_percent,
            current_time,
        ) = {
            let active = self.play.active_play.as_ref().context("viewer play is not active")?;
            (
                Arc::clone(&active.running.session.chart),
                active.running.session.play_config_key_mode,
                SharedInputBackend::default(),
                active.running.audio.engine.output_sample_rate(),
                active.running.session.audio_mix.chart_normalization_gain,
                active.running.session.assist,
                active.running.playback_rate_percent,
                active.running.audio.clock().now(),
            )
        };
        let target = viewer_seek_target(&chart, current_time, seek);
        let mut options = self.active_play_retry_options(ResultRetryMode::SameArrange);
        options.playback_rate_percent = playback_rate_percent;
        let mut session_options =
            play_session_options_from_start(&self.play_session_app_config(), options);
        session_options.sample_rate = sample_rate;
        session_options.assist_runtime = assist_runtime;
        session_options.ln_policy_setting = self.boot.profile_config.play.ln_mode_policy;
        session_options.rule_mode = self.boot.profile_config.play.rule_mode;
        let mut session = build_viewer_seek_session(
            chart,
            play_config_key_mode,
            &self.boot.profile_config,
            session_options,
            input.clone(),
        );
        session.audio_mix.chart_normalization_gain = normalization_gain;

        let effects = self.gameplay_sound_output();
        let (carryover_count, feedback) = {
            let active = self.play.active_play.as_mut().context("viewer play ended during seek")?;
            active.running.gameplay = crate::gameplay_runtime::GameplayClient::new(session);
            active.input = input;
            active.running.play_duration_ms = None;
            active.running.finished = None;
            active.running.pending_finished = None;
            active.running.finish_error = None;
            active.running.result_graph =
                crate::screens::result_model::ResultGraphCollector::default();
            active.running.failed_video_bga.clear();
            crate::video_bga::prepare_reused_video_decoders_for_seek(
                &mut active.running.video_bga_decoders,
            );
            let carryover_count = active.running.start_viewer_seek(target, self.viewer_paused)?;
            active.running.start_gameplay_runtime(effects)?;
            let feedback = viewer_seek_feedback(&active.running.session.chart, target);
            (carryover_count, feedback)
        };
        self.play.viewer_ready_timer_offset =
            self.play_ready_animation_elapsed_time().unwrap_or(TimeUs(0));
        self.play.play_entry_presentation = PlayEntryPresentation::ViewerSeek;
        self.play.play_ready_sound_started_at = Some(Instant::now());
        self.play.play_ending = None;
        self.result.finished_play = None;
        self.viewer_waiting = false;
        self.sync_input_capture_target();
        tracing::info!(target_time_us = target.0, carryover_count, ?seek, "viewer seek applied");
        self.show_left_overlay_toast(feedback);
        Ok(())
    }

    fn reload_active_viewer(&mut self, reroll: bool) -> Result<()> {
        let path = self.viewer_chart_path.clone().context("viewer chart path is unavailable")?;
        let active = self.play.active_play.as_ref().context("viewer play is not active")?;
        let measure = viewer_measure_at_time(
            &active.running.session.chart,
            active.running.audio.clock().now(),
        );
        let seed = if reroll {
            crate::random_option_seed::fresh_bms_random_seed()
        } else {
            self.viewer_bms_random_seed
                .unwrap_or_else(crate::random_option_seed::fresh_bms_random_seed)
        };
        let options =
            (!reroll).then(|| self.active_play_retry_options(ResultRetryMode::SameArrange));
        let paused_play_elapsed = self.viewer_paused.then_some(self.play_elapsed_time());
        let battle = self.select.session_mode == SessionMode::AutoplayBattle;
        self.load_viewer_chart(&path, measure, seed, battle, options, paused_play_elapsed)?;
        self.show_left_overlay_toast(if reroll {
            format!("Reloaded measure {measure} with a new RANDOM")
        } else {
            format!("Reloaded measure {measure}")
        });
        Ok(())
    }

    fn toggle_viewer_pause(&mut self) -> Result<()> {
        if self.play.play_ready_sound_started_at.is_none() {
            return Ok(());
        }
        if self.viewer_paused {
            let frozen_play_elapsed =
                self.viewer_paused_play_elapsed.unwrap_or_else(|| self.play_elapsed_time());
            let chart_time = {
                let active = self.play.active_play.as_mut().context("viewer play is not active")?;
                active.running.resume_viewer_playback()?;
                active.running.audio.clock().now()
            };
            self.viewer_paused = false;
            self.viewer_paused_play_elapsed = None;
            self.play.play_scene_started_at = instant_for_elapsed(frozen_play_elapsed);
            self.show_left_overlay_toast(viewer_resume_feedback(chart_time));
        } else {
            let frozen_play_elapsed = self.play_elapsed_time();
            let chart_time = {
                let active = self.play.active_play.as_mut().context("viewer play is not active")?;
                let chart_time = active.running.audio.clock().now();
                active.running.pause_viewer_playback(chart_time)?;
                chart_time
            };
            self.viewer_paused = true;
            self.viewer_paused_play_elapsed = Some(frozen_play_elapsed);
            if let Some(snapshot) = &mut self.play.last_play_snapshot {
                snapshot.time = chart_time;
                snapshot.play_elapsed_time = frozen_play_elapsed;
            }
            self.show_left_overlay_toast(viewer_pause_feedback(chart_time));
        }
        Ok(())
    }

    pub(super) fn update_viewer_paused_snapshot(&mut self) {
        let Some(play_elapsed) = self.viewer_paused_play_elapsed else {
            return;
        };
        let chart_time =
            self.play.active_play.as_ref().map(|active| active.running.audio.clock().now());
        if let Some(snapshot) = &mut self.play.last_play_snapshot {
            snapshot.play_elapsed_time = play_elapsed;
            if let Some(chart_time) = chart_time {
                snapshot.time = chart_time;
            }
        }
    }

    /// ViewerのPlay/待機画面を明示的に抜け、プロセスを終了する。
    pub(super) fn finish_viewer_exit(&mut self, reason: &'static str) -> bool {
        if !self.viewer_mode
            || (!self.viewer_waiting
                && self.play.active_play.is_none()
                && self.play.pending_play_start.is_none()
                && self.play.play_ending.is_none())
        {
            return false;
        }

        tracing::info!(reason, "leaving viewer playback and exiting app");
        // 自然終了時は待機へ入る前に通知済み。保持中のactive_playだけを見て
        // Select退出時に二重通知しない。
        if !self.viewer_waiting
            && (self.play.active_play.is_some() || self.play.pending_play_start.is_some())
        {
            self.notify_obs_play_ended();
        }
        let completed_fadeout_ms = self
            .play_fadeout_duration(PlayEndingCompletion::ViewerExit)
            .as_millis()
            .min(i32::MAX as u128) as i32;
        self.reset_viewer_playback();
        self.viewer_waiting = false;
        self.viewer_chart_path = None;
        self.viewer_bms_random_seed = None;
        complete_viewer_exit_fade(&mut self.play.last_play_snapshot, completed_fadeout_ms);
        self.shutdown_requested.store(true, Ordering::SeqCst);
        self.request_redraw();
        true
    }

    /// 最終Play frameで待機中のE1/E2/E3物理holdを退出操作へ同期する。
    pub(super) fn sync_viewer_wait_exit_holds(&mut self) -> bool {
        if !self.viewer_mode || !self.viewer_waiting {
            return false;
        }
        let e1_held = self.input.select_e_action_holds.contains(&InputActionConfig::E1);
        let e2_held = self.input.select_e_action_holds.contains(&InputActionConfig::E2);
        let e3_held = self.input.select_e_action_holds.contains(&InputActionConfig::E3);
        self.play.play_e1_held = e1_held;
        self.play.play_e2_held = e2_held;
        self.play.play_e3_held = e3_held;
        self.update_play_exit_hold_timer();
        if play_exit_chord_pressed(e2_held, e3_held) {
            return self.begin_viewer_exit_transition("E2+E3 pressed while viewer waiting");
        }
        false
    }

    fn reset_viewer_playback(&mut self) {
        self.viewer_paused = false;
        self.viewer_paused_play_elapsed = None;
        self.stop_select_preview();
        if let Some(manager) = &self.audio.system_sound {
            manager.stop_all_bgm();
        }
        if let Some(audio) = &self.result.result_skin_audio {
            audio.stop_all();
        }
        self.play.pending_decide = None;
        self.result.finished_play = None;
        self.result.result_exit = None;
        self.abort_pending_play_start();
        self.audio.draining_audio = None;
        self.play.play_media_cache = None;
    }
}

pub(super) fn complete_viewer_exit_fade(snapshot: &mut Option<RenderSnapshot>, fadeout_ms: i32) {
    let snapshot = snapshot.get_or_insert_with(RenderSnapshot::default);
    snapshot.fadeout_elapsed_ms =
        Some(fadeout_ms.max(bmz_render::snapshot::DEFAULT_PLAY_FADEOUT_DURATION_MS));
}

const fn viewer_session_mode(battle: bool) -> SessionMode {
    if battle { SessionMode::AutoplayBattle } else { SessionMode::Autoplay }
}

fn instant_for_elapsed(elapsed: TimeUs) -> Instant {
    let micros = elapsed.0.max(0) as u64;
    Instant::now().checked_sub(Duration::from_micros(micros)).unwrap_or_else(Instant::now)
}

fn build_viewer_seek_session(
    chart: Arc<PlayableChart>,
    play_config_key_mode: KeyMode,
    profile: &ProfileConfig,
    mut options: crate::screens::play_session::PlaySessionOptions,
    input: SharedInputBackend,
) -> bmz_gameplay::session::GameSession {
    // The reused chart already contains the Battle presentation's duplicated 2P lanes.
    // Preserve the source mode so those lanes remain separate from the player's score.
    options.play_config_key_mode = Some(play_config_key_mode);
    crate::screens::play_session::build_game_session_with_input_backend(
        chart,
        profile,
        options,
        Box::new(input),
    )
}

fn viewer_pause_feedback(chart_time: TimeUs) -> String {
    format!("Paused at {:.3} s", chart_time.0 as f64 / 1_000_000.0)
}

fn viewer_resume_feedback(chart_time: TimeUs) -> String {
    format!("Resumed at {:.3} s", chart_time.0 as f64 / 1_000_000.0)
}

fn viewer_wheel_direction(delta: MouseScrollDelta) -> Option<i32> {
    let amount = match delta {
        MouseScrollDelta::LineDelta(_, y) => f64::from(y),
        MouseScrollDelta::PixelDelta(position) => position.y,
    };
    (amount != 0.0).then_some(if amount > 0.0 { 1 } else { -1 })
}

fn viewer_keyboard_seek(key: PhysicalKey, shift: bool, control: bool) -> Option<ViewerSeek> {
    match key {
        PhysicalKey::Code(KeyCode::ArrowLeft) => Some(if shift {
            ViewerSeek::Seconds(-VIEWER_SHORT_SEEK_US)
        } else {
            ViewerSeek::Measures(-1)
        }),
        PhysicalKey::Code(KeyCode::ArrowRight) => Some(if shift {
            ViewerSeek::Seconds(VIEWER_SHORT_SEEK_US)
        } else {
            ViewerSeek::Measures(1)
        }),
        PhysicalKey::Code(KeyCode::ArrowUp) => Some(if control {
            ViewerSeek::SnappedSeconds(VIEWER_SNAPPED_SEEK_US)
        } else if shift {
            ViewerSeek::Measures(4)
        } else {
            ViewerSeek::Measures(1)
        }),
        PhysicalKey::Code(KeyCode::ArrowDown) => Some(if control {
            ViewerSeek::SnappedSeconds(-VIEWER_SNAPPED_SEEK_US)
        } else if shift {
            ViewerSeek::Measures(-4)
        } else {
            ViewerSeek::Measures(-1)
        }),
        PhysicalKey::Code(KeyCode::Home) => Some(ViewerSeek::Home),
        _ => None,
    }
}

fn viewer_measure_at_time(chart: &PlayableChart, time: TimeUs) -> u32 {
    chart.bar_lines.iter().take_while(|bar| bar.time <= time).last().map_or(0, |bar| bar.measure)
}

fn viewer_seek_target(chart: &PlayableChart, current: TimeUs, seek: ViewerSeek) -> TimeUs {
    let clamp = |time: TimeUs| TimeUs(time.0.clamp(0, chart.end_time.0.max(0)));
    match seek {
        ViewerSeek::Home => chart.bar_lines.first().map_or(TimeUs(0), |bar| bar.time),
        ViewerSeek::Seconds(delta) => clamp(TimeUs(current.0.saturating_add(delta))),
        ViewerSeek::SnappedSeconds(delta) => {
            let raw = clamp(TimeUs(current.0.saturating_add(delta)));
            chart.bar_lines.iter().take_while(|bar| bar.time <= raw).last().map_or_else(
                || chart.bar_lines.first().map_or(TimeUs(0), |bar| bar.time),
                |bar| bar.time,
            )
        }
        ViewerSeek::Measures(delta) => {
            let bars = &chart.bar_lines;
            if bars.is_empty() {
                return TimeUs(0);
            }
            let current_index = bars.partition_point(|bar| bar.time <= current).saturating_sub(1);
            let target_index = if delta < 0 {
                current_index.saturating_sub(delta.unsigned_abs() as usize)
            } else {
                current_index.saturating_add(delta as usize).min(bars.len() - 1)
            };
            bars[target_index].time
        }
    }
}

fn viewer_seek_feedback(chart: &PlayableChart, target: TimeUs) -> String {
    let measure = viewer_measure_at_time(chart, target);
    format!("Measure {measure}  {:.3}s", target.0 as f64 / 1_000_000.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use bmz_chart::model::BarLine;
    use bmz_core::time::ChartTick;

    fn chart_with_bars() -> PlayableChart {
        let mut chart = crate::app::tests::app_test_chart();
        chart.bar_lines = (0..=5)
            .map(|measure| BarLine {
                measure,
                tick: ChartTick(u64::from(measure) * 192),
                time: TimeUs(1_000_000 + i64::from(measure) * 2_000_000),
            })
            .collect();
        chart.end_time = TimeUs(12_000_000);
        chart
    }

    #[test]
    fn viewer_battle_seek_keeps_prefix_and_boundary_scores_separate() {
        use bmz_chart::model::{NoteEvent, NoteKind};
        use bmz_core::ids::NoteId;
        use bmz_gameplay::session::{advance_session_frame, prepare_viewer_seek};

        for (source_mode, expanded_mode) in
            [(KeyMode::K5, KeyMode::K10), (KeyMode::K7, KeyMode::K14)]
        {
            let mut chart = crate::app::tests::app_test_chart();
            chart.metadata.key_mode = expanded_mode;
            for (index, lane) in [Lane::Key1, Lane::Key8].into_iter().enumerate() {
                for (note_index, time) in [1_000_000, 2_000_000].into_iter().enumerate() {
                    chart.lane_notes[lane.index()].push(NoteEvent {
                        id: NoteId((index * 2 + note_index + 1) as u32),
                        lane,
                        kind: NoteKind::Tap,
                        tick: ChartTick((note_index as u64 + 1) * 192),
                        time: TimeUs(time),
                        sound: None,
                        layered_sounds: Vec::new(),
                        damage: None,
                    });
                }
            }
            chart.total_notes = 4;
            chart.end_time = TimeUs(3_000_000);
            let profile = ProfileConfig::new_default("test", "Test", 1);
            let options = play_session_options_from_start(
                &AppConfig::default(),
                PlayStartOptions {
                    session_mode: SessionMode::AutoplayBattle,
                    autoplay: true,
                    ..Default::default()
                },
            );
            let mut session = build_viewer_seek_session(
                Arc::new(chart),
                source_mode,
                &profile,
                options,
                SharedInputBackend::default(),
            );
            assert_eq!(session.primary_key_mode, source_mode);
            assert_eq!(session.scored_total_notes, 2);
            assert!(session.display_only_lane_mask[Lane::Key8.index()]);

            prepare_viewer_seek(&mut session, TimeUs(2_000_000));
            assert_eq!((session.score.past_notes, session.score.ex_score()), (1, 2));
            let opponent = session.opponent_score.as_ref().unwrap();
            assert_eq!((opponent.past_notes, opponent.ex_score()), (1, 2));

            session.audio_clock = bmz_audio::clock::AudioClock::with_position(
                48_000,
                0,
                2_000_000,
                Arc::new(std::sync::atomic::AtomicU64::new(0)),
                true,
            );
            let mut audio = bmz_audio::engine::AudioEngine::new(48_000);
            advance_session_frame(&mut session, &mut audio);
            assert_eq!(
                (session.score.past_notes, session.score.combo, session.score.ex_score()),
                (2, 2, 4)
            );
            let opponent = session.opponent_score.as_ref().unwrap();
            assert_eq!((opponent.past_notes, opponent.combo, opponent.ex_score()), (2, 2, 4));
            advance_session_frame(&mut session, &mut audio);
            assert_eq!(session.score.ex_score(), 4);
        }
    }

    #[test]
    fn measure_seek_uses_containing_bar_and_clamps() {
        let chart = chart_with_bars();
        assert_eq!(
            viewer_seek_target(&chart, TimeUs(4_500_000), ViewerSeek::Measures(1)),
            TimeUs(5_000_000)
        );
        assert_eq!(
            viewer_seek_target(&chart, TimeUs(4_500_000), ViewerSeek::Measures(-9)),
            TimeUs(1_000_000)
        );
    }

    #[test]
    fn left_right_bind_measure_and_shift_three_second_seeks() {
        assert_eq!(
            viewer_keyboard_seek(PhysicalKey::Code(KeyCode::ArrowLeft), false, false),
            Some(ViewerSeek::Measures(-1))
        );
        assert_eq!(
            viewer_keyboard_seek(PhysicalKey::Code(KeyCode::ArrowRight), false, false),
            Some(ViewerSeek::Measures(1))
        );
        assert_eq!(
            viewer_keyboard_seek(PhysicalKey::Code(KeyCode::ArrowLeft), true, false),
            Some(ViewerSeek::Seconds(-3_000_000))
        );
        assert_eq!(
            viewer_keyboard_seek(PhysicalKey::Code(KeyCode::ArrowRight), true, false),
            Some(ViewerSeek::Seconds(3_000_000))
        );
    }

    #[test]
    fn second_seek_is_exact_and_snapped_seek_uses_previous_bar() {
        let chart = chart_with_bars();
        assert_eq!(
            viewer_seek_target(&chart, TimeUs(4_500_000), ViewerSeek::Seconds(3_000_000)),
            TimeUs(7_500_000)
        );
        assert_eq!(
            viewer_seek_target(&chart, TimeUs(4_500_000), ViewerSeek::SnappedSeconds(5_000_000)),
            TimeUs(9_000_000)
        );
    }
}

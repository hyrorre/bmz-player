use super::*;

impl WinitApp {
    pub(super) fn retry_last_chart_with_mode(&mut self, mode: ResultRetryMode) {
        let Some(chart_id) = self.play.last_started_chart_id else {
            tracing::warn!("no previous chart is available to retry");
            return;
        };
        let mut options = match mode {
            ResultRetryMode::SameArrange => self.result_retry_same_arrange_options(),
            ResultRetryMode::DifferentArrange => self.result_retry_different_arrange_options(),
        };
        if !self.prepare_session_mode_or_show_error(chart_id, &mut options) {
            return;
        }
        self.apply_rival_play_overrides(chart_id, &mut options);
        if options.chart_zero_time == TimeUs(0) {
            options.chart_zero_time = self.play_skin_playstart_offset();
        }

        if let Some(cache) = self.play.play_media_cache.take()
            && cache.chart_id == chart_id
        {
            self.start_retry_preload(chart_id, options.clone(), cache, mode);
            self.begin_preloaded_play_scene(chart_id, options);
            return;
        }
        self.start_chart_with_options(chart_id, options);
    }

    pub(super) fn retry_course_same_arrange(&mut self) {
        self.retry_course_with_mode(ResultRetryMode::SameArrange);
    }

    pub(super) fn retry_course_different_arrange(&mut self) {
        self.retry_course_with_mode(ResultRetryMode::DifferentArrange);
    }

    pub(super) fn retry_course_with_mode(&mut self, mode: ResultRetryMode) {
        let Some(course) = self.result.finished_course.as_ref() else {
            tracing::warn!("no finished course is available to retry");
            return;
        };
        let course_id = course.course_id;
        let arrange_overrides = match mode {
            ResultRetryMode::SameArrange => course.entry_arranges.clone(),
            ResultRetryMode::DifferentArrange => Vec::new(),
        };
        tracing::info!(course_id, entries = arrange_overrides.len(), ?mode, "retrying course");
        self.clear_finished_course();
        self.result.finished_play = None;
        self.result.result_exit = None;
        self.result.result_key5_held = false;
        self.result.result_key7_held = false;
        self.start_course_with_arrange(course_id, arrange_overrides, false);
    }

    pub(super) fn result_retry_same_arrange_options(&self) -> PlayStartOptions {
        let mut options = self.play_start_options();
        options.battle_target = self.play.last_battle_target.clone();
        if let Some(applied) =
            self.result.finished_play.as_ref().map(|finished| &finished.applied_arrange)
        {
            options.key_mode_conversion = applied.key_mode_conversion;
            options.seven_to_nine_pattern = applied.seven_to_nine_pattern;
            options.seven_to_nine_type = applied.seven_to_nine_type;
            options.seven_to_nine_rule_mode = applied.seven_to_nine_rule_mode;
            options.score_save_disabled |= applied.score_persistence_disabled();
            options.arrange = applied.arrange;
            options.arrange_2p = applied.arrange_2p;
            options.double_option = applied.double_option;
            options.arrange_seed = applied.seed;
            options.arrange_seed_2p = applied.seed_2p;
            options.legacy_arrange_seed = applied.legacy_seed;
            options.s_random_scheme = applied.s_random_scheme;
            options.s_random_scheme_2p = applied.s_random_scheme_2p;
            options.bms_random_choices = Some(applied.bms_random_choices.clone());
            options.bms_switch_choices = Some(applied.bms_switch_choices.clone());
            options.arrange_pattern = applied.pattern.clone();
        }
        options
    }

    pub(super) fn result_retry_different_arrange_options(&self) -> PlayStartOptions {
        let mut options = self.play_start_options();
        options.battle_target = self.play.last_battle_target.clone();
        if let Some(applied) =
            self.result.finished_play.as_ref().map(|finished| &finished.applied_arrange)
        {
            options.key_mode_conversion = applied.key_mode_conversion;
            options.seven_to_nine_pattern = applied.seven_to_nine_pattern;
            options.seven_to_nine_type = applied.seven_to_nine_type;
            options.seven_to_nine_rule_mode = applied.seven_to_nine_rule_mode;
            options.score_save_disabled |= applied.score_persistence_disabled();
            options.arrange = applied.arrange;
            options.arrange_pattern = None;
        }
        options
    }

    pub(super) fn active_play_retry_options(&self, mode: ResultRetryMode) -> PlayStartOptions {
        let mut options = self.play_start_options();
        options.battle_target = self.play.last_battle_target.clone();
        if let Some(active) = &self.play.active_play {
            let applied = &active.running.applied_arrange;
            options.key_mode_conversion = applied.key_mode_conversion;
            options.seven_to_nine_pattern = applied.seven_to_nine_pattern;
            options.seven_to_nine_type = applied.seven_to_nine_type;
            options.seven_to_nine_rule_mode = applied.seven_to_nine_rule_mode;
            options.score_save_disabled |= applied.score_persistence_disabled();
            options.arrange = applied.arrange;
            options.arrange_2p = applied.arrange_2p;
            options.double_option = applied.double_option;
            match mode {
                ResultRetryMode::SameArrange => {
                    options.arrange_seed = applied.seed;
                    options.arrange_seed_2p = applied.seed_2p;
                    options.legacy_arrange_seed = applied.legacy_seed;
                    options.s_random_scheme = applied.s_random_scheme;
                    options.s_random_scheme_2p = applied.s_random_scheme_2p;
                    options.bms_random_choices = Some(applied.bms_random_choices.clone());
                    options.bms_switch_choices = Some(applied.bms_switch_choices.clone());
                    options.arrange_pattern = applied.pattern.clone();
                }
                ResultRetryMode::DifferentArrange => {
                    options.arrange_pattern = None;
                }
            }
        }
        options
    }

    pub(super) fn take_play_media_cache_from_active(
        &mut self,
        chart_id: i64,
        mode: ResultRetryMode,
    ) -> Option<PlayMediaCache> {
        let active = self.play.active_play.as_mut()?;
        Some(PlayMediaCache::from_running(chart_id, &mut active.running, mode))
    }

    pub(super) fn capture_play_media_cache_from_running(
        &mut self,
        chart_id: i64,
        running: &mut crate::audio::RunningPlaySession,
    ) {
        self.play.play_media_cache =
            Some(PlayMediaCache::from_running(chart_id, running, ResultRetryMode::SameArrange));
    }

    pub(super) fn apply_reused_bga_preload(
        &mut self,
        chart_id: i64,
        bga_frames: BgaFrameCatalog,
        bga_assets: Vec<BgaAssetRef>,
    ) {
        let bga_frame_count = bga_frames.len();
        let bga_generation = self.play.bga_preload.apply_reused(chart_id, bga_frames, bga_assets);
        tracing::info!(
            chart_id,
            bga_generation,
            bga_frames = bga_frame_count,
            "reused static BGA preload"
        );
    }

    fn start_retry_preload(
        &mut self,
        chart_id: i64,
        options: PlayStartOptions,
        mut cache: PlayMediaCache,
        mode: ResultRetryMode,
    ) {
        let kind = retry_preload_kind(mode, cache.prepared_chart.is_some());
        let prepared = cache
            .prepared_chart
            .as_ref()
            .filter(|_| kind == RetryPreloadKind::CachedChartWithFreshAudio);
        let source = match prepared {
            Some(chart) => PlayPreloadSource::Cached {
                chart: Box::new(chart.clone()),
                normalization_gain: cache.chart_normalization_gain,
            },
            None => {
                cache.prepared_chart = None;
                PlayPreloadSource::Import {
                    library_db_path: self.boot.app_paths.library_db.clone(),
                    normalization_output_gain: crate::config::play::chart_normalization_output_gain(
                        &self.boot.profile_config,
                    ),
                }
            }
        };
        self.apply_reused_bga_preload(chart_id, cache.bga_frames.clone(), cache.bga_assets.clone());
        self.play.play_media_cache = Some(cache);
        let mut session_options =
            play_session_options_from_start(&self.play_session_app_config(), options);
        session_options.ln_policy_setting = self.boot.profile_config.play.ln_mode_policy;
        session_options.rule_mode = self.boot.profile_config.play.rule_mode;
        self.spawn_play_preload(chart_id, session_options, source);
    }

    pub(super) fn handle_quick_retry_control(&mut self, control: &str) -> bool {
        let Some(ending) = &self.play.play_ending else {
            return false;
        };
        if !ending.failed
            || self.play.active_course.is_some()
            || self.play.practice_session.is_some()
            || self.result.last_play_session_mode.primary_autoplay()
        {
            return false;
        }
        let mode = if self.select.select_keys.is_start(control) {
            Some(ResultRetryMode::DifferentArrange)
        } else if self.select.select_keys.is_e2_action(control) || matches!(control, "Select") {
            Some(ResultRetryMode::SameArrange)
        } else {
            None
        };
        let Some(mode) = mode else {
            return false;
        };
        self.quick_retry_active_play(mode);
        true
    }

    pub(super) fn begin_play_fadeout_after_final_notes_control(&mut self, control: &str) -> bool {
        if !play_fadeout_after_final_notes_control(control, &self.select.select_keys) {
            return false;
        }
        if let Some(ending) = &mut self.play.play_ending {
            if ending.failed {
                return false;
            }
            if ending.fadeout_started_at.is_none() {
                ending.fadeout_started_at = Some(Instant::now());
                self.update_play_ending_snapshot();
                tracing::info!(control, "started pending play fadeout");
            }
            return true;
        }

        let Some(active_play) = &self.play.active_play else {
            return false;
        };
        let should_begin = should_begin_play_fadeout_after_final_notes(
            control,
            &self.select.select_keys,
            self.play.play_ready_sound_started_at.is_some(),
            self.play.play_ending.is_some(),
            active_play.running.session.state,
            active_play.running.gameplay.final_notes_processed(),
        );
        let practice_playing = self
            .play
            .practice_session
            .as_ref()
            .is_some_and(|practice| practice.phase == PracticePhase::Playing);
        match final_notes_control_action(should_begin, practice_playing) {
            Some(FinalNotesControlAction::ReturnToPractice) => {
                let now = Instant::now();
                if let Some(active_play) = &mut self.play.active_play {
                    active_play
                        .running
                        .gameplay
                        .edit(|session| session.state = bmz_gameplay::session::PlayState::Finished);
                    if let Err(error) = active_play.running.pause_audio() {
                        tracing::warn!(%error, "failed to stop practice audio on requested finish");
                    }
                }
                self.commit_active_play_lane_state_to_profile();
                self.clear_play_control_holds();
                self.notify_obs_play_ended();
                self.play.play_ending = Some(practice_requested_finish_ending(now));
                self.update_play_ending_snapshot();
                tracing::info!(control, "started practice fadeout after final notes");
                return true;
            }
            Some(FinalNotesControlAction::BeginResultFadeout) => {}
            None => return false,
        }

        // The worker publishes the terminal result before the ordinary play
        // transition persists it. Never save an observation from before this command.
        self.play.active_play.as_mut().is_some_and(|active| {
            active
                .running
                .gameplay
                .edit(|session| session.state = bmz_gameplay::session::PlayState::Finished)
        })
    }

    pub(super) fn quick_retry_active_play(&mut self, mode: ResultRetryMode) {
        let Some(chart_id) = self.play.last_started_chart_id else {
            tracing::warn!("quick retry ignored without previous chart id");
            return;
        };
        let mut options = self.active_play_retry_options(mode);
        if !self.prepare_session_mode_or_show_error(chart_id, &mut options) {
            return;
        }
        self.apply_rival_play_overrides(chart_id, &mut options);
        if options.chart_zero_time == TimeUs(0) {
            options.chart_zero_time = self.play_skin_playstart_offset();
        }
        let media = self.take_play_media_cache_from_active(chart_id, mode);
        tracing::info!(chart_id, ?mode, "quick retrying chart");
        self.notify_obs_retry_play();
        self.save_configs_after_play(
            self.play.active_play.as_ref().map(|active| active.running.session.hispeed),
            "quick retry",
        );
        if let Some(active) = &mut self.play.active_play
            && let Err(error) = active.running.pause_audio()
        {
            tracing::warn!(%error, "failed to stop previous play audio for quick retry");
        }
        self.play.active_play = None;
        self.play.play_ending = None;
        self.result.finished_play = None;
        self.audio.draining_audio = None;
        self.clear_play_control_holds();
        match media {
            Some(cache) => self.start_retry_preload(chart_id, options.clone(), cache, mode),
            None => {
                self.play.play_media_cache = None;
                self.start_play_preload(chart_id, options.clone());
            }
        }
        self.prepare_play_skin_for_scene(chart_id, &options);
        self.enter_play_scene(chart_id, options, self.decide_snapshot_for_chart(chart_id));
        self.poll_play_preload();
    }
}

use super::*;

impl WinitApp {
    pub(super) fn view_state(&self) -> AppViewState {
        if self.play.pending_decide.is_some() {
            return AppViewState::Decide;
        }
        if self.play.active_play.is_some() || self.play.pending_play_start.is_some() {
            return AppViewState::Play;
        }

        if self.result.finished_course.is_some() || self.result.finished_play.is_some() {
            return AppViewState::Result;
        }

        AppViewState::Select
    }

    pub(super) fn current_scene_kind(&self) -> AppSceneKind {
        if self.play.pending_decide.is_some() {
            return AppSceneKind::Decide;
        }
        if self.play.active_play.is_some() || self.play.pending_play_start.is_some() {
            return AppSceneKind::Play;
        }
        if self.result.finished_course.is_some() || self.result.finished_play.is_some() {
            return AppSceneKind::Result;
        }
        AppSceneKind::Select
    }

    pub(super) fn current_result_summary(&self) -> Option<&ResultSummary> {
        self.result
            .finished_course_skin_summary
            .as_ref()
            .or_else(|| self.result.finished_play.as_ref().map(|finished| &finished.summary))
    }

    pub(super) fn install_finished_course(
        &mut self,
        course: CourseResultSummary,
        course_hash: Option<String>,
        rian_course_hash_v1: Option<String>,
        bms_ir_course_key: Option<String>,
    ) {
        self.result.finished_course_skin_summary = Some(course_result_summary_for_skin(&course));
        self.result.finished_course = Some(course);
        self.result.finished_course_hash = course_hash;
        self.result.finished_course_rian_hash_v1 = rian_course_hash_v1;
        self.result.finished_course_bms_ir_key = bms_ir_course_key;
        self.result.finished_course_ir_attempted = false;
    }

    pub(super) fn clear_finished_course(&mut self) {
        self.result.prepared_course_finish = None;
        self.result.finished_course = None;
        self.result.finished_course_skin_summary = None;
        self.result.finished_course_hash = None;
        self.result.finished_course_rian_hash_v1 = None;
        self.result.finished_course_bms_ir_key = None;
        self.result.finished_course_ir_attempted = false;
    }

    pub(super) fn scene_snapshot(&self) -> AppSceneSnapshot {
        let mut scene = match self.view_state() {
            AppViewState::Select => AppSceneSnapshot::Select(self.select_snapshot()),
            AppViewState::Decide => {
                let mut snapshot = self
                    .play
                    .pending_decide
                    .as_ref()
                    .map(|decide| self.decide_snapshot(decide))
                    .or_else(|| self.play.last_play_snapshot.clone())
                    .unwrap_or_default();
                snapshot.skin_offsets =
                    skin_offset_values_from_config(&self.boot.profile_config.skin.decide_offsets);
                AppSceneSnapshot::Decide(snapshot)
            }
            AppViewState::Play => {
                AppSceneSnapshot::Play(self.play.last_play_snapshot.clone().unwrap_or_default())
            }
            AppViewState::Result => {
                // `view_state` only returns Result when one of the result sources exists.
                // A finished course is always installed together with its skin summary.
                let summary =
                    self.current_result_summary().expect("result scene is missing its summary");
                let raw_clear_type = self
                    .is_course_intermediate_result()
                    .then(|| {
                        self.result
                            .finished_play
                            .as_ref()
                            .map(|finished| finished.result.clear_type)
                    })
                    .flatten();
                let result_failed = result_failed_for_skin_ops(summary.clear_type, raw_clear_type);
                let score_save_enabled = self.current_result_score_save_enabled();
                let frozen_score_policy = self
                    .result
                    .finished_course
                    .as_ref()
                    .map(|course| (course.rule_mode, course.ln_policy))
                    .or_else(|| {
                        self.result
                            .finished_play
                            .as_ref()
                            .map(|play| (play.rule_mode, play.ln_policy))
                    });
                let rule_mode = frozen_score_policy
                    .map(|(rule_mode, _)| rule_mode)
                    .unwrap_or(self.boot.profile_config.play.rule_mode);
                let ln_score_policy_index = frozen_score_policy.map(|(_, ln_score_policy)| {
                    crate::skin_extension::ln_score_policy_index(ln_score_policy)
                });
                let result_ir_scope_binding = self
                    .renderer
                    .result_skin_document()
                    .map(|document| document.result_ir_scope_binding)
                    .unwrap_or_default();
                AppSceneSnapshot::Result(ResultSnapshot {
                    player_name: String::new(),
                    target_name: summary.target_name.clone(),
                    current_fps: 0,
                    skin_input: Default::default(),
                    skin_attempt: summary.skin_attempt,
                    skin_offsets: skin_offset_values_from_config(
                        match self.current_result_skin_slot() {
                            ResultSkinSlot::Normal => &self.boot.profile_config.skin.result_offsets,
                            ResultSkinSlot::Course => {
                                &self.boot.profile_config.skin.course_result_offsets
                            }
                        },
                    ),
                    mouse_position: self
                        .renderer
                        .result_skin_mouse_position(self.cursor_position_normalized()),
                    hispeed_auto_adjust: self.boot.profile_config.lane.hispeed_auto_adjust,
                    clear_type: summary.clear_type,
                    result_failed,
                    autoplay: self.current_result_autoplay(),
                    arrange: summary.arrange.as_str().to_string(),
                    arrange_2p: summary.arrange_2p.as_str().to_string(),
                    double_option: self
                        .result_double_option_for_slot(self.current_result_skin_slot())
                        .as_str()
                        .to_string(),
                    lane_shuffle_pattern: summary.lane_shuffle_pattern.clone(),
                    ex_score: summary.ex_score,
                    ex_score_rate: summary.ex_score_rate(),
                    max_combo: summary.max_combo,
                    bp: summary.bp,
                    cb: summary.cb,
                    gauge_value: summary.gauge_value,
                    gauge_type: summary.gauge_type as i32,
                    total_notes: summary.total_notes,
                    duration_ms: summary.duration_ms,
                    note_display_duration_ms: Some(Self::select_note_display_duration_ms_for_skin(
                        &self.boot.profile_config,
                    )),
                    initial_bpm: summary.initial_bpm,
                    min_bpm: result_min_bpm(summary),
                    max_bpm: result_max_bpm(summary),
                    main_bpm: result_main_bpm(summary),
                    total_gauge: summary.total_gauge,
                    judge_rank: summary.judge_rank,
                    key_mode: summary.key_mode,
                    has_long_notes: summary.has_long_notes,
                    ln_mode_index: result_long_note_mode_index(summary.long_note_mode),
                    rule_mode_index: crate::skin_extension::rule_mode_index(rule_mode),
                    ln_score_policy_index,
                    result_gauge_graph_type: self.result.result_gauge_graph_type,
                    result_panel: self.result.result_panel,
                    favorite_chart: self.result.result_favorite_chart,
                    judge_counts: DisplayJudgeCounts {
                        pgreat: summary.judge_counts.pgreat,
                        great: summary.judge_counts.great,
                        good: summary.judge_counts.good,
                        bad: summary.judge_counts.bad,
                        poor: summary.judge_counts.poor,
                        empty_poor: summary.judge_counts.empty_poor,
                    },
                    fast_slow_counts: FastSlowJudgeCounts {
                        fast_pgreat: summary.fast_slow_counts.fast_pgreat,
                        slow_pgreat: summary.fast_slow_counts.slow_pgreat,
                        fast_great: summary.fast_slow_counts.fast_great,
                        slow_great: summary.fast_slow_counts.slow_great,
                        fast_good: summary.fast_slow_counts.fast_good,
                        slow_good: summary.fast_slow_counts.slow_good,
                        fast_bad: summary.fast_slow_counts.fast_bad,
                        slow_bad: summary.fast_slow_counts.slow_bad,
                        fast_poor: summary.fast_slow_counts.fast_poor,
                        slow_poor: summary.fast_slow_counts.slow_poor,
                        fast_empty_poor: summary.fast_slow_counts.fast_empty_poor,
                        slow_empty_poor: summary.fast_slow_counts.slow_empty_poor,
                    },
                    score_save_enabled,
                    assist_flags: self.boot.profile_config.play.assist.flags(),
                    assist_extra_note_depth: self.boot.profile_config.play.assist.extra_note_depth,
                    assist_mine_mode: self.boot.profile_config.play.assist.mine_mode as i64,
                    assist_scroll_mode: self.boot.profile_config.play.assist.scroll_mode as i64,
                    assist_long_note_mode: self.boot.profile_config.play.assist.long_note_mode
                        as i64,
                    score_history_id: summary.score_history_id,
                    replay_saved: !summary.replay_path.is_empty(),
                    replay_slots: summary.replay_slots,
                    saved_replay_slots: summary.saved_replay_slots,
                    best_ex_score: summary.best_ex_score,
                    best_clear_type: summary.best_clear_type,
                    target_ex_score: summary.target_ex_score,
                    best_max_combo: summary.best_max_combo,
                    target_max_combo: summary.target_max_combo,
                    best_bp: summary.best_bp,
                    target_bp: summary.target_bp,
                    previous_best_ex_score: summary.previous_best_ex_score,
                    previous_best_clear_type: summary.previous_best_clear_type,
                    previous_best_max_combo: summary.previous_best_max_combo,
                    previous_best_bp: summary.previous_best_bp,
                    target_clear_type: summary.target_clear_type,
                    elapsed_time: bmz_core::time::TimeUs(
                        self.result
                            .result_scene_started_at
                            .elapsed()
                            .as_micros()
                            .min(i64::MAX as u128) as i64,
                    ),
                    fadeout_elapsed: self.result.result_exit.as_ref().map(|exit| {
                        bmz_core::time::TimeUs(
                            exit.started_at.elapsed().as_micros().min(i64::MAX as u128) as i64,
                        )
                    }),
                    title: summary.title.clone(),
                    subtitle: summary.subtitle.clone(),
                    artist: summary.artist.clone(),
                    subartist: summary.subartist.clone(),
                    genre: summary.genre.clone(),
                    difficulty_name: summary.difficulty_name.clone(),
                    play_level: summary.play_level.clone(),
                    table_text_primary: self.play.play_table_text_primary.clone(),
                    table_text_secondary: self.play.play_table_text_secondary.clone(),
                    table_text_fallback: self.play.play_table_text_fallback.clone(),
                    stagefile_background: self.play.play_stagefile_loaded,
                    stagefile_image_size: self.play.play_stagefile_size,
                    course_titles: self
                        .result
                        .finished_course
                        .as_ref()
                        .map(|course| course.course_titles.clone())
                        .unwrap_or_default(),
                    course_result: self
                        .result
                        .finished_course
                        .as_ref()
                        .map(course_result_skin_snapshot)
                        .unwrap_or_default(),
                    graph: summary.graph.clone(),
                    overlay: OverlaySnapshot::default(),
                    ir: self
                        .result
                        .result_ir
                        .as_ref()
                        .map(|state| state.skin_snapshot_for_binding(result_ir_scope_binding))
                        .unwrap_or_default(),
                    player_stats: self.select.player_stats.clone(),
                })
            }
        };
        apply_skin_logical_input_to_scene(
            &mut scene,
            skin_logical_input_snapshot_from_pressed_controls(
                &self.input.pressed_controls,
                &self.select.select_keys,
            ),
        );
        self.apply_operating_time_to_scene(&mut scene);
        self.apply_skin_runtime_info_to_scene(&mut scene);
        let overlay = self.build_overlay_snapshot();
        self.apply_overlay_to_scene(&mut scene, overlay);
        scene
    }

    pub(super) fn operating_time_ms(&self) -> i32 {
        elapsed_since_ms(self.smoke.app_started_at)
    }

    pub(super) fn apply_operating_time_to_scene(&self, scene: &mut AppSceneSnapshot) {
        apply_operating_time_ms_to_scene(scene, self.operating_time_ms());
    }

    pub(super) fn apply_skin_runtime_info_to_scene(&self, scene: &mut AppSceneSnapshot) {
        apply_skin_runtime_info_to_scene(
            scene,
            &self.boot.profile_config.display_name,
            self.frame.current_fps(),
        );
    }

    pub(super) fn build_overlay_snapshot(&self) -> OverlaySnapshot {
        OverlaySnapshot {
            left_text: self.left_overlay_text(),
            text: self.always_overlay_text(),
            fps_text: self.skin_fps_overlay_text(),
        }
    }

    pub(super) fn left_overlay_text(&self) -> String {
        resolve_left_overlay_text(
            self.renderer.has_pending_screenshot(),
            self.smoke
                .left_overlay_toast
                .as_ref()
                .map(|toast| (toast.message.as_str(), toast.shown_at.elapsed())),
            &self.background_task_overlay_text(),
        )
    }

    pub(super) fn background_task_overlay_text(&self) -> String {
        let mut tasks = Vec::new();
        if let Some(progress) = self.jobs.song_scan_progress {
            tasks.push(format!("SCAN {} / {}", progress.done, progress.total));
        }
        if let Some(pending) = &self.jobs.pending_replay_import {
            tasks.push(format!(
                "REPLAY {} / {}",
                pending.done.load(Ordering::Relaxed),
                pending.total.load(Ordering::Relaxed)
            ));
        }
        if let Some(progress) = &self.jobs.table_fetch.progress {
            tasks.push(format!("TABLE {} / {}", progress.completed, progress.total));
        }
        tasks.join(" | ")
    }

    pub(super) fn always_overlay_text(&self) -> String {
        let player_name = env!("CARGO_PKG_NAME");
        let player_version = env!("CARGO_PKG_VERSION");
        let (autoplay, replay_playback) = self.playback_flags_for_overlay();
        match playback_overlay_suffix(self.session_mode_for_overlay(), autoplay, replay_playback) {
            Some(suffix) => format!("{player_name} {player_version} {suffix}"),
            None => format!("{player_name} {player_version}"),
        }
    }

    pub(super) fn skin_fps_overlay_text(&self) -> String {
        self.frame.overlay_text(
            self.boot.profile_config.ui.show_fps,
            Localizer::new(self.boot.profile_config.ui.locale()),
        )
    }

    pub(super) fn session_mode_for_overlay(&self) -> SessionMode {
        match self.view_state() {
            AppViewState::Result => self.result.last_play_session_mode,
            AppViewState::Play => self
                .play
                .pending_play_start
                .as_ref()
                .map(|pending| pending.options.session_mode)
                .unwrap_or(self.result.last_play_session_mode),
            AppViewState::Decide => self
                .play
                .pending_decide
                .as_ref()
                .map(|pending| pending.options.session_mode)
                .unwrap_or(self.select.session_mode),
            AppViewState::Select => self.select.session_mode,
        }
    }

    pub(super) fn playback_flags_for_overlay(&self) -> (bool, bool) {
        match self.view_state() {
            AppViewState::Play => self
                .play
                .last_play_snapshot
                .as_ref()
                .map(|snapshot| (snapshot.autoplay, snapshot.replay_playback))
                .unwrap_or_default(),
            AppViewState::Decide => self
                .play
                .pending_decide
                .as_ref()
                .map(|pending| {
                    let mode = pending.options.session_mode;
                    let replay_playback = pending.options.replay_player.is_some();
                    let autoplay = !replay_playback
                        && !mode.is_practice()
                        && (mode.primary_autoplay()
                            || pending.options.autoplay
                            || self.boot.profile_config.play.auto_play);
                    (autoplay, replay_playback)
                })
                .unwrap_or_default(),
            AppViewState::Select | AppViewState::Result => (false, false),
        }
    }

    pub(super) fn apply_overlay_to_scene(
        &self,
        scene: &mut AppSceneSnapshot,
        overlay: OverlaySnapshot,
    ) {
        match scene {
            AppSceneSnapshot::Select(snapshot) => snapshot.overlay = overlay,
            AppSceneSnapshot::Decide(snapshot) | AppSceneSnapshot::Play(snapshot) => {
                snapshot.overlay = overlay
            }
            AppSceneSnapshot::Result(snapshot) => snapshot.overlay = overlay,
        }
    }

    pub(super) fn fallback_table_breadcrumb(source_url: &str) -> TableBreadcrumb {
        TableBreadcrumb {
            name: std::path::Path::new(source_url)
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or(source_url)
                .to_string(),
            symbol: String::new(),
        }
    }

    pub(super) fn table_breadcrumb(&self, source_url: &str) -> TableBreadcrumb {
        if let Some(cached) = self.select.table_breadcrumb_cache.borrow().get(source_url) {
            return cached.clone();
        }

        let breadcrumb = self
            .select
            .difficulty_tables
            .iter()
            .find(|table| table.source_url == source_url)
            .map(table_breadcrumb_from_record)
            .unwrap_or_else(|| Self::fallback_table_breadcrumb(source_url));
        self.select
            .table_breadcrumb_cache
            .borrow_mut()
            .insert(source_url.to_string(), breadcrumb.clone());
        breadcrumb
    }

    /// 難易度表のパンくず表示名。テーブルが既知なら表名、
    /// 不明なら URL のファイル名部分にフォールバックする。
    pub(super) fn table_breadcrumb_name(&self, source_url: &str) -> String {
        self.table_breadcrumb(source_url).name
    }

    pub(super) fn table_text_context_for_chart(&self, chart_id: i64) -> DifficultyTableText {
        self.table_text_context_for_chart_with_metadata(chart_id, None)
    }

    pub(super) fn table_text_context_for_chart_with_metadata(
        &self,
        chart_id: i64,
        chart_hint: Option<&ChartListItem>,
    ) -> DifficultyTableText {
        if let Some(table_text) = self.select.select_items.iter().find_map(|item| match item {
            SelectItem::Chart(row)
                if row.chart.as_ref().is_some_and(|chart| chart.chart_id == chart_id) =>
            {
                row.table_text.is_table_song().then(|| row.table_text.clone())
            }
            _ => None,
        }) {
            return table_text;
        }
        let selected = self.select.select_items.get(self.select.selected_index);
        let source_hint = table_source_url_from_context(&self.select.folder_stack, selected);
        let source_order = table_source_order(&self.boot.app_config);

        let chart = chart_hint.cloned().or_else(|| self
            .select.select_items
            .iter()
            .find_map(|item| match item {
                SelectItem::Chart(row)
                    if row.chart.as_ref().is_some_and(|chart| chart.chart_id == chart_id) =>
                {
                    row.chart.clone()
                }
                _ => None,
            })
            )
            .or_else(|| {
                self.boot
                    .library_db
                    .list_charts_by_ids(&[chart_id])
                    .map_err(|error| {
                        tracing::warn!(%error, chart_id, "failed to load chart for table skin text");
                        error
                    })
                    .ok()
                    .and_then(|mut charts| charts.pop())
            });

        let Some(chart) = chart else {
            return DifficultyTableText::default();
        };

        difficulty_table_text_for_chart_with_active_sources(
            &self.boot.library_db,
            &chart,
            &source_order,
            source_hint.as_deref(),
            Some(&source_order),
        )
        .map_err(|error| {
            tracing::warn!(%error, chart_id, "failed to resolve difficulty table skin text");
            error
        })
        .unwrap_or_default()
    }

    pub(super) fn capture_play_table_text_for_chart(&mut self, chart_id: i64) {
        let (primary, secondary, fallback) = self.table_text_context_for_chart(chart_id).as_tuple();
        self.play.play_table_text_primary = primary;
        self.play.play_table_text_secondary = secondary;
        self.play.play_table_text_fallback = fallback;
    }

    pub(super) fn apply_play_table_text(&self, snapshot: &mut RenderSnapshot) {
        snapshot.table_text_primary = self.play.play_table_text_primary.clone();
        snapshot.table_text_secondary = self.play.play_table_text_secondary.clone();
        snapshot.table_text_fallback = self.play.play_table_text_fallback.clone();
    }
}

pub(super) const fn playback_overlay_suffix(
    mode: SessionMode,
    autoplay: bool,
    replay_playback: bool,
) -> Option<&'static str> {
    if replay_playback {
        return Some("replay");
    }
    match mode {
        SessionMode::Normal if autoplay => Some("AUTOPLAY"),
        SessionMode::Normal => None,
        SessionMode::Practice => Some("PRACTICE"),
        SessionMode::Autoplay => Some("AUTOPLAY"),
        SessionMode::AutoplayBattle => Some("AUTO BATTLE"),
        SessionMode::GBattle => Some("G-BATTLE"),
    }
}

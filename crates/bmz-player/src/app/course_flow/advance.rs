use super::*;

impl WinitApp {
    fn next_course_stage_start(&self) -> Option<(i64, usize, i64, PlayStartOptions)> {
        let course = self.play.active_course.as_ref()?;
        let (entry_index, chart_id, options) = course.next_stage_start()?;
        Some((course.course_id, entry_index, chart_id, options))
    }

    /// 中間リザルト表示中に次曲の譜面/WAV/BGA と Play skin を先読みする。
    fn begin_next_course_chart_preload(&mut self) {
        let Some((course_id, entry_index, chart_id, options)) = self.next_course_stage_start()
        else {
            return;
        };
        if self.play.pending_course_stage_launch.as_ref().is_some_and(|launch| {
            launch.matches(course_id, entry_index, chart_id)
                && launch.preload_generation == self.play.play_preload_generation
        }) {
            return;
        }

        if self.play.play_media_cache.as_ref().is_some_and(|cache| cache.chart_id != chart_id) {
            self.play.play_media_cache = None;
        }
        // 先に楽曲 worker を起動し、その後の Play/Result skin 準備時間とも重ねる。
        let preload_generation = self.start_play_preload(chart_id, options.clone());
        self.play.pending_course_stage_launch = Some(PendingCourseStageLaunch {
            course_id,
            entry_index,
            chart_id,
            options: options.clone(),
            preload_generation,
            preload_error: None,
        });

        let play_skin_key_mode = self.play_skin_key_mode_for_chart(chart_id, &options);
        let skin_attempt = self.skin_attempt_for_chart(chart_id, &options);
        let play_skin_runtime_state = lua_runtime_state_for_play(
            &options,
            self.boot.profile_config.play.auto_play,
            play_skin_key_mode,
            self.play_skin_previous_best_ex_score(chart_id, &options),
            &self.boot.profile_config.display_name,
            skin_attempt,
        );
        self.spawn_play_skin_decode_for(
            play_skin_key_mode,
            options.session_mode,
            play_skin_runtime_state,
        );
        tracing::info!(
            course_id,
            entry_index,
            chart_id,
            preload_generation,
            "course next-stage preload started during intermediate result"
        );
    }

    /// コース曲間の中間リザルト状態かどうか。active_course を保持したまま
    /// finished_play だけが立ち、finished_course はまだ無い状態を指す。
    pub(super) fn is_course_intermediate_result(&self) -> bool {
        is_course_intermediate_result(
            self.play.active_course.is_some(),
            self.result.finished_course.is_some(),
            self.result.finished_play.is_some(),
        )
    }

    pub(super) fn course_intermediate_auto_advance_enabled(&self) -> bool {
        self.play
            .active_course
            .as_ref()
            .is_some_and(|course| course.auto_advance_intermediate_results)
    }

    /// コース曲間の中間リザルト画面を表示する。直前に終わった曲の結果を
    /// finished_play に入れて Result スキンを出すが、active_course は保持し
    /// finished_course は立てないので「中間リザルト」状態になる。
    pub(super) fn show_course_intermediate_result(&mut self) {
        let last = self
            .play
            .active_course
            .as_ref()
            .and_then(|course| course.entry_results.last())
            .map(|entry| entry.finished.clone());
        let Some(last) = last else {
            // 直前結果が無い異常系では中間リザルトを出さず、次の曲へ進む。
            self.start_next_course_chart();
            return;
        };
        self.result.result_gauge_graph_type = last.summary.gauge_type as i32;
        self.result.finished_play = Some(last);
        self.result.result_exit = None;
        self.result.result_key5_held = false;
        self.result.result_key7_held = false;
        self.result.result_scene_started_at = Instant::now();
        self.ensure_result_skin_ready_for_entry(ResultSkinSlot::Normal);
    }

    /// 中間リザルトを閉じて次の曲へ進む。finished_play をクリアして中間リザルト
    /// 状態を抜け、active_course はそのまま次の曲を開始する。
    pub(super) fn advance_to_next_course_chart(&mut self) {
        self.result.finished_play = None;
        self.result.result_exit = None;
        self.result.result_key5_held = false;
        self.result.result_key7_held = false;
        self.start_next_course_chart();
    }

    pub(super) fn autoplay_folder_has_next(&self) -> bool {
        self.select
            .autoplay_folder
            .as_ref()
            .is_some_and(|session| session.next_index < session.chart_ids.len())
    }

    pub(super) fn advance_autoplay_folder(&mut self) {
        let Some(session) = self.select.autoplay_folder.as_mut() else {
            self.leave_result();
            return;
        };
        let Some(&chart_id) = session.chart_ids.get(session.next_index) else {
            self.select.autoplay_folder = None;
            self.leave_result();
            return;
        };
        session.next_index += 1;
        self.result.finished_play = None;
        self.result.result_exit = None;
        self.result.result_key5_held = false;
        self.result.result_key7_held = false;
        let mut options = self.play_start_options();
        options.session_mode = SessionMode::Autoplay;
        options.autoplay = true;
        self.start_chart_with_options(chart_id, options);
        tracing::info!(chart_id, "advanced folder autoplay");
    }

    pub(super) fn finish_course_after_intermediate_result(&mut self) {
        self.result.finished_play = None;
        self.result.result_exit = None;
        self.result.result_key5_held = false;
        self.result.result_key7_held = false;
        self.finish_active_course();
    }

    /// コースの (current_index が指す) 次の曲を開始する。ゲージ持ち越しや
    /// replay / 同配置 arrange の適用は元の advance_course_after_finish と同じ。
    pub(super) fn start_next_course_chart(&mut self) {
        let Some((course_id, entry_index, chart_id, _)) = self.next_course_stage_start() else {
            return;
        };
        let launch_matches = self.play.pending_course_stage_launch.as_ref().is_some_and(|launch| {
            launch.matches(course_id, entry_index, chart_id)
                && launch.preload_generation == self.play.play_preload_generation
        });
        if !launch_matches {
            tracing::warn!(
                course_id,
                entry_index,
                chart_id,
                "course next-stage preload was unavailable; starting it at Play transition"
            );
            self.begin_next_course_chart_preload();
        }

        let Some(launch) = self.play.pending_course_stage_launch.as_ref().filter(|launch| {
            launch.matches(course_id, entry_index, chart_id)
                && launch.preload_generation == self.play.play_preload_generation
        }) else {
            tracing::error!(course_id, entry_index, chart_id, "failed to start course preload");
            return;
        };
        if let Some(error) = launch.preload_error.as_deref() {
            tracing::error!(course_id, entry_index, chart_id, error, "course preload failed");
            self.result.finished_play = None;
            self.abort_pending_play_start();
            return;
        }
        let options = launch.options.clone();
        self.begin_preloaded_play_scene(chart_id, options);
    }

    pub(super) fn course_intermediate_exit_action(&self) -> ResultExitAction {
        let Some(course) = self.play.active_course.as_ref() else {
            return ResultExitAction::FinishCourse;
        };
        let failed = course.entry_results.last().is_some_and(|entry| {
            entry.finished.result.clear_type == bmz_core::clear::ClearType::Failed
        });
        let has_next_chart = course.next_stage_start().is_some();
        course_intermediate_exit_action_for_state(failed, has_next_chart)
    }

    /// コース中間リザルトのコントロール処理。Key6 はゲージグラフ切替のみ許可し、
    /// それ以外の終了レーン (Key1-4/Key5/Key7) は retry せず次の曲へ進む。
    pub(super) fn handle_course_intermediate_control(
        &mut self,
        control: &PhysicalControl,
        pressed: bool,
        repeat: bool,
        synthesized_analog_axis: bool,
    ) -> bool {
        if self.handle_result_open_ir_control(control, pressed, repeat, false) {
            return true;
        }
        if self.handle_result_ir_scroll_control(control, pressed, repeat, synthesized_analog_axis) {
            return true;
        }
        if pressed
            && !repeat
            && self.result_input_ready()
            && self.result.result_panel == 1
            && self.result_ir_scope_toggle_is_e1()
            && self.is_result_ir_scope_toggle_control(control)
            && self.toggle_result_ir_scope()
        {
            return true;
        }
        if pressed
            && !repeat
            && self.result_input_ready()
            && self.select_result_panel_for_control(control)
        {
            return true;
        }
        if pressed
            && !repeat
            && self.result_input_ready()
            && self.is_result_panel_toggle_control(control)
            && self.toggle_result_panel()
        {
            return true;
        }
        let Some(lane) = self.result_lane_for_control(control) else {
            return false;
        };
        match lane {
            Lane::Key6 => {
                if pressed && !repeat && self.result_input_ready() {
                    self.cycle_result_gauge_graph_type();
                }
                true
            }
            lane if lane_starts_result_exit(lane) => {
                if pressed && self.result_input_ready() {
                    self.begin_result_exit(self.course_intermediate_exit_action());
                }
                true
            }
            _ => false,
        }
    }

    pub(super) fn advance_course_after_finish(&mut self, finished: FinishedPlaySession) {
        let Some(course) = &mut self.play.active_course else {
            return;
        };
        let chart_id = self.play.last_started_chart_id.unwrap_or(0);
        // Beatoraja behavior: if any chart in the course is Failed, the course
        // ends immediately and remaining charts are skipped.
        let failed = finished.result.clear_type == bmz_core::clear::ClearType::Failed;
        course.entry_results.push(CourseEntryResult { chart_id, finished });
        course.current_index += 1;

        let next_chart_id = course.next_stage_start().map(|(_, chart_id, _)| chart_id);
        let stage_limit = course
            .replay_stage_limit
            .unwrap_or(course.definition.entries.len())
            .min(course.definition.entries.len());
        let has_next_entry = course.current_index < stage_limit;

        if should_show_course_stage_result(failed, has_next_entry, next_chart_id.is_some()) {
            // 次の曲をすぐ始めず、まず直前の曲の単曲リザルト (中間リザルト) を出す。
            // active_course を保持したまま finished_play に直前結果を入れることで、
            // view_state は Result を返し、入力は中間リザルト分岐へ入る。実際の次曲
            // 開始 (ゲージ持ち越し / replay / 同配置 arrange の適用を含む) は、結果画面
            // を閉じたとき advance_to_next_course_chart まで遅延する。
            if !failed && next_chart_id.is_some() {
                self.begin_next_course_chart_preload();
            }
            self.show_course_intermediate_result();
            return;
        }

        self.finish_active_course();
    }
}

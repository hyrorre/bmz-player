use super::*;

impl WinitApp {
    /// Key5/Key7 の現在の押下状態を記録する。フェードアウト中も含めて
    /// 常に呼び、終了アニメーション終了時に retry arrange を決める。
    pub(super) fn track_result_lane_hold(&mut self, control: &PhysicalControl, pressed: bool) {
        match self.result_lane_for_control(control) {
            Some(Lane::Key5) => self.result.result_key5_held = pressed,
            Some(Lane::Key7) => self.result.result_key7_held = pressed,
            _ => {}
        }
    }

    pub(super) fn handle_result_control(
        &mut self,
        control: &PhysicalControl,
        pressed: bool,
        repeat: bool,
    ) -> bool {
        if self.handle_result_open_ir_control(control, pressed, repeat, false) {
            return true;
        }
        if self.handle_result_ir_scroll_control(control, pressed, repeat) {
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
            // ゲージグラフ種別の切り替え。
            Lane::Key6 => {
                if pressed && !repeat && self.result_input_ready() {
                    self.cycle_result_gauge_graph_type();
                }
                true
            }
            // Key1-4 / Key5 / Key7 の押下で終了アニメーションを開始する。
            // フェードアウト終了時の Key5/Key7 押下状態で retry か選曲へ戻るかを決める。
            lane if lane_starts_result_exit(lane) => {
                if pressed && self.result_input_ready() {
                    self.begin_result_exit(ResultExitAction::HeldLanes);
                }
                true
            }
            _ => false,
        }
    }

    pub(super) fn handle_course_result_control(
        &mut self,
        control: &PhysicalControl,
        pressed: bool,
        repeat: bool,
    ) -> bool {
        if self.handle_result_open_ir_control(control, pressed, repeat, true) {
            return true;
        }
        if self.handle_result_ir_scroll_control(control, pressed, repeat) {
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
                    self.begin_result_exit(ResultExitAction::HeldCourseLanes);
                }
                true
            }
            _ => false,
        }
    }

    pub(super) fn handle_result_open_ir_control(
        &mut self,
        control: &PhysicalControl,
        pressed: bool,
        repeat: bool,
        course_result: bool,
    ) -> bool {
        let Some(control_name) = physical_control_name(control) else {
            return false;
        };
        if !self.select.select_keys.is_open_ir(control_name) {
            return false;
        }
        if !pressed || repeat || !self.result_input_ready() {
            return true;
        }
        let identity = if course_result {
            PrimaryIrPageIdentity::Course {
                canonical_hash: self.result.finished_course_hash.clone(),
                rian_hash_v1: self.result.finished_course_rian_hash_v1.clone(),
                bms_ir_course_key: self.result.finished_course_bms_ir_key.clone(),
            }
        } else {
            let Some(finished) = &self.result.finished_play else {
                return true;
            };
            PrimaryIrPageIdentity::Chart { sha256: hash_to_hex(&finished.result.chart_sha256) }
        };
        self.open_primary_ir_page(identity);
        true
    }

    pub(super) fn handle_result_ir_scroll_control(
        &mut self,
        control: &PhysicalControl,
        pressed: bool,
        repeat: bool,
    ) -> bool {
        let Some(control_name) = physical_control_name(control) else {
            return false;
        };
        let Some(rows) = result_ir_scroll_rows_for_control(control_name, &self.select.select_keys)
        else {
            return false;
        };

        if !pressed {
            self.clear_result_ir_scroll_hold_control(control_name);
            return self.result_ir_scroll_interactive();
        }
        if !self.result_ir_scroll_interactive() {
            self.clear_result_ir_scroll_input();
            return false;
        }

        // アナログ軸の合成 Press は tick 比例スクロールと重複させない。
        if control_name.starts_with("Axis") || repeat {
            return true;
        }

        if self.scroll_result_ir_rows(rows) {
            self.start_result_ir_scroll_hold(rows, control_name);
        }
        true
    }

    pub(super) fn result_ir_scroll_interactive(&self) -> bool {
        if self.result.result_exit.is_some() || !self.result_input_ready() {
            return false;
        }
        let Some(document) = self.renderer.result_skin_document() else {
            return false;
        };
        self.result.result_ir.is_some()
            && result_ir_scroll_supported(document, self.result.result_panel)
    }

    pub(super) fn scroll_result_ir_rows(&mut self, rows: i32) -> bool {
        if !self.result_ir_scroll_interactive() {
            return false;
        }
        let changed = self
            .result
            .result_ir
            .as_mut()
            .is_some_and(|result_ir| result_ir.scroll_skin_rows(rows));
        if changed {
            self.play_system_sound(crate::system_sound::SoundType::Scratch);
        }
        changed
    }

    pub(super) fn start_result_ir_scroll_hold(&mut self, rows: i32, control: &str) {
        let now = Instant::now();
        let scroll = &mut self.result.result_ir_scroll;
        scroll.hold_rows = rows;
        scroll.hold_started_at = Some(now);
        scroll.hold_last_trigger_at = Some(now);
        scroll.hold_control = Some(control.to_string());
    }

    pub(super) fn advance_result_ir_scroll_hold(&mut self) {
        if !self.ui.focused || !self.result_ir_scroll_interactive() {
            self.clear_result_ir_scroll_input();
            return;
        }
        let scroll = &self.result.result_ir_scroll;
        let (rows, Some(started_at), Some(last_trigger_at)) =
            (scroll.hold_rows, scroll.hold_started_at, scroll.hold_last_trigger_at)
        else {
            return;
        };
        let now = Instant::now();
        if now.duration_since(started_at) < self.select_scroll_duration_low()
            || now.duration_since(last_trigger_at) < self.select_scroll_duration_high()
        {
            return;
        }
        self.result.result_ir_scroll.hold_last_trigger_at = Some(now);
        self.scroll_result_ir_rows(rows);
    }

    pub(super) fn clear_result_ir_scroll_hold_control(&mut self, control: &str) {
        if self.result.result_ir_scroll.hold_control.as_deref() == Some(control) {
            self.clear_result_ir_scroll_hold();
        }
    }

    pub(super) fn clear_result_ir_scroll_hold(&mut self) {
        let scroll = &mut self.result.result_ir_scroll;
        scroll.hold_rows = 0;
        scroll.hold_started_at = None;
        scroll.hold_last_trigger_at = None;
        scroll.hold_control = None;
    }

    pub(super) fn clear_result_ir_scroll_input(&mut self) {
        self.result.result_ir_scroll = ResultIrScrollRuntime::default();
    }

    pub(super) fn save_finished_play_replay_slot(&mut self, slot: u8) -> bool {
        let Some(finished) = self.result.finished_play.as_mut() else {
            return false;
        };
        let saved = match crate::storage::play_result::save_existing_replay_to_slot(
            &mut self.boot.score_db,
            &self.boot.profile_paths,
            &finished.result,
            &finished.stored,
            finished.ln_policy,
            finished.double_option,
            finished.rule_mode,
            slot,
        ) {
            Ok(Some(path)) => {
                finished.stored.slot_paths[slot as usize] = Some(path);
                finished.summary.saved_replay_slots[slot as usize] = true;
                finished.summary.replay_slots[slot as usize] = true;
                true
            }
            Ok(None) => false,
            Err(error) => {
                tracing::warn!(%error, slot, "failed to save replay slot from result");
                false
            }
        };
        if saved {
            self.notify_obs_save_recording(crate::obs::ObsRecordingSaveReason::OnReplay);
            self.play_system_sound(crate::system_sound::SoundType::OptionChange);
            tracing::info!(slot, "saved result replay slot");
        } else {
            tracing::info!(slot, "result replay slot was not saved");
        }
        saved
    }

    pub(super) fn save_finished_course_replay_slot(&mut self, slot: u8) -> bool {
        let Some(course_id) = self.result.finished_course.as_ref().map(|course| course.course_id)
        else {
            return false;
        };
        let Some(course_hash) = self.result.finished_course_hash.clone() else {
            tracing::warn!(course_id, slot, "course identity unavailable for replay slot save");
            return false;
        };
        let Some(course_score_id) =
            self.result.finished_course.as_ref().and_then(|course| course.course_score_id)
        else {
            tracing::info!(slot, "course replay slot unavailable without persisted course score");
            return false;
        };
        if slot > 3 {
            return false;
        }
        match self.boot.score_db.course_replay_attempt_is_complete(course_score_id) {
            Ok(true) => {}
            Ok(false) => {
                tracing::warn!(
                    course_id,
                    course_score_id,
                    slot,
                    "course replay slot unavailable because the saved replay is incomplete"
                );
                return false;
            }
            Err(error) => {
                tracing::warn!(
                    %error,
                    course_id,
                    course_score_id,
                    slot,
                    "failed to validate course replay before slot save"
                );
                return false;
            }
        }
        let Some(course) = self.result.finished_course.as_mut() else {
            return false;
        };
        let max_combo = course.course_max_combo;
        let clear_rank = course.final_clear_type as u8;
        let played_at = course.course_played_at.unwrap_or(0);
        let rule_mode = course.rule_mode;
        let record = crate::storage::score_db::CourseReplaySlotRecord {
            course_hash: course_hash.clone(),
            ln_policy: course.ln_policy,
            rule_mode,
            slot,
            rule: crate::config::profile_config::ReplaySlotRule::Always.as_str().to_string(),
            course_score_id,
            played_at,
            ex_score: course.total_ex_score,
            bp: course.bp,
            max_combo,
            clear_rank,
        };
        match self.boot.score_db.upsert_course_replay_slot(&record) {
            Ok(()) => {
                mark_course_replay_slot_saved(
                    course,
                    self.result.finished_course_skin_summary.as_mut(),
                    slot as usize,
                );
                self.notify_obs_save_recording(crate::obs::ObsRecordingSaveReason::OnReplay);
                self.play_system_sound(crate::system_sound::SoundType::OptionChange);
                tracing::info!(
                    course_id,
                    course_hash = %course_hash,
                    rule_mode = rule_mode.as_str(),
                    slot,
                    "saved course replay slot"
                );
                true
            }
            Err(error) => {
                tracing::warn!(
                    %error,
                    course_id,
                    course_hash = %course_hash,
                    slot,
                    "failed to save course replay slot from result"
                );
                false
            }
        }
    }

    pub(super) fn result_lane_for_control(&self, control: &PhysicalControl) -> Option<Lane> {
        if let Some(control_name) = physical_control_name(control)
            && let Some(lane) = self.select.select_keys.ui_lane_for_control(control_name)
        {
            return Some(lane);
        }
        let key_mode = self.result.finished_play.as_ref()?.summary.key_mode;
        crate::config::play::lane_binding_for_chart(&self.boot.profile_config.input, key_mode)
            .resolve(DeviceId(0), control)
    }

    pub(super) fn is_result_panel_toggle_control(&self, control: &PhysicalControl) -> bool {
        physical_control_name(control).is_some_and(|control| {
            control == "Select" || self.select.select_keys.is_e2_action(control)
        })
    }

    pub(super) fn select_result_panel_for_control(&mut self, control: &PhysicalControl) -> bool {
        result_panel_for_control(control).is_some_and(|panel| self.set_result_panel(panel))
    }

    pub(super) fn toggle_result_panel(&mut self) -> bool {
        let Some(document) = self.renderer.result_skin_document() else {
            return false;
        };
        let Some(requested) = toggled_result_panel(
            self.result.result_panel,
            result_panel_supported(document),
            self.result.result_ir.is_some(),
        ) else {
            return false;
        };
        self.set_result_panel(requested)
    }

    pub(super) fn set_result_panel(&mut self, requested: i32) -> bool {
        let Some(document) = self.renderer.result_skin_document() else {
            return false;
        };
        let Some(panel) = selected_result_panel(
            self.result.result_panel,
            requested,
            result_panel_supported(document),
            self.result.result_ir.is_some(),
        ) else {
            return false;
        };
        self.result.result_panel = panel;
        self.clear_result_ir_scroll_input();
        tracing::info!(panel = self.result.result_panel, "result panel changed");
        self.play_system_sound(crate::system_sound::SoundType::OptionChange);
        true
    }

    /// `resultIrScopeBinding=active` を宣言したスキンだけが Result IR scope を切り替える。
    /// 既存スキンの standard IR ref は常に global のままにする。
    pub(super) fn select_result_ir_scope(
        &mut self,
        tab: crate::screens::result_ir::ResultRankingTab,
    ) -> bool {
        let Some(document) = self.renderer.result_skin_document() else {
            return false;
        };
        if document.result_ir_scope_binding != bmz_render::skin::ResultIrScopeBinding::Active {
            return false;
        }
        let Some(result_ir) = &mut self.result.result_ir else {
            return false;
        };
        if !result_ir.supports_tab(tab) || result_ir.active_tab == tab {
            return false;
        }
        result_ir.select_tab(tab);
        self.clear_result_ir_scroll_input();
        self.play_system_sound(crate::system_sound::SoundType::OptionChange);
        true
    }

    pub(super) fn toggle_result_ir_scope(&mut self) -> bool {
        let Some(document) = self.renderer.result_skin_document() else {
            return false;
        };
        if document.result_ir_scope_binding != bmz_render::skin::ResultIrScopeBinding::Active {
            return false;
        }
        let Some(result_ir) = &self.result.result_ir else {
            return false;
        };
        let next = match result_ir.active_tab {
            crate::screens::result_ir::ResultRankingTab::Global => {
                crate::screens::result_ir::ResultRankingTab::SelfAndRivals
            }
            crate::screens::result_ir::ResultRankingTab::SelfAndRivals => {
                crate::screens::result_ir::ResultRankingTab::Global
            }
        };
        self.select_result_ir_scope(next)
    }

    pub(super) fn result_ir_scope_toggle_is_e1(&self) -> bool {
        self.renderer.result_skin_document().is_some_and(|document| {
            document.result_ir_scope_binding == bmz_render::skin::ResultIrScopeBinding::Active
                && document.result_ir_scope_toggle == bmz_render::skin::ResultIrScopeToggle::E1Press
        })
    }

    pub(super) fn is_result_ir_scope_toggle_control(&self, control: &PhysicalControl) -> bool {
        physical_control_name(control).is_some_and(|name| {
            self.select.select_keys.is_start(name)
                || self.select.select_keys.e_action_for_control(name) == Some(InputActionConfig::E1)
        })
    }

    /// `selectIrScopeBinding=active` を宣言したスキンだけが Select IR scope を切り替える。
    /// 既存スキンの standard IR ref は常に global のままにする。
    pub(super) fn select_select_ir_scope(
        &mut self,
        scope: crate::screens::select_ir::SelectIrRankingScope,
    ) -> bool {
        let Some(document) = self.renderer.select_skin_document() else {
            return false;
        };
        if document.select_ir_scope_binding != bmz_render::skin::IrScopeBinding::Active {
            return false;
        }
        if !self.select.select_ir.select_scope(
            &self.boot.profile_config.ir,
            self.selected_chart_sha256(),
            scope,
        ) {
            return false;
        }
        self.play_system_sound(crate::system_sound::SoundType::OptionChange);
        true
    }

    pub(super) fn toggle_select_ir_scope(&mut self) -> bool {
        let Some(document) = self.renderer.select_skin_document() else {
            return false;
        };
        if document.select_ir_scope_binding != bmz_render::skin::IrScopeBinding::Active {
            return false;
        }
        if !self
            .select
            .select_ir
            .toggle_scope(&self.boot.profile_config.ir, self.selected_chart_sha256())
        {
            return false;
        }
        self.play_system_sound(crate::system_sound::SoundType::OptionChange);
        true
    }

    pub(super) fn select_ir_scope_toggle_is_e3(&self) -> bool {
        self.renderer.select_skin_document().is_some_and(|document| {
            document.select_ir_scope_binding == bmz_render::skin::IrScopeBinding::Active
                && document.select_ir_scope_toggle == bmz_render::skin::SelectIrScopeToggle::E3Press
        })
    }

    pub(super) fn is_select_ir_scope_toggle_control(&self, control: &str) -> bool {
        self.select.select_keys.e_action_for_control(control) == Some(InputActionConfig::E3)
    }

    pub(super) fn cycle_result_gauge_graph_type(&mut self) {
        self.result.result_gauge_graph_type =
            cycle_result_gauge_graph_type(self.result.result_gauge_graph_type);
        tracing::info!(
            gauge_type = self.result.result_gauge_graph_type,
            "result gauge graph type changed"
        );
        self.play_system_sound(crate::system_sound::SoundType::OptionChange);
    }
}

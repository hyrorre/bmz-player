use super::*;

impl WinitApp {
    pub(super) fn reset_selected_replay_slot(&mut self) {
        self.select.selected_replay_slot =
            normalize_replay_slot(self.selected_item_replay_slots(), None);
    }

    pub(super) fn normalize_selected_replay_slot(&mut self) {
        let slots = self.selected_item_replay_slots();
        self.select.selected_replay_slot =
            normalize_replay_slot(slots, self.select.selected_replay_slot);
    }

    pub(super) fn selected_replay_slot_for_selected(&self) -> Option<u8> {
        normalize_replay_slot(self.selected_item_replay_slots(), self.select.selected_replay_slot)
    }

    pub(super) fn cycle_selected_replay_slot(&mut self, direction: i32) -> bool {
        let slots = self.selected_item_replay_slots();
        let current = self.select.selected_replay_slot;
        let next = cycle_replay_slot(slots, current, direction);
        self.select.selected_replay_slot = next;
        if next == current {
            return false;
        }
        let text = Localizer::new(self.boot.profile_config.ui.locale());
        if let Some(slot) = next {
            let mut args = FluentArgs::new();
            args.set("slot", i64::from(slot) + 1);
            self.show_left_overlay_toast(text.format("toast-select-replay-slot", &args));
            self.play_system_sound(crate::system_sound::SoundType::OptionChange);
            tracing::info!(slot, "selected replay slot changed");
            true
        } else {
            self.show_left_overlay_toast(text.text("toast-select-replay-unavailable"));
            false
        }
    }

    pub(super) fn start_selected_replay_slot(&mut self) -> bool {
        let Some(slot) = self.selected_replay_slot_for_selected() else {
            self.show_left_overlay_toast(
                Localizer::new(self.boot.profile_config.ui.locale())
                    .text("toast-select-replay-unavailable"),
            );
            return false;
        };
        self.select.selected_replay_slot = Some(slot);
        self.start_replay_for_selected(slot)
    }

    fn selected_item_replay_slots(&self) -> [bool; 4] {
        match self.select.select_items.get(self.select.selected_index) {
            Some(SelectItem::Chart(_)) => self.selected_chart_replay_slots(),
            Some(item) => replay_slots_for_item(item),
            None => [false; 4],
        }
    }

    pub(super) fn selected_chart_replay_slots(&self) -> [bool; 4] {
        let Some(row) = self.selected_chart_row() else {
            return [false; 4];
        };
        let Some(chart) = row.chart.as_ref() else {
            return [false; 4];
        };
        let key_mode = KeyMode::from_str_opt(&chart.mode).unwrap_or_default();
        let key = crate::storage::score_db::ScoreKey::with_options(
            chart.sha256,
            crate::ln_policy::score_ln_policy(
                self.boot.profile_config.play.ln_mode_policy,
                chart.ln_profile,
            ),
            self.select.double_option.normalize_for_key_mode(key_mode).score_bucket(),
            self.boot.profile_config.play.rule_mode,
        );
        if let Some((cached_key, slots)) = *self.select.replay_slot_cache.borrow()
            && cached_key == key
        {
            return slots;
        }
        let slots = self
            .boot
            .score_db
            .replay_slots_for_chart(key)
            .map(|slots| slots.map(|slot| slot.is_some()))
            .unwrap_or_else(|error| {
                tracing::warn!(%error, "failed to load replay slots for selected score bucket");
                [false; 4]
            });
        self.select.replay_slot_cache.replace(Some((key, slots)));
        slots
    }

    pub(super) fn try_start_replay_from_file(&mut self, path: &std::path::Path) -> bool {
        let replay_file = match crate::storage::replay::load_replay(path) {
            Ok(file) => file,
            Err(error) => {
                tracing::warn!(%error, path = %path.display(), "replay file load failed");
                return false;
            }
        };
        let Ok(sha) = crate::storage::common::hex_to_hash::<32>(&replay_file.chart_sha256) else {
            tracing::warn!(sha = %replay_file.chart_sha256, "replay file has invalid chart sha256");
            return false;
        };
        let Some(chart_id) = self.boot.library_db.chart_id_by_sha256(sha).ok().flatten() else {
            tracing::warn!(
                sha = %replay_file.chart_sha256,
                "replay chart is not in the library; load the song first"
            );
            return false;
        };
        let s_random_scheme = match replay_file.effective_s_random_scheme() {
            Ok(scheme) => scheme,
            Err(error) => {
                tracing::warn!(%error, path = %path.display(), "replay arrangement scheme is unsupported");
                return false;
            }
        };
        let s_random_scheme_2p = match replay_file.effective_s_random_scheme_2p() {
            Ok(scheme) => Some(scheme),
            Err(error) => {
                tracing::warn!(%error, path = %path.display(), "replay 2P arrangement scheme is unsupported");
                return false;
            }
        };
        let player = replay_file.player();
        let options = PlayStartOptions {
            session_mode: SessionMode::Normal,
            autoplay: false,
            key_mode_conversion: self.boot.profile_config.play.key_mode_conversion,
            seven_to_nine_pattern: self.boot.profile_config.play.seven_to_nine_pattern,
            seven_to_nine_type: self.boot.profile_config.play.seven_to_nine_type,
            seven_to_nine_rule_mode: self.boot.profile_config.play.seven_to_nine_rule_mode,
            score_save_disabled: false,
            playback_rate_percent: 100,
            assist: Default::default(),
            replay_player: Some(player),
            battle_target: None,
            chart_zero_time: TimeUs(0),
            gauge: Some(self.select.gauge_option),
            replay_gauge_override: replay_file.recorded_gauge_type(),
            gauge_auto_shift: self.select.gauge_auto_shift_option,
            bottom_shiftable_gauge: self.select.bottom_shiftable_gauge_option,
            arrange: replay_file.arrange_option(),
            arrange_2p: replay_file.arrange_2p_option(),
            double_option: replay_file.double_option(),
            hs_fix: self.select.hs_fix_option,
            target: self.select.target_option,
            resolved_target: None,
            rival_name: self.select.select_ir.active_rival_display_name().map(str::to_string),
            arrange_seed: replay_file.arrange_seed,
            arrange_seed_2p: replay_file.arrange_seed_2p,
            random_trainer_seed: None,
            legacy_arrange_seed: replay_file.uses_legacy_seed_scheme(),
            s_random_scheme,
            s_random_scheme_2p,
            h_random_threshold_ms: replay_file.h_random_threshold_ms,
            bms_random_seed: None,
            bms_random_choices: replay_file.bms_random_choices.clone(),
            bms_switch_choices: replay_file.bms_switch_choices.clone(),
            arrange_pattern: replay_file.lane_shuffle_pattern.clone(),
            initial_gauge_value: None,
            initial_gauge_values: None,
            initial_course_combo: None,
            judge_constraint: bmz_core::course::CourseJudgeConstraint::Normal,
            speed_constraint: bmz_core::course::CourseSpeedConstraint::Free,
            ln_mode_override: None,
            course_gauge_override: None,
            course_gauge_property_override: None,
        };
        self.start_chart_with_options(chart_id, options);
        true
    }

    pub(super) fn start_replay_chart_with_options(
        &mut self,
        chart_id: i64,
        options: PlayStartOptions,
        show_decide: bool,
    ) {
        if show_decide {
            self.begin_decide_for_chart(chart_id, options);
        } else {
            self.start_chart_with_options(chart_id, options);
        }
    }

    pub(super) fn try_start_replay_for_chart(
        &mut self,
        chart_id: i64,
        slot: u8,
        show_decide: bool,
    ) -> bool {
        let chart = match crate::screens::play_session::load_source_chart_for_chart(
            &self.boot.library_db,
            chart_id,
            None,
        ) {
            Ok(chart) => chart,
            Err(error) => {
                tracing::warn!(chart_id, %error, "replay start failed: source chart load failed");
                return false;
            }
        };
        let sha = chart.identity.file_sha256;
        let key_mode = chart.metadata.key_mode;
        let key = crate::storage::score_db::ScoreKey::with_options(
            sha,
            crate::ln_policy::score_ln_policy_for_chart(
                self.boot.profile_config.play.ln_mode_policy,
                &chart,
            ),
            self.select.double_option.normalize_for_key_mode(key_mode).score_bucket(),
            self.boot.profile_config.play.rule_mode,
        );
        let Some(slot_record) = self.boot.score_db.replay_slot(key, slot).ok().flatten() else {
            tracing::info!(slot, "no replay saved for slot");
            return false;
        };
        let abs_path = self.boot.profile_paths.root_dir.join(&slot_record.replay_path);
        let replay_file = match load_replay_for_chart_policy_and_double_option(
            &abs_path,
            sha,
            slot_record.ln_policy,
            slot_record.double_option,
        ) {
            Ok(file) => file,
            Err(error) => {
                tracing::warn!(%error, path = %abs_path.display(), "replay load failed");
                return false;
            }
        };
        let s_random_scheme = match replay_file.effective_s_random_scheme() {
            Ok(scheme) => scheme,
            Err(error) => {
                tracing::warn!(%error, path = %abs_path.display(), "replay arrangement scheme is unsupported");
                return false;
            }
        };
        let s_random_scheme_2p = match replay_file.effective_s_random_scheme_2p() {
            Ok(scheme) => Some(scheme),
            Err(error) => {
                tracing::warn!(%error, path = %abs_path.display(), "replay 2P arrangement scheme is unsupported");
                return false;
            }
        };
        let player = replay_file.player();
        let options = PlayStartOptions {
            session_mode: SessionMode::Normal,
            autoplay: false,
            key_mode_conversion: self.boot.profile_config.play.key_mode_conversion,
            seven_to_nine_pattern: self.boot.profile_config.play.seven_to_nine_pattern,
            seven_to_nine_type: self.boot.profile_config.play.seven_to_nine_type,
            seven_to_nine_rule_mode: self.boot.profile_config.play.seven_to_nine_rule_mode,
            score_save_disabled: false,
            playback_rate_percent: 100,
            assist: Default::default(),
            replay_player: Some(player),
            battle_target: None,
            chart_zero_time: TimeUs(0),
            gauge: Some(self.select.gauge_option),
            replay_gauge_override: replay_file.recorded_gauge_type(),
            gauge_auto_shift: self.select.gauge_auto_shift_option,
            bottom_shiftable_gauge: self.select.bottom_shiftable_gauge_option,
            arrange: replay_file.arrange_option(),
            arrange_2p: replay_file.arrange_2p_option(),
            double_option: replay_file.double_option(),
            hs_fix: self.select.hs_fix_option,
            target: self.select.target_option,
            resolved_target: None,
            rival_name: self.select.select_ir.active_rival_display_name().map(str::to_string),
            arrange_seed: replay_file.arrange_seed,
            arrange_seed_2p: replay_file.arrange_seed_2p,
            random_trainer_seed: None,
            legacy_arrange_seed: replay_file.uses_legacy_seed_scheme(),
            s_random_scheme,
            s_random_scheme_2p,
            h_random_threshold_ms: replay_file.h_random_threshold_ms,
            bms_random_seed: None,
            bms_random_choices: replay_file.bms_random_choices.clone(),
            bms_switch_choices: replay_file.bms_switch_choices.clone(),
            arrange_pattern: replay_file.lane_shuffle_pattern.clone(),
            initial_gauge_value: None,
            initial_gauge_values: None,
            initial_course_combo: None,
            judge_constraint: bmz_core::course::CourseJudgeConstraint::Normal,
            speed_constraint: bmz_core::course::CourseSpeedConstraint::Free,
            ln_mode_override: None,
            course_gauge_override: None,
            course_gauge_property_override: None,
        };
        self.start_replay_chart_with_options(chart_id, options, show_decide);
        true
    }

    pub(super) fn start_replay_for_selected(&mut self, slot: u8) -> bool {
        if self.select.course_builder.is_some() {
            self.show_select_course_builder_chart_required();
            return false;
        }
        if let Some(chart_id) = self.currently_selected_chart_id() {
            return self.try_start_replay_for_chart(chart_id, slot, true);
        }
        if let Some(course_id) = self.currently_selected_course_id() {
            return self.try_start_course_replay_for_slot(course_id, slot);
        }
        false
    }

    pub(super) fn currently_selected_chart_id(&self) -> Option<i64> {
        match self.select.select_items.get(self.select.selected_index)? {
            SelectItem::Chart(row) => row.chart.as_ref().map(|chart| chart.chart_id),
            SelectItem::Folder { .. }
            | SelectItem::Course(_)
            | SelectItem::Executable(_)
            | SelectItem::Config(_)
            | SelectItem::AppConfig(_)
            | SelectItem::KeyBinding(_)
            | SelectItem::SettingsBack
            | SelectItem::SettingsClose
            | SelectItem::AdvancedSettings
            | SelectItem::ApplyAudioSettings => None,
        }
    }

    pub(super) fn currently_selected_course_id(&self) -> Option<i64> {
        match self.select.select_items.get(self.select.selected_index)? {
            SelectItem::Course(row) => Some(row.course_id),
            SelectItem::Chart(_)
            | SelectItem::Folder { .. }
            | SelectItem::Executable(_)
            | SelectItem::Config(_)
            | SelectItem::AppConfig(_)
            | SelectItem::KeyBinding(_)
            | SelectItem::SettingsBack
            | SelectItem::SettingsClose
            | SelectItem::AdvancedSettings
            | SelectItem::ApplyAudioSettings => None,
        }
    }

    pub(super) fn try_start_course_replay_for_slot(&mut self, course_id: i64, slot: u8) -> bool {
        let Some((stored, identity)) = self.course_identity_with_stored(course_id) else {
            tracing::warn!(course_id, slot, "course identity unavailable for replay slot");
            return false;
        };
        let ln_policy =
            match crate::screens::select_model::normalized_course_ln_policy_for_definition(
                &self.boot.library_db,
                &stored.definition,
                self.boot.profile_config.play.ln_mode_policy,
            ) {
                Ok(policy) => policy,
                Err(error) => {
                    tracing::warn!(%error, course_id, slot, "course LN policy unavailable for replay slot");
                    return false;
                }
            };
        let rule_mode = self.boot.profile_config.play.rule_mode;
        match self.boot.score_db.course_replay_slot(
            &identity.course_hash,
            ln_policy,
            rule_mode,
            slot,
        ) {
            Ok(Some(record)) => {
                tracing::info!(
                    course_id,
                    course_hash = %identity.course_hash,
                    rule_mode = rule_mode.as_str(),
                    course_score_id = record.course_score_id,
                    slot,
                    "starting course replay from select"
                );
                self.start_course_replay(course_id, record.course_score_id);
                true
            }
            Ok(None) => {
                tracing::info!(
                    course_id,
                    course_hash = %identity.course_hash,
                    rule_mode = rule_mode.as_str(),
                    slot,
                    "no saved course attempt in this replay slot"
                );
                false
            }
            Err(error) => {
                tracing::error!(
                    %error,
                    course_id,
                    course_hash = %identity.course_hash,
                    rule_mode = rule_mode.as_str(),
                    slot,
                    "failed to look up course_replay_slot"
                );
                false
            }
        }
    }
}

pub(super) fn first_replay_slot_for_item(item: &SelectItem) -> Option<u8> {
    normalize_replay_slot(replay_slots_for_item(item), None)
}

fn replay_slots_for_item(item: &SelectItem) -> [bool; 4] {
    match item {
        SelectItem::Chart(row) => row.replay_slots,
        SelectItem::Course(row) => row.replay_slots,
        _ => [false; 4],
    }
}

pub(super) fn normalize_replay_slot(slots: [bool; 4], selected: Option<u8>) -> Option<u8> {
    selected
        .filter(|slot| slots.get(usize::from(*slot)).copied().unwrap_or(false))
        .or_else(|| slots.iter().position(|exists| *exists).map(|slot| slot as u8))
}

pub(super) fn cycle_replay_slot(
    slots: [bool; 4],
    selected: Option<u8>,
    direction: i32,
) -> Option<u8> {
    if !selected.is_some_and(|slot| slots.get(usize::from(slot)).copied().unwrap_or(false)) {
        return if direction >= 0 {
            slots.iter().position(|exists| *exists).map(|slot| slot as u8)
        } else {
            slots.iter().rposition(|exists| *exists).map(|slot| slot as u8)
        };
    }
    let current = usize::from(selected.unwrap());
    for distance in 1..4 {
        let slot =
            if direction >= 0 { (current + distance) % 4 } else { (current + 4 - distance) % 4 };
        if slots[slot] {
            return Some(slot as u8);
        }
    }
    selected
}

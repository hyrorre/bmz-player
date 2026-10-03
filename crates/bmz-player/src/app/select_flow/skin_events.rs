use super::*;
use crate::config::profile_config::SevenToNinePattern;

impl WinitApp {
    pub(super) fn execute_select_skin_event(&mut self, event_id: i32, arg: i32) {
        if (bmz_render::skin::LR2_BUTTON_BASE..=bmz_render::skin::LR2_BUTTON_LAST)
            .contains(&event_id)
        {
            self.execute_lr2_select_button(event_id - bmz_render::skin::LR2_BUTTON_BASE, arg);
            return;
        }
        match event_id {
            // beatoraja EventFactory: difficulty / skin config / documents.
            10 => self.cycle_select_difficulty_filter(arg),
            14 => self.open_skin_settings_from_select(),
            17 => self.open_selected_chart_documents(),
            SKIN_EVENT_IR_SCOPE_GLOBAL => {
                self.select_select_ir_scope(
                    crate::screens::select_ir::SelectIrRankingScope::Global,
                );
            }
            SKIN_EVENT_IR_SCOPE_RIVAL => {
                self.select_select_ir_scope(
                    crate::screens::select_ir::SelectIrRankingScope::SelfAndRivals,
                );
            }
            SKIN_EVENT_IR_SCOPE_TOGGLE => {
                self.toggle_select_ir_scope();
            }
            SKIN_EVENT_DAILY_STATISTICS_RESET => self.reset_daily_statistics(),
            // beatoraja EventFactory: open key configuration.
            13 => self.open_key_config_from_select(),
            // beatoraja EventFactory: play / autoplay / practice.
            15 => {
                self.set_session_mode(SessionMode::Normal);
                self.enter_or_play_selected();
            }
            16 => {
                self.set_session_mode(SessionMode::Autoplay);
                self.enter_or_play_selected();
            }
            315 => {
                self.set_session_mode(SessionMode::Practice);
                self.enter_or_play_selected();
            }
            // beatoraja event 19 starts the currently selected replay slot.
            19 => {
                if !self.start_selected_replay_slot() {
                    tracing::info!("select skin replay click ignored; no replay is selected");
                }
            }
            316..=318 => {
                let slot = (event_id - 315) as u8;
                if !self.start_replay_for_selected(slot) {
                    tracing::info!(slot, "select skin replay click ignored; slot is empty");
                }
            }
            11 => self.cycle_select_mode_filter(arg),
            12 => self.cycle_select_sort(arg),
            40 => self.cycle_select_gauge(arg),
            42 => self.cycle_select_arrange(arg),
            43 => self.cycle_select_arrange_2p(arg),
            54 => self.cycle_select_double_option(arg),
            55 => self.cycle_select_hs_fix(arg),
            57 => self.adjust_select_hispeed(arg),
            59 => self.adjust_select_note_display_duration(arg),
            72 => self.cycle_select_bga(arg),
            73 => self.cycle_select_bga_expand(arg),
            74 => {
                if self.adjust_visual_offset_ms(if arg >= 0 { 1 } else { -1 }) {
                    self.play_system_sound(crate::system_sound::SoundType::OptionChange);
                }
            }
            75 => {
                self.toggle_visual_offset_auto_adjust();
                self.play_system_sound(crate::system_sound::SoundType::OptionChange);
            }
            77 => self.cycle_select_target(arg),
            79 => self.cycle_active_rival(arg),
            78 => self.cycle_select_gauge_auto_shift(arg),
            89 => self.toggle_favorite_song_selected(),
            90 => self.toggle_favorite_chart_selected(),
            210 => self.open_primary_ir_for_selected(),
            211 => self.reload_from_select_context(),
            212 => self.open_selected_chart_folder(),
            213 => self.open_selected_chart_download_sites(),
            341 => self.cycle_select_bottom_shiftable_gauge(arg),
            340 => self.cycle_select_judge_algorithm(arg),
            308 => self.cycle_select_ln_mode(arg),
            309 => self.cycle_select_difficulty_filter(arg),
            260..=266 => self.adjust_random_mix_skin_option(event_id, arg),
            301..=307 => {
                if self.boot.profile_config.play.assist.toggle_beatoraja_button(event_id) {
                    self.boot.profile_config.updated_at = now_unix_seconds();
                    self.invalidate_play_preload();
                    self.play_system_sound(crate::system_sound::SoundType::OptionChange);
                }
            }
            312 => {
                // BMZ only exposes beatoraja's default sorter set for now.
                self.cycle_select_sort(arg);
            }
            321..=324 => self.cycle_replay_slot_rule(event_id, arg),
            330 => {
                if !self.begin_selected_play_mode_edit() {
                    return;
                }
                self.boot.profile_config.play.lane_effect =
                    toggled_select_sudden(self.boot.profile_config.play.lane_effect);
                self.finish_selected_play_mode_edit();
                self.play_system_sound(crate::system_sound::SoundType::OptionChange);
            }
            331 => {
                if !self.begin_selected_play_mode_edit() {
                    return;
                }
                self.boot.profile_config.lane.lift_enabled =
                    !self.boot.profile_config.lane.lift_enabled;
                self.finish_selected_play_mode_edit();
                self.play_system_sound(crate::system_sound::SoundType::OptionChange);
            }
            332 => {
                if !self.begin_selected_play_mode_edit() {
                    return;
                }
                self.boot.profile_config.play.lane_effect =
                    toggled_select_hidden(self.boot.profile_config.play.lane_effect);
                self.finish_selected_play_mode_edit();
                self.play_system_sound(crate::system_sound::SoundType::OptionChange);
            }
            342 => {
                if !self.begin_selected_play_mode_edit() {
                    return;
                }
                self.boot.profile_config.lane.hispeed_auto_adjust =
                    !self.boot.profile_config.lane.hispeed_auto_adjust;
                self.finish_selected_play_mode_edit();
                self.play_system_sound(crate::system_sound::SoundType::OptionChange);
            }
            344 => self.cycle_chart_replication_mode(arg),
            343 => {
                self.boot.profile_config.play.guide_se = !self.boot.profile_config.play.guide_se;
                self.boot.profile_config.updated_at = now_unix_seconds();
                self.invalidate_play_preload();
                self.play_system_sound(crate::system_sound::SoundType::OptionChange);
            }
            350 => self.cycle_extra_note_depth(arg),
            351 => self.cycle_assist_mine_mode(arg),
            352 => self.cycle_assist_scroll_mode(arg),
            353 => self.cycle_assist_long_note_mode(arg),
            360 => self.cycle_seven_to_nine_pattern(arg),
            361 => self.cycle_seven_to_nine_type(arg),
            400 => self.toggle_select_constant(),
            _ => {
                tracing::debug!(event_id, arg, "unsupported select skin event");
            }
        }
    }

    pub(super) fn reset_daily_statistics(&mut self) {
        match self.boot.score_db.reset_daily_statistics(now_unix_seconds()) {
            Ok(()) => {
                self.refresh_player_stats_snapshot();
                self.play_system_sound(crate::system_sound::SoundType::OptionChange);
            }
            Err(error) => tracing::warn!(%error, "failed to reset daily statistics"),
        }
    }

    pub(super) fn cycle_select_mode_filter(&mut self, arg: i32) {
        self.select.select_mode_filter = if arg >= 0 {
            self.select.select_mode_filter.next()
        } else {
            self.select.select_mode_filter.previous()
        };
        // reload_select_items 内で beatoraja 準拠の自動送りと profile config への
        // 永続化（退出 / プレイ後の save_current_play_options 用）を行う。
        let previous_len = self.select.select_items.len();
        self.reload_select_items();
        tracing::info!(
            mode = self.select.select_mode_filter.as_str(),
            previous_len,
            current_len = self.select.select_items.len(),
            "select mode filter changed"
        );
        self.play_system_sound(crate::system_sound::SoundType::OptionChange);
    }

    pub(super) fn cycle_select_difficulty_filter(&mut self, arg: i32) {
        self.select.select_difficulty_filter = if arg >= 0 {
            self.select.select_difficulty_filter.next()
        } else {
            self.select.select_difficulty_filter.previous()
        };
        self.boot.profile_config.select.difficulty_filter =
            self.select.select_difficulty_filter.as_str().to_string();
        self.reload_select_items();
        tracing::info!(
            difficulty = self.select.select_difficulty_filter.as_str(),
            "select difficulty filter changed"
        );
        self.play_system_sound(crate::system_sound::SoundType::OptionChange);
    }

    pub(super) fn cycle_select_gauge(&mut self, arg: i32) {
        self.select.gauge_option = cycle_gauge_option_with_direction(self.select.gauge_option, arg);
        tracing::info!(gauge = ?self.select.gauge_option, "gauge option changed");
        self.play_system_sound(crate::system_sound::SoundType::OptionChange);
    }

    pub(super) fn cycle_select_arrange(&mut self, arg: i32) {
        self.select.arrange_option =
            cycle_arrange_option_with_direction(self.select.arrange_option, arg);
        tracing::info!(arrange = self.select.arrange_option.as_str(), "arrange option changed");
        self.play_system_sound(crate::system_sound::SoundType::OptionChange);
    }

    pub(super) fn cycle_select_arrange_2p(&mut self, arg: i32) {
        self.select.arrange_option_2p =
            cycle_arrange_option_with_direction(self.select.arrange_option_2p, arg);
        tracing::info!(arrange_2p = self.select.arrange_option_2p.as_str(), "2P arrange changed");
        self.play_system_sound(crate::system_sound::SoundType::OptionChange);
    }

    pub(super) fn cycle_select_double_option(&mut self, arg: i32) {
        self.select.double_option =
            cycle_double_option_with_direction(self.select.double_option, arg);
        if matches!(
            self.select.double_option,
            DoubleOption::Battle | DoubleOption::BattleAutoScratch
        ) && self.boot.profile_config.play.key_mode_conversion != KeyModeConversionConfig::Off
        {
            self.boot.profile_config.play.key_mode_conversion = KeyModeConversionConfig::Off;
            self.boot.profile_config.updated_at = now_unix_seconds();
            self.invalidate_play_preload();
            self.play.play_media_cache = None;
        }
        tracing::info!(double_option = self.select.double_option.as_str(), "double option changed");
        self.play_system_sound(crate::system_sound::SoundType::OptionChange);
    }

    pub(super) fn cycle_seven_to_nine_pattern(&mut self, arg: i32) {
        let current = if self.boot.profile_config.play.key_mode_conversion
            == KeyModeConversionConfig::SevenToNine
        {
            self.boot.profile_config.play.seven_to_nine_pattern.value()
        } else {
            0
        };
        let next = if arg >= 0 {
            (current + 1) % 7
        } else if current == 0 {
            6
        } else {
            current - 1
        };
        if next == 0 {
            self.boot.profile_config.play.key_mode_conversion = KeyModeConversionConfig::Off;
        } else if let Ok(pattern) = SevenToNinePattern::try_from(next) {
            self.boot.profile_config.play.key_mode_conversion =
                KeyModeConversionConfig::SevenToNine;
            self.boot.profile_config.play.seven_to_nine_pattern = pattern;
            self.boot.profile_config.play.double_option = DoubleOptionConfig::Off;
            self.select.double_option = DoubleOption::Off;
        }
        self.boot.profile_config.updated_at = now_unix_seconds();
        self.invalidate_play_preload();
        self.play.play_media_cache = None;
        self.play_system_sound(crate::system_sound::SoundType::OptionChange);
    }

    pub(super) fn cycle_seven_to_nine_type(&mut self, arg: i32) {
        self.boot.profile_config.play.seven_to_nine_type =
            self.boot.profile_config.play.seven_to_nine_type.next(arg >= 0);
        self.boot.profile_config.updated_at = now_unix_seconds();
        self.invalidate_play_preload();
        self.play.play_media_cache = None;
        self.play_system_sound(crate::system_sound::SoundType::OptionChange);
    }

    pub(super) fn cycle_select_hs_fix(&mut self, arg: i32) {
        if !self.begin_selected_play_mode_edit() {
            return;
        }
        self.select.hs_fix_option =
            cycle_hs_fix_option_with_direction(self.select.hs_fix_option, arg);
        self.boot.profile_config.play.hs_fix = hs_fix_config_from_option(self.select.hs_fix_option);
        self.finish_selected_play_mode_edit();
        tracing::info!(hs_fix = self.select.hs_fix_option.as_str(), "HS-FIX option changed");
        self.play_system_sound(crate::system_sound::SoundType::OptionChange);
    }

    pub(super) fn cycle_select_bga(&mut self, arg: i32) {
        self.boot.profile_config.play.bga =
            cycle_bga_option_with_direction(self.boot.profile_config.play.bga, arg);
        tracing::info!(
            bga = bga_mode_as_str(self.boot.profile_config.play.bga),
            "bga option changed"
        );
        self.play_system_sound(crate::system_sound::SoundType::OptionChange);
    }

    pub(super) fn cycle_select_bga_expand(&mut self, arg: i32) {
        self.boot.profile_config.play.bga_expand =
            cycle_bga_expand_with_direction(self.boot.profile_config.play.bga_expand, arg);
        tracing::info!(
            bga_expand = ?self.boot.profile_config.play.bga_expand,
            "bga expand changed"
        );
        self.play_system_sound(crate::system_sound::SoundType::OptionChange);
    }

    pub(super) fn cycle_select_target(&mut self, arg: i32) {
        let cycle = if arg >= 0 { TargetCycle::Next } else { TargetCycle::Previous };
        self.apply_target_option_cycle(cycle);
        self.play_system_sound(crate::system_sound::SoundType::OptionChange);
    }

    pub(super) fn cycle_active_rival(&mut self, arg: i32) {
        let Some(provider) =
            crate::ir::provider_key::primary_provider_config(&self.boot.profile_config.ir)
        else {
            return;
        };
        if !crate::ir::rian_ir::is_rian_ir_provider(&provider.provider) {
            return;
        }
        let Some(provider_key) = crate::ir::provider_key::configured_provider_key(provider) else {
            return;
        };
        let ids: Vec<String> = self
            .boot
            .profile_config
            .rival
            .entries
            .iter()
            .filter(|entry| {
                matches!(entry.source, RivalSourceConfig::Ir)
                    && entry.ir_service == provider_key
                    && !entry.ir_user_id.is_empty()
            })
            .map(|entry| entry.id.clone())
            .collect();
        if ids.is_empty() {
            return;
        }

        let current = self.boot.profile_config.rival.active_rival.clone();
        let position = if current.is_empty() {
            0
        } else {
            ids.iter().position(|id| id == &current).map(|index| index + 1).unwrap_or(0)
        };
        let count = ids.len() + 1;
        let next = if arg >= 0 { (position + 1) % count } else { (position + count - 1) % count };
        self.boot.profile_config.rival.active_rival =
            if next == 0 { String::new() } else { ids[next - 1].clone() };
        self.boot.profile_config.updated_at = now_unix_seconds();
        if let Err(error) =
            save_profile_config(&self.boot.profile_paths.profile_toml, &self.boot.profile_config)
        {
            self.boot.profile_config.rival.active_rival = current;
            tracing::error!(%error, "failed to save active rival");
            return;
        }
        let target = crate::screens::select_ir::SelectRivalFetchTarget::from_profile(
            &self.boot.profile_config,
        );
        self.select.select_ir.update_rival(target, &self.boot.profile_paths.root_dir);
        tracing::info!(
            rival = %self.boot.profile_config.rival.active_rival,
            "active rival changed"
        );
        self.play_system_sound(crate::system_sound::SoundType::OptionChange);
    }

    pub(super) fn cycle_chart_replication_mode(&mut self, arg: i32) {
        let previous = self.boot.profile_config.rival.chart_replication_mode;
        self.boot.profile_config.rival.chart_replication_mode = previous.cycle(arg >= 0);
        self.boot.profile_config.updated_at = now_unix_seconds();
        if let Err(error) =
            save_profile_config(&self.boot.profile_paths.profile_toml, &self.boot.profile_config)
        {
            self.boot.profile_config.rival.chart_replication_mode = previous;
            tracing::error!(%error, "failed to save chart replication mode");
            return;
        }
        tracing::info!(
            mode = self.boot.profile_config.rival.chart_replication_mode.as_str(),
            "chart replication mode changed"
        );
        self.play_system_sound(crate::system_sound::SoundType::OptionChange);
    }

    pub(super) fn cycle_select_gauge_auto_shift(&mut self, arg: i32) {
        self.select.gauge_auto_shift_option =
            cycle_gauge_auto_shift_option_with_direction(self.select.gauge_auto_shift_option, arg);
        tracing::info!(
            gauge_auto_shift = gauge_auto_shift_as_str(self.select.gauge_auto_shift_option),
            "gauge auto shift changed"
        );
        self.play_system_sound(crate::system_sound::SoundType::OptionChange);
    }

    pub(super) fn cycle_select_bottom_shiftable_gauge(&mut self, arg: i32) {
        self.select.bottom_shiftable_gauge_option = cycle_bottom_shiftable_gauge_with_direction(
            self.select.bottom_shiftable_gauge_option,
            arg,
        );
        tracing::info!(
            bottom_shiftable_gauge =
                bottom_shiftable_gauge_as_str(self.select.bottom_shiftable_gauge_option),
            "bottom shiftable gauge changed"
        );
        self.play_system_sound(crate::system_sound::SoundType::OptionChange);
    }

    pub(super) fn cycle_select_judge_algorithm(&mut self, arg: i32) {
        self.boot.profile_config.judge.judge_algorithm = cycle_judge_algorithm_with_direction(
            self.boot.profile_config.judge.judge_algorithm,
            arg,
        );
        self.boot.profile_config.updated_at = now_unix_seconds();
        self.sync_realtime_profile_settings();
        self.invalidate_play_preload();
        tracing::info!(
            judge_algorithm = self.boot.profile_config.judge.judge_algorithm.beatoraja_name(),
            "judge algorithm changed"
        );
        self.play_system_sound(crate::system_sound::SoundType::OptionChange);
    }

    pub(super) fn cycle_select_sort(&mut self, arg: i32) {
        self.select.select_sort = if arg >= 0 {
            self.select.select_sort.next()
        } else {
            self.select.select_sort.previous()
        };
        // 退出 / プレイ後の save_current_play_options で永続化されるよう、
        // profile config をメモリ上で先に更新しておく。
        self.boot.profile_config.select.sort = self.select.select_sort.as_str().to_string();
        self.reload_select_items();
        tracing::info!(sort = self.select.select_sort.as_str(), "select sort changed");
        self.play_system_sound(crate::system_sound::SoundType::OptionChange);
    }

    pub(super) fn cycle_select_ln_mode(&mut self, arg: i32) {
        let score_context_before = SelectScoreContext::from_profile(&self.boot.profile_config);
        self.boot.profile_config.play.ln_mode_policy = if arg >= 0 {
            self.boot.profile_config.play.ln_mode_policy.next()
        } else {
            self.boot.profile_config.play.ln_mode_policy.previous()
        };
        self.sync_changed_select_score_context(score_context_before);
        tracing::info!(
            ln_mode = self.boot.profile_config.play.ln_mode_policy.display_label(),
            "select LN mode policy changed"
        );
        self.play_system_sound(crate::system_sound::SoundType::OptionChange);
    }

    pub(super) fn cycle_replay_slot_rule(&mut self, event_id: i32, arg: i32) {
        let slot = (event_id - 321) as usize;
        if slot >= 4 {
            return;
        }
        let rule = &mut self.boot.profile_config.replay.slot_rules[slot];
        let next = rule.cycle(arg >= 0);
        if next == *rule {
            return;
        }
        *rule = next;
        tracing::info!(slot, ?next, "select replay autosave rule changed");
        self.play_system_sound(crate::system_sound::SoundType::OptionChange);
    }

    pub(super) fn adjust_select_hispeed(&mut self, arg: i32) {
        if !self.begin_selected_play_mode_edit() {
            return;
        }
        let mode = match self.boot.profile_config.lane.floating_policy {
            FloatingPolicyConfig::Disabled => {
                base_hispeed_mode_from_config(self.boot.profile_config.lane.base_hispeed)
            }
            FloatingPolicyConfig::Locked => HispeedMode::Floating,
            FloatingPolicyConfig::Toggle if self.select.hs_fix_option != HsFixOption::Off => {
                HispeedMode::Floating
            }
            FloatingPolicyConfig::Toggle => {
                base_hispeed_mode_from_config(self.boot.profile_config.lane.base_hispeed)
            }
        };
        let change = if arg >= 0 { HispeedChange::Up } else { HispeedChange::Down };
        if mode == HispeedMode::Normal {
            let current = self.boot.profile_config.lane.normal_hispeed_level;
            let next = adjusted_normal_hispeed_level(current, change);
            if next == current {
                return;
            }
            self.boot.profile_config.lane.normal_hispeed_level = next;
            self.finish_selected_play_mode_edit();
            self.sync_realtime_profile_settings();
            self.play_system_sound(crate::system_sound::SoundType::OptionChange);
            return;
        }
        let step = hispeed_step_for_profile(&self.boot.profile_config, mode);
        let current = self.boot.profile_config.lane.hispeed;
        let next = adjusted_hispeed(current, change, step);
        if next == current {
            return;
        }
        self.boot.profile_config.lane.hispeed = next;
        self.finish_selected_play_mode_edit();
        self.sync_realtime_profile_settings();
        self.play_system_sound(crate::system_sound::SoundType::OptionChange);
    }

    pub(super) fn adjust_select_note_display_duration(&mut self, arg: i32) {
        if !self.begin_selected_play_mode_edit() {
            return;
        }
        if self.boot.profile_config.lane.floating_policy == FloatingPolicyConfig::Disabled {
            return;
        }
        let current = self.boot.profile_config.lane.target_green_number;
        let next = crate::config::play::adjust_green_number_by_duration_ms(
            current,
            if arg >= 0 { 1 } else { -1 },
        );
        if next == current {
            return;
        }
        self.boot.profile_config.lane.target_green_number = next;
        self.finish_selected_play_mode_edit();
        self.sync_realtime_profile_settings();
        self.play_system_sound(crate::system_sound::SoundType::OptionChange);
    }

    pub(super) fn cycle_extra_note_depth(&mut self, arg: i32) {
        let current = self.boot.profile_config.play.assist.extra_note_depth.min(3);
        self.boot.profile_config.play.assist.extra_note_depth = cycle_u8(current, 4, arg >= 0);
        self.finish_global_assist_event();
    }

    pub(super) fn cycle_assist_mine_mode(&mut self, arg: i32) {
        use crate::config::profile_config::AssistMineMode::*;
        let values = [Off, Remove, AddRandom, AddNear, AddBlank];
        self.boot.profile_config.play.assist.mine_mode =
            cycle_copy(values, self.boot.profile_config.play.assist.mine_mode, arg >= 0);
        self.finish_global_assist_event();
    }

    pub(super) fn cycle_assist_scroll_mode(&mut self, arg: i32) {
        use crate::config::profile_config::AssistScrollMode::*;
        let values = [Off, Remove, Add];
        self.boot.profile_config.play.assist.scroll_mode =
            cycle_copy(values, self.boot.profile_config.play.assist.scroll_mode, arg >= 0);
        self.finish_global_assist_event();
    }

    pub(super) fn cycle_assist_long_note_mode(&mut self, arg: i32) {
        use crate::config::profile_config::AssistLongNoteMode::*;
        let values = [Off, Remove, AddLn, AddCn, AddHcn, AddAll];
        self.boot.profile_config.play.assist.long_note_mode =
            cycle_copy(values, self.boot.profile_config.play.assist.long_note_mode, arg >= 0);
        self.finish_global_assist_event();
    }

    fn finish_global_assist_event(&mut self) {
        self.boot.profile_config.updated_at = now_unix_seconds();
        self.invalidate_play_preload();
        self.play_system_sound(crate::system_sound::SoundType::OptionChange);
    }

    pub(super) fn toggle_select_constant(&mut self) {
        if !self.begin_selected_play_mode_edit() {
            return;
        }
        self.boot.profile_config.lane.constant_enabled =
            !self.boot.profile_config.lane.constant_enabled;
        self.finish_selected_play_mode_edit();
        self.sync_realtime_profile_settings();
        self.play_system_sound(crate::system_sound::SoundType::OptionChange);
    }
}

fn cycle_u8(current: u8, count: u8, forward: bool) -> u8 {
    if forward { (current + 1) % count } else { (current + count - 1) % count }
}

fn cycle_copy<T: Copy + PartialEq, const N: usize>(values: [T; N], current: T, forward: bool) -> T {
    let index = values.iter().position(|value| *value == current).unwrap_or(0);
    let next = if forward { (index + 1) % N } else { (index + N - 1) % N };
    values[next]
}

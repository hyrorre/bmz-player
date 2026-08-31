use super::*;

impl WinitApp {
    pub(super) fn select_snapshot(&self) -> SelectSnapshot {
        let locale = self.boot.profile_config.ui.locale();
        let text = Localizer::new(locale);
        let selected = self.select.select_items.get(self.select.selected_index);
        let session_mode = if self.select.ir_battle.active {
            SessionMode::GBattle
        } else {
            self.select.session_mode
        };
        let active_rival_name =
            self.select.select_ir.active_rival_display_name().map(str::to_string);
        let rival = match selected {
            Some(SelectItem::Chart(row)) if active_rival_name.is_some() => {
                row.score_sha256().and_then(|sha256| {
                    let policy = crate::ln_policy::score_ln_policy(
                        self.boot.profile_config.play.ln_mode_policy,
                        row.chart.as_ref().map(|chart| chart.ln_profile).unwrap_or_default(),
                    );
                    let ln_mode = crate::screens::select_ir::rian_ln_mode_for_chart(
                        row.chart.as_ref().map(|chart| chart.ln_profile).unwrap_or_default(),
                        policy,
                    );
                    self.select.select_ir.active_rival_snapshot(sha256, ln_mode)
                })
            }
            _ => self
                .select
                .select_ir
                .rival_for(&self.boot.profile_config.ir, self.selected_chart_sha256()),
        };
        let rival_selected = active_rival_name.is_some() || rival.is_some();
        let rival_name = active_rival_name
            .or_else(|| rival.as_ref().map(|rival| rival.display_name.clone()))
            .unwrap_or_default();
        let selected_course_ir = self.selected_course_ir_target();
        let select_ir_scope_binding = self
            .renderer
            .select_skin_document()
            .map(|document| document.select_ir_scope_binding)
            .unwrap_or_default();
        let current_folder = if self.select.ir_battle.active {
            "IR RANKING / G-BATTLE".to_string()
        } else {
            match self.select.folder_stack.last() {
            None => String::new(),
            Some(path) if path == FAVORITE_ROOT_PATH => "FAVORITE".to_string(),
            Some(path) if path == FAVORITE_CHART_PATH => "FAVORITE CHART".to_string(),
            Some(path) if path == FAVORITE_SONG_PATH => "FAVORITE SONG".to_string(),
            Some(path) if parse_favorite_song_detail_path(path).is_some() => {
                "FAVORITE SONG".to_string()
            }
            Some(path) if parse_course_contents_path(path).is_some() => "COURSE".to_string(),
            Some(path) if path.starts_with(VIRTUAL_FOLDER_PATH_PREFIX) => {
                virtual_folder_breadcrumb(&self.boot.profile_paths.root_dir, path)
                    .unwrap_or_else(|error| {
                        tracing::warn!(%error, path, "failed to build virtual-folder breadcrumb");
                        None
                    })
                    .unwrap_or_default()
            }
            Some(path) if let Some(folder) = parse_same_folder_path(path) => {
                std::path::Path::new(folder)
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("")
                    .to_string()
            }
            Some(path) if path.starts_with(TABLE_ROOT_PATH) => match parse_table_path(path) {
                Some(TablePath::Root) | None => text.text("select-difficulty-tables"),
                Some(TablePath::Table { source_url }) => self.table_breadcrumb_name(source_url),
                Some(TablePath::Level { source_url, level }) => {
                    let table = self.table_breadcrumb(source_url);
                    format!("{} > {}{}", table.name, table.symbol, level)
                }
            },
            Some(path) if in_settings_stack(std::slice::from_ref(path)) => {
                settings_breadcrumb_for_locale(path, locale)
            }
            Some(path) => std::path::Path::new(path)
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("")
                .to_string(),
            }
        };
        let (search_word, search_word_alpha, search_caret_byte_index) = self.display_search_word();
        self.ensure_visible_select_chart_distributions(25);
        let chart_distributions = self.select.select_distribution_cache.borrow();
        let selected_play_mode = self.selected_play_mode();
        let mode_config =
            selected_play_mode.map(|mode| self.boot.profile_config.play_mode_config(mode));
        let hispeed_config = mode_config.as_ref().map(PlayModeConfig::hispeed_config);
        let select_floating =
            mode_config.as_ref().is_some_and(|config| match config.floating_policy {
                FloatingPolicyConfig::Disabled => false,
                FloatingPolicyConfig::Locked => true,
                FloatingPolicyConfig::Toggle => self.select.hs_fix_option != HsFixOption::Off,
            });
        let note_display_duration_ms = mode_config.as_ref().map(|config| {
            Self::select_note_display_duration_ms_for_config(config, select_floating)
        });
        let ln_policy_setting = self.boot.profile_config.play.ln_mode_policy;
        let ln_score_policy = match selected {
            Some(SelectItem::Chart(row)) => row.chart.as_ref().map(|chart| {
                crate::ln_policy::score_ln_policy(ln_policy_setting, chart.ln_profile)
            }),
            Some(SelectItem::Course(row)) => Some(row.ln_policy),
            _ => None,
        };
        let source_ln_profile = match selected {
            Some(SelectItem::Chart(row)) => row.chart.as_ref().map(|chart| chart.ln_profile),
            _ => None,
        };
        let source_key_mode = selected_play_mode;
        let conversion = source_key_mode
            .filter(|_| {
                !session_mode.is_battle()
                    && !matches!(
                        self.select.double_option,
                        DoubleOption::Battle | DoubleOption::BattleAutoScratch
                    )
            })
            .filter(|mode| self.boot.profile_config.play.key_mode_conversion.applies_to(*mode))
            .map(|_| self.boot.profile_config.play.key_mode_conversion)
            .unwrap_or(KeyModeConversionConfig::Off);
        let applied_double_option = if conversion == KeyModeConversionConfig::Off {
            self.select.double_option
        } else {
            DoubleOption::Off
        };
        let skin_attempt = bmz_render::snapshot::SkinAttemptState {
            source_key_mode,
            effective_key_mode: source_key_mode.map(|mode| {
                crate::skin_extension::select_effective_key_mode(
                    mode,
                    applied_double_option,
                    session_mode,
                    conversion,
                )
            }),
            seven_to_six: conversion == KeyModeConversionConfig::SevenToSix,
            seven_to_nine_pattern: if conversion == KeyModeConversionConfig::SevenToNine {
                self.boot.profile_config.play.seven_to_nine_pattern.value()
            } else {
                0
            },
            seven_to_nine_type: self.boot.profile_config.play.seven_to_nine_type.value(),
            source_ln_profile_bits: source_ln_profile
                .map(crate::skin_extension::source_ln_profile_bits),
            session_mode_index: Some(crate::skin_extension::session_mode_index(session_mode)),
            double_option_index: Some(crate::skin_extension::double_option_index(
                applied_double_option,
            )),
            hsfix_index: mode_config.as_ref().map(|config| {
                crate::skin_extension::hsfix_index(hs_fix_option_from_profile(config.hs_fix))
            }),
            gauge_auto_shift_index: Some(bmz_render::skin::select_gauge_auto_shift_index(
                gauge_auto_shift_as_str(self.select.gauge_auto_shift_option),
            )),
            bottom_shiftable_gauge_index: Some(
                bmz_render::skin::select_bottom_shiftable_gauge_index(
                    bottom_shiftable_gauge_as_str(self.select.bottom_shiftable_gauge_option),
                ),
            ),
            judge_algorithm_index: Some(crate::skin_extension::judge_algorithm_index(
                crate::screens::play_session::judge_algorithm_from_config(
                    self.boot.profile_config.judge.judge_algorithm,
                ),
            )),
            ln_mode_index: ln_score_policy.map(|policy| {
                source_ln_profile.map_or_else(
                    || {
                        crate::skin_extension::long_note_mode_index(
                            crate::skin_extension::ln_score_policy_mode(policy),
                        )
                    },
                    |profile| crate::skin_extension::effective_ln_mode_index(profile, policy),
                )
            }),
            has_bga: match selected {
                Some(SelectItem::Chart(row)) => row.chart.as_ref().map(|chart| chart.has_bga),
                _ => None,
            },
            has_random_sequence: match selected {
                Some(SelectItem::Chart(row)) => {
                    row.chart.as_ref().map(|chart| chart.has_bms_random)
                }
                _ => None,
            },
        };
        let battle_choices = self.select_battle_choices();
        let displayed_index = if self.select.ir_battle.active {
            self.select.ir_battle.cursor
        } else {
            self.select.selected_index
        };
        let in_difficulty_table_level =
            self.select.folder_stack.last().is_some_and(|path| {
                matches!(parse_table_path(path), Some(TablePath::Level { .. }))
            });
        let mut rows = if self.select.ir_battle.active {
            // G-BATTLEでは人物/スコア表示だけを差し替え、譜面そのものの
            // BPM・長さ・ノート数・解析値などは元の選択行から引き継ぐ。
            let source_row = select_snapshot_rows_with_rival(
                &self.select.select_items,
                self.select.selected_index,
                1,
                &self.boot.profile_config,
                &self.boot.app_config,
                in_difficulty_table_level,
                self.select.key_config_edit.as_ref(),
                &chart_distributions,
                Some(&self.select.select_ir),
            )
            .into_iter()
            .next();
            crate::app::select_ir_battle::select_ir_battle_snapshot_rows(
                &battle_choices,
                self.select.ir_battle.cursor,
                25,
                source_row.as_ref(),
            )
        } else {
            select_snapshot_rows_with_rival(
                &self.select.select_items,
                self.select.selected_index,
                25,
                &self.boot.profile_config,
                &self.boot.app_config,
                in_difficulty_table_level,
                self.select.key_config_edit.as_ref(),
                &chart_distributions,
                Some(&self.select.select_ir),
            )
        };
        if matches!(selected, Some(SelectItem::Chart(_)))
            && !self.select.ir_battle.active
            && let Some(selected_row) =
                rows.iter_mut().find(|row| row.index as usize == self.select.selected_index)
        {
            selected_row.replay_slots = self.selected_chart_replay_slots();
        }
        SelectSnapshot {
            time: self.select_time(),
            player_name: String::new(),
            current_fps: 0,
            operating_time_ms: 0,
            skin_input: Default::default(),
            skin_attempt,
            skin_offsets: skin_offset_values_from_config(
                &self.boot.profile_config.skin.select_offsets,
            ),
            selection_time: self.select_bar_time(),
            option_panel_time: self.option_panel_time(),
            option_panel_off_times: self
                .select
                .option_panel_off_started_at
                .map(|started_at| started_at.map(elapsed_since)),
            option_panel: self.select.select_option_panel,
            chart_count: if self.select.ir_battle.active {
                battle_choices.len()
            } else {
                self.select.select_items.len()
            } as u32,
            selected_index: displayed_index as u32,
            bar_scroll_direction: self.select.select_bar_scroll_direction,
            bar_scroll_progress: self.select_bar_scroll_progress(),
            selected_chart_id: match selected {
                Some(SelectItem::Chart(row)) => row.chart.as_ref().map(|chart| chart.chart_id),
                _ => None,
            },
            selected_replay_slot: if self.select.ir_battle.active {
                None
            } else {
                self.selected_replay_slot_for_selected()
            },
            selected_title: if self.select.ir_battle.active {
                battle_choices
                    .get(self.select.ir_battle.cursor)
                    .map(crate::app::select_ir_battle::SelectBattleChoice::title)
                    .unwrap_or_default()
            } else {
                selected.map(|item| item.display_name_for_locale(locale)).unwrap_or_default()
            },
            hispeed: mode_config.as_ref().map(|config| config.hispeed).unwrap_or(0.0),
            hispeed_mode_index: i32::from(select_floating),
            base_hispeed_index: mode_config
                .as_ref()
                .map_or(0, |config| i32::from(config.base_hispeed == BaseHispeedConfig::Normal)),
            normal_hispeed_level: mode_config.as_ref().map_or(18, |config| {
                crate::config::play::normalize_normal_hispeed_level(config.normal_hispeed_level)
            }),
            hispeed_config_index: hispeed_config.map_or(4, HispeedConfigPreset::index),
            note_display_duration_ms,
            rows,
            arrange: self.select.arrange_option.as_str().to_string(),
            arrange_2p: self.select.arrange_option_2p.as_str().to_string(),
            // 通常のRANDOMはプレイ開始時に抽選する。将来、選曲中に確定した
            // リプレイ／ライバル配置をここへ渡す。
            lane_shuffle_pattern: Vec::new(),
            target: self.select.target_option.as_string(),
            chart_replication_mode: self
                .boot
                .profile_config
                .rival
                .chart_replication_mode
                .as_str()
                .to_string(),
            gauge: gauge_option_as_str(self.select.gauge_option).to_string(),
            gauge_auto_shift: gauge_auto_shift_as_str(self.select.gauge_auto_shift_option)
                .to_string(),
            bottom_shiftable_gauge: bottom_shiftable_gauge_as_str(
                self.select.bottom_shiftable_gauge_option,
            )
            .to_string(),
            double_option: self.select.double_option.as_str().to_string(),
            hs_fix: mode_config
                .as_ref()
                .map(|config| hs_fix_option_from_profile(config.hs_fix).as_str().to_string())
                .unwrap_or_default(),
            assist: session_mode.as_str().to_string(),
            assist_flags: self.boot.profile_config.play.assist.flags(),
            assist_extra_note_depth: self.boot.profile_config.play.assist.extra_note_depth,
            assist_mine_mode: self.boot.profile_config.play.assist.mine_mode as i64,
            assist_scroll_mode: self.boot.profile_config.play.assist.scroll_mode as i64,
            assist_long_note_mode: self.boot.profile_config.play.assist.long_note_mode as i64,
            guide_se_enabled: self.boot.profile_config.play.guide_se,
            constant_enabled: mode_config.as_ref().is_some_and(|config| config.constant_enabled),
            select_mode: self.select.select_mode_filter.as_str().to_string(),
            select_difficulty_filter: self.select.select_difficulty_filter.difficulty_code(),
            random_mix_options: {
                let mix = self.boot.profile_config.select.random_mix;
                [
                    mix.target_level,
                    mix.max_level,
                    mix.min_level,
                    mix.bpm_range,
                    mix.max_bpm,
                    mix.min_bpm,
                    mix.stages,
                ]
            },
            select_sort: self.select.select_sort.as_str().to_string(),
            select_ln_mode: self
                .boot
                .profile_config
                .play
                .ln_mode_policy
                .display_label()
                .to_string(),
            rule_mode_index: crate::skin_extension::rule_mode_index(
                self.boot.profile_config.play.rule_mode,
            ),
            ln_policy_setting_index: crate::skin_extension::ln_policy_setting_index(
                ln_policy_setting,
            ),
            ln_score_policy_index: ln_score_policy
                .map(crate::skin_extension::ln_score_policy_index),
            judge_algorithm: self
                .boot
                .profile_config
                .judge
                .judge_algorithm
                .beatoraja_name()
                .to_string(),
            bga: bga_mode_as_str(self.boot.profile_config.play.bga).to_string(),
            judge_timing_offset_ms: mode_config
                .as_ref()
                .map(|config| {
                    (config.visual_offset_us / 1_000).clamp(i32::MIN as i64, i32::MAX as i64) as i32
                })
                .unwrap_or(i32::MIN),
            judge_timing_auto_adjust: self.boot.profile_config.judge.visual_offset_auto_adjust,
            lanecover_enabled: mode_config.as_ref().is_some_and(|config| {
                matches!(
                    config.lane_effect,
                    LaneEffectConfig::Sudden | LaneEffectConfig::HiddenSudden
                )
            }),
            lift_enabled: mode_config.as_ref().is_some_and(|config| config.lift_enabled),
            hidden_enabled: mode_config.as_ref().is_some_and(|config| {
                matches!(
                    config.lane_effect,
                    LaneEffectConfig::Hidden | LaneEffectConfig::HiddenSudden
                )
            }),
            hispeed_auto_adjust: mode_config
                .as_ref()
                .is_some_and(|config| config.hispeed_auto_adjust),
            master_volume: crate::config::play::volume_unit_to_f32(
                self.boot.profile_config.audio_mix.master_volume,
            ),
            key_volume: crate::config::play::volume_unit_to_f32(
                self.boot.profile_config.audio_mix.key_volume,
            ),
            bgm_volume: crate::config::play::volume_unit_to_f32(
                self.boot.profile_config.audio_mix.bgm_volume,
            ),
            current_folder,
            key_hint: self.select.select_keys.key_hint().to_string(),
            option_hint: self.select.select_keys.option_hint().to_string(),
            exit_hold_progress: self.select_exit_hold_progress(),
            overlay: OverlaySnapshot::default(),
            stage_background: self
                .select
                .select_assets
                .meta_image_loaded(SelectMetaImageSlot::Stage),
            stage_image_size: self.select.select_assets.meta_image_size(SelectMetaImageSlot::Stage),
            backbmp_image: self
                .select
                .select_assets
                .meta_image_loaded(SelectMetaImageSlot::Backbmp),
            backbmp_image_size: self
                .select
                .select_assets
                .meta_image_size(SelectMetaImageSlot::Backbmp),
            banner_image: self.select.select_assets.meta_image_loaded(SelectMetaImageSlot::Banner),
            banner_image_size: self
                .select
                .select_assets
                .meta_image_size(SelectMetaImageSlot::Banner),
            in_settings: in_settings_stack(&self.select.folder_stack),
            settings_editing: self.select.settings_edit.is_some()
                || self.select.key_config_edit.is_some(),
            search_word,
            search_word_alpha,
            search_caret_byte_index,
            search_input_active: self.select.search.is_active(),
            mouse_position: self
                .renderer
                .select_skin_mouse_position(self.cursor_position_normalized()),
            ir: selected_course_ir.as_ref().map_or_else(
                || {
                    self.select.select_ir.snapshot_for_binding(
                        &self.boot.profile_config.ir,
                        self.selected_chart_sha256(),
                        select_ir_scope_binding,
                    )
                },
                |target| {
                    self.select
                        .select_ir
                        .course_snapshot_for(&self.boot.profile_config.ir, Some(target))
                },
            ),
            rival,
            rival_selected,
            rival_name,
            replay_slot_rule_indices: replay_slot_rule_indices(
                &self.boot.profile_config.replay.slot_rules,
            ),
            player_stats: self.select.player_stats.clone(),
        }
    }

    /// 選曲カーソルが曲行のときの chart SHA256。フォルダ / コース行は None。
    pub(super) fn selected_chart_sha256(&self) -> Option<[u8; 32]> {
        match self.select.select_items.get(self.select.selected_index)? {
            SelectItem::Chart(row) => row.score_sha256(),
            _ => None,
        }
    }

    pub(super) fn selected_course_ir_target(
        &self,
    ) -> Option<crate::screens::select_ir::SelectCourseIrTarget> {
        let SelectItem::Course(row) = self.select.select_items.get(self.select.selected_index)?
        else {
            return None;
        };
        Some(crate::screens::select_ir::SelectCourseIrTarget {
            course_hash: row.course_hash.clone()?,
            rian_course_hash_v1: row.rian_course_hash_v1.clone()?,
            bms_ir_course_key: row.bms_ir_course_key.clone(),
            gauge: crate::screens::play_start::course_gauge_for(self.select.gauge_option)
                .as_str()
                .to_string(),
            ln_policy: row.ln_policy.as_str().to_string(),
            rule_mode: self.boot.profile_config.play.rule_mode,
        })
    }

    pub(super) fn select_note_display_duration_ms_for_skin(profile: &ProfileConfig) -> i32 {
        crate::config::play::duration_ms_from_green_number(
            profile.play_mode_config(profile.active_play_mode).target_green_number.max(1),
        ) as i32
    }

    pub(super) fn select_note_display_duration_ms_for_config(
        config: &PlayModeConfig,
        select_floating: bool,
    ) -> i32 {
        let green = if !select_floating && config.base_hispeed == BaseHispeedConfig::Normal {
            crate::config::play::normal_hispeed_green_number(config.normal_hispeed_level)
        } else {
            config.target_green_number.max(1)
        };
        let sudden = if config.lane_effect.sudden_enabled() {
            crate::config::play::lane_unit_to_f32(config.sudden)
        } else {
            0.0
        };
        let hidden = if config.lane_effect.hidden_enabled() {
            crate::config::play::lane_unit_to_f32(config.hidden)
        } else {
            0.0
        };
        let visible = if select_floating {
            let lift = if config.lift_enabled {
                crate::config::play::lane_unit_to_f32(config.lift)
            } else {
                0.0
            };
            let floating_visible = crate::config::play::visible_lane_fraction(sudden, lift);
            if floating_visible > f32::EPSILON {
                let hidden_extent = (1.0 - lift) * hidden;
                ((floating_visible - hidden_extent).clamp(0.0, floating_visible) / floating_visible)
                    .clamp(0.0, 1.0)
            } else {
                0.0
            }
        } else {
            (1.0 - sudden - hidden).clamp(0.0, 1.0)
        };
        (crate::config::play::duration_ms_from_green_number(green) as f32 * visible)
            .round()
            .clamp(0.0, i32::MAX as f32) as i32
    }

    pub(super) fn ensure_visible_select_chart_distributions(&self, visible_limit: usize) {
        let chart_ids: Vec<i64> = select_visible_item_indices(
            self.select.select_items.len(),
            self.select.selected_index,
            visible_limit,
        )
        .into_iter()
        .filter_map(|index| match self.select.select_items.get(index) {
            Some(SelectItem::Chart(row)) => row.chart.as_ref().map(|chart| chart.chart_id),
            _ => None,
        })
        .collect();
        if chart_ids.is_empty() {
            return;
        }

        let missing_ids: Vec<i64> = {
            let cache = self.select.select_distribution_cache.borrow();
            chart_ids.iter().copied().filter(|chart_id| !cache.contains_key(chart_id)).collect()
        };
        if !missing_ids.is_empty() {
            match self.boot.library_db.chart_distributions_by_chart_ids(&missing_ids) {
                Ok(distributions) => {
                    let mut cache = self.select.select_distribution_cache.borrow_mut();
                    for (chart_id, distribution) in distributions {
                        cache.insert(chart_id, distribution);
                    }
                    for chart_id in missing_ids {
                        cache.entry(chart_id).or_default();
                    }
                }
                Err(error) => {
                    tracing::warn!(%error, "failed to load visible chart distributions");
                }
            }
        }
        self.select
            .select_distribution_cache
            .borrow_mut()
            .retain(|chart_id, _| chart_ids.contains(chart_id));
    }

    /// Returns the string to render in the skin's `STRING_SEARCHWORD` (ref=30)
    /// slot along with an alpha multiplier (0.0..=1.0). beatoraja's libgdx
    /// `TextField` uses `messageFontColor=GRAY` for placeholder; we approximate
    /// that by multiplying skin-resolved alpha by `< 1.0` for placeholder /
    /// feedback states.
    pub(super) fn display_search_word(&self) -> (String, f32, Option<usize>) {
        self.select.search.display_word(
            in_settings_stack(&self.select.folder_stack),
            Localizer::new(self.boot.profile_config.ui.locale()).text("select-search-placeholder"),
        )
    }
}

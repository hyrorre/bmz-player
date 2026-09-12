use super::*;
use crate::config::profile_config::SevenToNineRuleMode;

impl WinitApp {
    pub(super) fn begin_decide_for_chart(&mut self, chart_id: i64, mut options: PlayStartOptions) {
        self.normalize_key_mode_conversion_options(chart_id, &mut options);
        self.resolve_play_target_from_cache(chart_id, &mut options);
        self.apply_rival_play_overrides(chart_id, &mut options);
        let snapshot = self.decide_snapshot_for_chart(chart_id);
        self.begin_decide_for_chart_with_snapshot(
            chart_id,
            options,
            snapshot,
            None,
            None,
            DecideLaunch::Play,
        );
    }

    pub(super) fn apply_rival_play_overrides(&self, chart_id: i64, options: &mut PlayStartOptions) {
        options.rival_name = self.select.select_ir.active_rival_display_name().map(str::to_string);
        if self.boot.profile_config.cli_play.as_ref().is_some_and(|state| {
            state.0.seed.is_some()
                || ["arrange", "arrange-2p", "double-option"]
                    .iter()
                    .any(|key| state.0.values.contains_key(*key))
        }) {
            // An explicit CLI chart layout takes priority over profile rival replication.
            return;
        }
        if options.session_mode.is_practice()
            || options.replay_player.is_some()
            || self.play.active_course.is_some()
        {
            return;
        }
        let Some(chart) = self
            .boot
            .library_db
            .list_charts_by_ids(&[chart_id])
            .ok()
            .and_then(|mut charts| charts.pop())
        else {
            return;
        };
        let key_mode = KeyMode::from_str_opt(&chart.mode).unwrap_or_default();
        use crate::config::profile_config::ChartReplicationModeConfig;
        let replication = self.boot.profile_config.rival.chart_replication_mode;

        // KEY4 G-BATTLE target is an explicit per-play choice and must win over
        // the persistent rival selected with KEY7.
        if let Some(arrangement) =
            options.battle_target.as_ref().map(|target| target.playback.arrangement())
        {
            apply_battle_target_replication(options, &arrangement, replication, key_mode);
            return;
        }

        if options.session_mode != SessionMode::Normal
            || options.autoplay
            || options.key_mode_conversion.applies_to(key_mode)
        {
            return;
        }
        let policy = crate::ln_policy::score_ln_policy(
            self.boot.profile_config.play.ln_mode_policy,
            chart.ln_profile,
        );
        let ln_mode = crate::screens::select_ir::rian_ln_mode_for_chart(chart.ln_profile, policy);
        let Some(score) = self.select.select_ir.active_rival_score(chart.sha256, ln_mode).cloned()
        else {
            // beatoraja: 選択ライバルが未プレイなら通常ターゲットへ戻す。
            return;
        };
        if let Some(name) = self.select.select_ir.active_rival_display_name() {
            options.resolved_target =
                Some(ResolvedTarget { name: name.to_string(), ex_score: score.ex_score });
        }

        if replication == ChartReplicationModeConfig::None {
            return;
        }
        let (arrange, arrange_2p, double_option) = rival_arrange_options(&score);
        options.arrange = arrange;
        options.arrange_2p = arrange_2p;
        options.double_option = double_option.normalize_for_key_mode(key_mode);
        options.arrange_pattern = None;
        options.random_trainer_seed = None;

        if replication == ChartReplicationModeConfig::RivalChart
            && let Some(packed) = score.play_seed.and_then(|seed| u64::try_from(seed).ok())
        {
            let is_double = matches!(key_mode, KeyMode::K10 | KeyMode::K14);
            if let Some(seeds) =
                crate::random_option_seed::RandomOptionSeeds::unpack(packed, is_double)
            {
                options.arrange_seed = Some(i64::from(seeds.p1.value()));
                options.arrange_seed_2p = seeds.p2.map(|seed| i64::from(seed.value()));
                options.legacy_arrange_seed = false;
            } else {
                tracing::warn!(packed, ?key_mode, "ignoring invalid rival play seed");
            }
        }
    }

    pub(super) fn begin_course_decide_for_chart(
        &mut self,
        chart_id: i64,
        mut options: PlayStartOptions,
        course_title: &str,
        chart_metadata: ChartListItem,
    ) {
        self.normalize_key_mode_conversion_options(chart_id, &mut options);
        self.resolve_play_target_from_cache(chart_id, &mut options);
        let mut snapshot = self.decide_snapshot_for_chart_with_metadata(chart_id, &chart_metadata);
        self.apply_course_skin_context(&mut snapshot);
        let title_override =
            DecideTitleOverride { title: course_title.to_string(), subtitle: String::new() };
        self.begin_decide_for_chart_with_snapshot(
            chart_id,
            options,
            snapshot,
            Some(title_override),
            Some(chart_metadata),
            DecideLaunch::Play,
        );
    }

    pub(super) fn begin_decide_for_chart_with_snapshot(
        &mut self,
        chart_id: i64,
        options: PlayStartOptions,
        mut snapshot: RenderSnapshot,
        title_override: Option<DecideTitleOverride>,
        chart_metadata: Option<ChartListItem>,
        launch: DecideLaunch,
    ) {
        // Pre-import placeholder only: resolve the same AUTO fallback / FORCE
        // priority as preload and account for Battle here. The running session
        // replaces this with a count derived from the imported source chart.
        let chart_metadata = chart_metadata.or_else(|| {
            self.boot
                .library_db
                .list_charts_by_ids(&[chart_id])
                .ok()
                .and_then(|mut charts| charts.pop())
        });
        let (ln_policy_setting, rule_mode) = self
            .play
            .active_course
            .as_ref()
            .map(|course| (course.ln_policy_setting, course.rule_mode))
            .unwrap_or((
                self.boot.profile_config.play.ln_mode_policy,
                self.boot.profile_config.play.rule_mode,
            ));
        snapshot.rule_mode_index = crate::skin_extension::rule_mode_index(rule_mode);
        snapshot.ln_score_policy_index = chart_metadata.as_ref().map(|chart| {
            crate::skin_extension::ln_score_policy_index(crate::ln_policy::course_score_ln_policy(
                ln_policy_setting,
                options.ln_mode_override,
                chart.ln_profile,
            ))
        });
        if let Some(chart) = &chart_metadata {
            let policy = crate::ln_policy::course_score_ln_policy(
                ln_policy_setting,
                options.ln_mode_override,
                chart.ln_profile,
            );
            let source_key_mode = KeyMode::from_str_opt(&chart.mode).unwrap_or_default();
            let multiplier = decide_total_notes_multiplier(source_key_mode, &options);
            snapshot.total_notes = chart.scored_total_notes(policy).saturating_mul(multiplier);
        }
        self.reload_skin_for_scene_entry(SkinKind::Decide);
        // Play 画面へ入ってから stagefile / backbmp の有無が切り替わると、ロード演出中に
        // 代替タイトルから曲画像へ差し替わって見える。Decide 中に先行ロードし、
        // Play の最初の snapshot から同じ runtime image 100 / 101 を使えるようにする。
        if let Some(chart) = &chart_metadata {
            self.prepare_play_meta_image_textures_from_chart(chart);
        } else {
            self.prepare_play_meta_image_textures(chart_id);
        }
        // Play スキンは裏で decode+upload を進めるが、Decide 入場では待たない。
        // 実際の Play 入場で `ensure_skin_ready` が保険として残る。
        let play_skin_key_mode = chart_metadata
            .as_ref()
            .and_then(|chart| KeyMode::from_str_opt(&chart.mode))
            .map(|key_mode| {
                play_skin_key_mode_for_options(
                    key_mode,
                    options.double_option,
                    options.session_mode,
                    options.key_mode_conversion,
                    options.battle_target.is_some(),
                )
            })
            .unwrap_or_else(|| self.play_skin_key_mode_for_chart(chart_id, &options));
        let skin_attempt = self.skin_attempt_for_chart(chart_id, &options);
        snapshot.skin_attempt = skin_attempt;
        let play_skin_runtime_state = lua_runtime_state_for_play(
            &options,
            self.boot.profile_config.play.auto_play,
            play_skin_key_mode,
            chart_metadata
                .as_ref()
                .and_then(|chart| self.play_skin_previous_best_ex_score_for_chart(chart, &options)),
            &self.boot.profile_config.display_name,
            skin_attempt,
        );
        self.start_play_preload(chart_id, options.clone());
        self.spawn_play_skin_decode_for(
            play_skin_key_mode,
            options.session_mode,
            play_skin_runtime_state,
        );
        let now = Instant::now();
        self.play.pending_decide = Some(DecideTransition {
            chart_id,
            options,
            launch,
            started_at: now,
            fadeout_started_at: None,
            cancel: false,
            snapshot,
            title_override,
        });
    }

    pub(super) fn start_play_preload(
        &mut self,
        chart_id: i64,
        mut options: PlayStartOptions,
    ) -> u64 {
        self.normalize_key_mode_conversion_options(chart_id, &mut options);
        self.resolve_play_target_from_cache(chart_id, &mut options);
        // 通常開始・practice・retry は、残っているコース次曲先読みを置き換える。
        // コース側は worker 開始後に同じ generation の launch 情報を設定し直す。
        self.play.pending_course_stage_launch = None;
        self.play.play_preload_generation = self.play.play_preload_generation.wrapping_add(1);
        let generation = self.play.play_preload_generation;
        self.play.preloaded_play_session = None;
        let (tx, rx) = mpsc::channel();
        let library_db_path = self.boot.app_paths.library_db.clone();
        let app_config = self.play_session_app_config();
        let normalization_output_gain =
            crate::config::play::chart_normalization_output_gain(&self.boot.profile_config);
        let play_config_key_mode =
            effective_play_key_mode(self.key_mode_for_chart(chart_id), options.key_mode_conversion);
        options.hs_fix = hs_fix_option_from_profile(
            self.boot.profile_config.play_mode_config(play_config_key_mode).hs_fix,
        );
        let (ln_policy_setting, rule_mode) = self
            .play
            .active_course
            .as_ref()
            .map(|course| (course.ln_policy_setting, course.rule_mode))
            .unwrap_or((
                self.boot.profile_config.play.ln_mode_policy,
                self.boot.profile_config.play.rule_mode,
            ));
        let input = SharedInputBackend::default();
        let preload_input = input.clone();
        let audio_progress = Arc::new(AtomicU32::new(0));
        let worker_audio_progress = Arc::clone(&audio_progress);
        let prepared_chart = Arc::new(OnceLock::new());
        let worker_prepared_chart = Arc::clone(&prepared_chart);
        thread::Builder::new()
            .name(format!("play-preload-{chart_id}"))
            .spawn(move || {
                let result = (|| -> Result<PreloadedInputPlaySession> {
                    let library_db =
                        crate::storage::library_db::LibraryDatabase::open(&library_db_path)?;
                    let mut session_options =
                        crate::screens::play_start::play_session_options_from_start(
                            &app_config,
                            options,
                        );
                    session_options.play_config_key_mode = Some(play_config_key_mode);
                    session_options.ln_policy_setting = ln_policy_setting;
                    session_options.rule_mode = rule_mode;
                    let preloaded =
                        crate::screens::play_session::preload_play_session_for_chart_with_callbacks(
                            &library_db,
                            chart_id,
                            session_options.clone(),
                            normalization_output_gain,
                            |chart| {
                                let _ = worker_prepared_chart.set(chart.clone());
                            },
                            |loaded, total| {
                                worker_audio_progress.store(
                                    resource_load_progress_units(loaded, total),
                                    Ordering::Relaxed,
                                );
                            },
                        )?;
                    Ok(PreloadedInputPlaySession {
                        chart_id,
                        preloaded,
                        input: preload_input,
                        session_options,
                    })
                })()
                .map_err(|error| format!("{error:#}"));
                let _ = tx.send(PlayPreloadResult { generation, chart_id, result });
            })
            .expect("failed to spawn play preload thread");
        self.play.pending_play_preload = Some(PendingPlayPreload {
            generation,
            chart_id,
            input,
            audio_progress,
            prepared_chart,
            rx,
        });
        // 譜面変換結果を受け取ってから、その同じ chart manifest で BMP/BGA を開始する。
        // ここでは対象だけを予約し、従来の BGA worker による BMS 二重 parse は行わない。
        self.play.bga_preload.begin_unresolved(chart_id);
        tracing::info!(chart_id, generation, "play preload started");
        generation
    }

    pub(super) fn invalidate_play_preload(&mut self) {
        self.play.play_preload_generation = self.play.play_preload_generation.wrapping_add(1);
        self.play.pending_play_preload = None;
        self.play.pending_course_stage_launch = None;
        // 裏で完成して退避していた結果も無効化する (decide キャンセル / 譜面差し替え)。
        self.play.preloaded_play_session = None;
        self.invalidate_chart_bga_texture_preload();
    }

    /// select_items に持っている `ChartListItem.mode` から KeyMode を引く。
    /// コース行から開始した譜面など select_items に Chart 行が無い場合は DB を参照し、
    /// 未知 / 見つからない場合だけデフォルトの 7K を返す。
    pub(super) fn key_mode_for_chart(&self, chart_id: i64) -> KeyMode {
        if let Some(key_mode) = self
            .select
            .select_items
            .iter()
            .find_map(|item| match item {
                SelectItem::Chart(row) => row.chart.as_ref().and_then(|chart| {
                    (chart.chart_id == chart_id).then(|| KeyMode::from_str_opt(&chart.mode))
                }),
                _ => None,
            })
            .flatten()
        {
            return key_mode;
        }
        match self.boot.library_db.list_charts_by_ids(&[chart_id]) {
            Ok(mut charts) => charts
                .pop()
                .and_then(|chart| KeyMode::from_str_opt(&chart.mode))
                .unwrap_or_default(),
            Err(error) => {
                tracing::warn!(chart_id, %error, "failed to load chart key_mode for play skin");
                KeyMode::default()
            }
        }
    }

    pub(super) fn skin_attempt_for_chart(
        &self,
        chart_id: i64,
        options: &PlayStartOptions,
    ) -> bmz_render::snapshot::SkinAttemptState {
        let chart = self
            .select
            .select_items
            .iter()
            .find_map(|item| match item {
                SelectItem::Chart(row) => {
                    row.chart.as_ref().filter(|chart| chart.chart_id == chart_id).cloned()
                }
                _ => None,
            })
            .or_else(|| {
                self.boot
                    .library_db
                    .list_charts_by_ids(&[chart_id])
                    .ok()
                    .and_then(|mut charts| charts.pop())
            });
        let Some(chart) = chart else {
            return bmz_render::snapshot::SkinAttemptState::default();
        };
        let Some(source_key_mode) = KeyMode::from_str_opt(&chart.mode) else {
            return bmz_render::snapshot::SkinAttemptState::default();
        };
        let battle_option =
            matches!(options.double_option, DoubleOption::Battle | DoubleOption::BattleAutoScratch);
        let conversion = if !options.session_mode.is_battle()
            && options.battle_target.is_none()
            && !battle_option
            && options.key_mode_conversion.applies_to(source_key_mode)
        {
            options.key_mode_conversion
        } else {
            KeyModeConversionConfig::Off
        };
        let applied_double_option = if conversion != KeyModeConversionConfig::Off
            || options.session_mode.is_battle()
            || options.battle_target.is_some()
        {
            DoubleOption::Off
        } else {
            options.double_option.normalize_for_key_mode(source_key_mode)
        };
        let ln_policy_setting = self
            .play
            .active_course
            .as_ref()
            .map(|course| course.ln_policy_setting)
            .unwrap_or(self.boot.profile_config.play.ln_mode_policy);
        let ln_policy = crate::ln_policy::course_score_ln_policy(
            ln_policy_setting,
            options.ln_mode_override,
            chart.ln_profile,
        );
        let gauge = options.gauge.unwrap_or(self.boot.profile_config.play.gauge);
        bmz_render::snapshot::SkinAttemptState {
            source_key_mode: Some(source_key_mode),
            effective_key_mode: Some(crate::skin_extension::effective_key_mode(
                source_key_mode,
                applied_double_option,
                options.session_mode,
                conversion,
            )),
            seven_to_six: conversion == KeyModeConversionConfig::SevenToSix,
            seven_to_nine_pattern: if conversion == KeyModeConversionConfig::SevenToNine {
                options.seven_to_nine_pattern.value()
            } else {
                0
            },
            seven_to_nine_type: options.seven_to_nine_type.value(),
            source_ln_profile_bits: Some(crate::skin_extension::source_ln_profile_bits(
                chart.ln_profile,
            )),
            session_mode_index: Some(crate::skin_extension::session_mode_index(
                options.session_mode,
            )),
            double_option_index: Some(crate::skin_extension::double_option_index(
                applied_double_option,
            )),
            hsfix_index: Some(crate::skin_extension::hsfix_index(options.hs_fix)),
            gauge_auto_shift_index: Some(crate::skin_extension::gauge_auto_shift_index(
                crate::config::play::gauge_auto_shift_from_config(gauge, options.gauge_auto_shift),
            )),
            bottom_shiftable_gauge_index: Some(
                crate::skin_extension::bottom_shiftable_gauge_index(
                    crate::config::play::bottom_shiftable_gauge_from_config(
                        options.bottom_shiftable_gauge,
                    ),
                ),
            ),
            judge_algorithm_index: Some(crate::skin_extension::judge_algorithm_index(
                crate::screens::play_session::judge_algorithm_from_config(
                    self.boot.profile_config.judge.judge_algorithm,
                ),
            )),
            ln_mode_index: Some(crate::skin_extension::effective_ln_mode_index(
                chart.ln_profile,
                ln_policy,
            )),
            has_bga: Some(chart.has_bga),
            has_random_sequence: Some(chart.has_bms_random),
        }
    }

    pub(super) fn play_skin_key_mode_for_chart(
        &self,
        chart_id: i64,
        options: &PlayStartOptions,
    ) -> KeyMode {
        play_skin_key_mode_for_options(
            self.key_mode_for_chart(chart_id),
            options.double_option,
            options.session_mode,
            options.key_mode_conversion,
            options.battle_target.is_some(),
        )
    }

    /// Playスキンをロードする時点で、実プレイと同じScoreKeyの保存済み
    /// ベストEXスコアを取得する。EX=0の保存レコードも`Some(0)`として扱い、
    /// 「未プレイ」と区別する。
    pub(super) fn play_skin_previous_best_ex_score(
        &self,
        chart_id: i64,
        options: &PlayStartOptions,
    ) -> Option<u32> {
        let chart = self.play_skin_chart(chart_id)?;
        self.play_skin_previous_best_ex_score_for_chart(&chart, options)
    }

    pub(super) fn play_skin_score_key_for_chart_id(
        &self,
        chart_id: i64,
        options: &PlayStartOptions,
    ) -> Option<ScoreKey> {
        let chart = self.play_skin_chart(chart_id)?;
        Some(self.play_skin_score_key_for_chart(&chart, options))
    }

    fn play_skin_chart(&self, chart_id: i64) -> Option<ChartListItem> {
        self.boot
            .library_db
            .list_charts_by_ids(&[chart_id])
            .ok()
            .and_then(|mut charts| charts.pop())
    }

    fn play_skin_previous_best_ex_score_for_chart(
        &self,
        chart: &ChartListItem,
        options: &PlayStartOptions,
    ) -> Option<u32> {
        let score_key = self.play_skin_score_key_for_chart(chart, options);
        match self.boot.score_db.best_ex_score(score_key) {
            Ok(score) => score,
            Err(error) => {
                tracing::warn!(chart_id = chart.chart_id, %error, "failed to load best score for play skin");
                None
            }
        }
    }

    fn play_skin_score_key_for_chart(
        &self,
        chart: &ChartListItem,
        options: &PlayStartOptions,
    ) -> ScoreKey {
        let (ln_policy_setting, rule_mode) = self
            .play
            .active_course
            .as_ref()
            .map(|course| (course.ln_policy_setting, course.rule_mode))
            .unwrap_or((
                self.boot.profile_config.play.ln_mode_policy,
                self.boot.profile_config.play.rule_mode,
            ));
        let source_key_mode = KeyMode::from_str_opt(&chart.mode).unwrap_or_default();
        play_skin_score_key(
            chart.sha256,
            chart.ln_profile,
            source_key_mode,
            options,
            ln_policy_setting,
            rule_mode,
        )
    }

    pub(super) fn resolve_play_target_from_cache(
        &self,
        chart_id: i64,
        options: &mut PlayStartOptions,
    ) {
        options.rival_name = options
            .battle_target
            .as_ref()
            .map(|target| target.player_name.clone())
            .or_else(|| self.select.select_ir.active_rival_display_name().map(str::to_string));
        if options.resolved_target.is_some() || !options.target.uses_ir_ranking() {
            return;
        }
        let Some(chart) = self
            .boot
            .library_db
            .list_charts_by_ids(&[chart_id])
            .ok()
            .and_then(|mut rows| rows.pop())
        else {
            return;
        };
        options.resolved_target = self.select.select_ir.resolved_target_for(
            &self.boot.profile_config.ir,
            Some(chart.sha256),
            options.target,
            self.play_skin_previous_best_ex_score_for_chart(&chart, options),
        );
    }

    pub(super) fn normalize_key_mode_conversion_options(
        &self,
        chart_id: i64,
        options: &mut PlayStartOptions,
    ) {
        let source_key_mode = self.key_mode_for_chart(chart_id);
        options.key_mode_conversion = key_mode_conversion_for_replay_playback(
            options.key_mode_conversion,
            options.seven_to_nine_rule_mode,
            options.replay_player.is_some(),
        );
        if options.session_mode.is_battle()
            || options.battle_target.is_some()
            || matches!(
                options.double_option,
                DoubleOption::Battle | DoubleOption::BattleAutoScratch
            )
        {
            options.key_mode_conversion = KeyModeConversionConfig::Off;
            return;
        }
        if !options.key_mode_conversion.applies_to(source_key_mode) {
            return;
        }
        options.score_save_disabled |=
            options.key_mode_conversion.score_persistence_disabled(options.seven_to_nine_rule_mode);
        if options.key_mode_conversion == KeyModeConversionConfig::SevenToSix {
            options.arrange =
                crate::screens::play_session::normalize_arrange_for_seven_to_six(options.arrange);
        }
        if matches!(
            options.key_mode_conversion,
            KeyModeConversionConfig::SevenToNine | KeyModeConversionConfig::SevenToSix
        ) {
            options.arrange_2p = ArrangeOption::Normal;
        }
        options.double_option = DoubleOption::Off;
        options.target = TargetOption::None;
        options.resolved_target = None;
    }

    pub(super) fn open_prepared_winit_play_session(
        &self,
        prepared: PreparedInputPlaySession,
    ) -> Result<StartedInputPlaySession> {
        let runtime = self.audio.audio_runtime.as_ref().context("audio output is not available")?;
        open_prepared_winit_play_session(&self.boot.score_db, runtime, prepared)
    }

    pub(super) fn play_output_sample_rate(&self) -> u32 {
        self.audio
            .audio_runtime
            .as_ref()
            .map(AudioRuntime::sample_rate)
            .unwrap_or(self.boot.app_config.audio.sample_rate)
    }

    pub(super) fn play_session_app_config(&self) -> AppConfig {
        let mut app_config = self.boot.app_config.clone();
        app_config.audio.sample_rate = self.play_output_sample_rate();
        app_config.input.gamepad_slot_runtime_device_ids =
            resolve_gamepad_runtime_slots(&app_config.input, self.gamepad.as_ref())
                .map(|id| id.map(|id| id.0));
        app_config
    }

    /// ウィンドウと renderer surface の準備後、初回シーン描画に合わせて共有
    /// cpal ストリームを開く。
    /// 起動ロード中に音声デバイスを start して、デバイス側の初期化音が先に鳴るのを避ける。
    /// scene transition sound の発火前に system audio を用意し、PulseAudio backend で
    /// corked stream の内部
    /// worker だけが動き続ける状態を避ける。
    pub(super) fn decide_snapshot_for_chart(&self, chart_id: i64) -> RenderSnapshot {
        let metadata = chart_snapshot_metadata_for_chart(
            &self.select.select_items,
            chart_id,
            |chart_id| {
                self.boot
                    .library_db
                    .list_charts_by_ids(&[chart_id])
                    .map_err(|error| {
                        tracing::warn!(%error, chart_id, "failed to load chart metadata for play snapshot");
                        error
                    })
                    .ok()
                    .and_then(|mut charts| charts.pop())
            },
        );
        self.decide_snapshot_for_chart_metadata(chart_id, metadata)
    }

    pub(super) fn decide_snapshot_for_chart_with_metadata(
        &self,
        chart_id: i64,
        chart: &ChartListItem,
    ) -> RenderSnapshot {
        self.decide_snapshot_for_chart_metadata(chart_id, Some((chart.clone(), None)))
    }

    fn decide_snapshot_for_chart_metadata(
        &self,
        chart_id: i64,
        metadata: Option<(ChartListItem, Option<u32>)>,
    ) -> RenderSnapshot {
        let mut snapshot = RenderSnapshot {
            rule_mode_index: crate::skin_extension::rule_mode_index(
                self.boot.profile_config.play.rule_mode,
            ),
            ..RenderSnapshot::default()
        };
        let chart_hint = metadata.as_ref().map(|(chart, _)| chart);
        if let Some((chart, best_ex_score)) = &metadata {
            snapshot.ln_score_policy_index = Some(crate::skin_extension::ln_score_policy_index(
                crate::ln_policy::score_ln_policy(
                    self.boot.profile_config.play.ln_mode_policy,
                    chart.ln_profile,
                ),
            ));
            let total_notes =
                chart.scored_total_notes_for_setting(self.boot.profile_config.play.ln_mode_policy);
            apply_chart_metadata_to_snapshot(&mut snapshot, chart, total_notes, *best_ex_score);
        }
        let (primary, secondary, fallback) =
            self.table_text_context_for_chart_with_metadata(chart_id, chart_hint).as_tuple();
        snapshot.table_text_primary = primary;
        snapshot.table_text_secondary = secondary;
        snapshot.table_text_fallback = fallback;
        snapshot
    }
}

fn play_skin_score_key(
    chart_sha256: [u8; 32],
    ln_profile: crate::ln_policy::ChartLnProfile,
    source_key_mode: KeyMode,
    options: &PlayStartOptions,
    ln_policy_setting: LnPolicySetting,
    rule_mode: RuleMode,
) -> ScoreKey {
    let battle_presentation = (options.session_mode.is_battle() || options.battle_target.is_some())
        && matches!(source_key_mode, KeyMode::K5 | KeyMode::K7);
    let battle_option =
        matches!(options.double_option, DoubleOption::Battle | DoubleOption::BattleAutoScratch);
    let key_mode_conversion = if !battle_presentation
        && !battle_option
        && options.key_mode_conversion.applies_to(source_key_mode)
    {
        options.key_mode_conversion
    } else {
        KeyModeConversionConfig::Off
    };
    let double_option =
        if key_mode_conversion != KeyModeConversionConfig::Off || battle_presentation {
            DoubleOption::Off
        } else {
            options.double_option.normalize_for_key_mode(source_key_mode)
        };
    let ln_policy = crate::ln_policy::course_score_ln_policy(
        ln_policy_setting,
        options.ln_mode_override,
        ln_profile,
    );
    ScoreKey::with_options(chart_sha256, ln_policy, double_option.score_bucket(), rule_mode)
}

fn decide_total_notes_multiplier(source_key_mode: KeyMode, options: &PlayStartOptions) -> u32 {
    let battle_presentation = (options.session_mode.is_battle() || options.battle_target.is_some())
        && matches!(source_key_mode, KeyMode::K5 | KeyMode::K7);
    if battle_presentation {
        return 1;
    }
    match options.double_option.normalize_for_key_mode(source_key_mode) {
        DoubleOption::Battle | DoubleOption::BattleAutoScratch => 2,
        DoubleOption::Off | DoubleOption::Flip => 1,
    }
}

pub(in crate::app) fn rival_arrange_options(
    score: &crate::storage::network_db::IrRivalScoreRecord,
) -> (ArrangeOption, ArrangeOption, DoubleOption) {
    if !score.arrange_1p.trim().is_empty() {
        return (
            arrange_option_from_rian(&score.arrange_1p),
            arrange_option_from_rian(&score.arrange_2p),
            double_option_from_rian(&score.double_option),
        );
    }
    let packed = score.play_option.max(0);
    (
        arrange_option_from_beatoraja_id(packed % 10),
        arrange_option_from_beatoraja_id((packed / 10) % 10),
        if (packed / 100) % 10 == 1 { DoubleOption::Flip } else { DoubleOption::Off },
    )
}

fn apply_battle_target_replication(
    options: &mut PlayStartOptions,
    arrangement: &crate::screens::play_start::BattleTargetArrangement,
    replication: crate::config::profile_config::ChartReplicationModeConfig,
    key_mode: KeyMode,
) {
    use crate::config::profile_config::ChartReplicationModeConfig;

    if replication == ChartReplicationModeConfig::None {
        return;
    }

    options.arrange = arrangement.arrange;
    options.arrange_2p = arrangement.arrange_2p;
    options.double_option = arrangement.double_option.normalize_for_key_mode(key_mode);
    options.arrange_pattern = None;
    options.random_trainer_seed = None;

    if replication != ChartReplicationModeConfig::RivalChart {
        return;
    }

    let mut copied_chart = false;
    if let Some(seed) = arrangement.arrange_seed {
        options.arrange_seed = Some(seed);
        options.arrange_seed_2p = arrangement.arrange_seed_2p;
        copied_chart = true;
    } else if let Some(packed_seed) = arrangement.packed_seed {
        let unpacked = u64::try_from(packed_seed).ok().and_then(|packed| {
            crate::random_option_seed::RandomOptionSeeds::unpack(
                packed,
                matches!(key_mode, KeyMode::K10 | KeyMode::K14),
            )
        });
        if let Some(seeds) = unpacked {
            options.arrange_seed = Some(i64::from(seeds.p1.value()));
            options.arrange_seed_2p = seeds.p2.map(|seed| i64::from(seed.value()));
            copied_chart = true;
        } else {
            tracing::warn!(packed_seed, ?key_mode, "ignoring invalid battle target play seed");
        }
    }

    if arrangement.arrange_pattern.is_some() {
        copied_chart = true;
    }
    if copied_chart {
        options.arrange_pattern = arrangement.arrange_pattern.clone();
        options.legacy_arrange_seed = arrangement.legacy_arrange_seed;
        options.s_random_scheme = arrangement.s_random_scheme;
        options.s_random_scheme_2p = arrangement.s_random_scheme_2p;
        options.h_random_threshold_ms = arrangement.h_random_threshold_ms;
    } else if arrange_uses_seed(arrangement.arrange) || arrange_uses_seed(arrangement.arrange_2p) {
        tracing::warn!(
            arrange = arrangement.arrange.as_str(),
            arrange_2p = arrangement.arrange_2p.as_str(),
            "battle target has no usable arrangement seed; keeping the local seed"
        );
    }
}

const fn arrange_uses_seed(arrange: ArrangeOption) -> bool {
    !matches!(arrange, ArrangeOption::Normal | ArrangeOption::Mirror)
}

pub(in crate::app) fn arrange_option_from_rian(value: &str) -> ArrangeOption {
    match value.trim().to_ascii_lowercase().as_str() {
        "mirror" => ArrangeOption::Mirror,
        "random" => ArrangeOption::Random,
        "r-random" => ArrangeOption::RRandom,
        "s-random" => ArrangeOption::SRandom,
        "spiral" => ArrangeOption::Spiral,
        "h-random" => ArrangeOption::HRandom,
        "all-scratch" => ArrangeOption::AllScratch,
        "random-ex" => ArrangeOption::RandomEx,
        "s-random-ex" => ArrangeOption::SRandomEx,
        "f-random" => ArrangeOption::FRandom,
        "mf-random" => ArrangeOption::MFRandom,
        _ => ArrangeOption::Normal,
    }
}

fn arrange_option_from_beatoraja_id(value: i32) -> ArrangeOption {
    match value {
        1 => ArrangeOption::Mirror,
        2 => ArrangeOption::Random,
        3 => ArrangeOption::RRandom,
        4 => ArrangeOption::SRandom,
        5 => ArrangeOption::Spiral,
        6 => ArrangeOption::HRandom,
        7 => ArrangeOption::AllScratch,
        8 => ArrangeOption::RandomEx,
        9 => ArrangeOption::SRandomEx,
        _ => ArrangeOption::Normal,
    }
}

pub(in crate::app) fn double_option_from_rian(value: &str) -> DoubleOption {
    match value.trim().to_ascii_lowercase().as_str() {
        "flip" => DoubleOption::Flip,
        _ => DoubleOption::Off,
    }
}

fn key_mode_conversion_for_replay_playback(
    conversion: KeyModeConversionConfig,
    seven_to_nine_rule_mode: SevenToNineRuleMode,
    replay_playback: bool,
) -> KeyModeConversionConfig {
    if replay_playback && conversion.score_persistence_disabled(seven_to_nine_rule_mode) {
        KeyModeConversionConfig::Off
    } else {
        conversion
    }
}

#[cfg(test)]
mod replay_key_mode_conversion_tests {
    use super::*;

    #[test]
    fn replay_disables_conversions_that_cannot_persist_replays() {
        for (conversion, rule_mode) in [
            (KeyModeConversionConfig::SpToDp, SevenToNineRuleMode::Keys7),
            (KeyModeConversionConfig::SevenToSix, SevenToNineRuleMode::Keys7),
            (KeyModeConversionConfig::SevenToNine, SevenToNineRuleMode::Keys9),
        ] {
            assert!(conversion.score_persistence_disabled(rule_mode));
            assert_eq!(
                key_mode_conversion_for_replay_playback(conversion, rule_mode, true),
                KeyModeConversionConfig::Off
            );
        }
    }

    #[test]
    fn replay_keeps_score_eligible_seven_to_nine_conversion() {
        let conversion = KeyModeConversionConfig::SevenToNine;
        let rule_mode = SevenToNineRuleMode::Keys7;

        assert!(!conversion.score_persistence_disabled(rule_mode));
        assert_eq!(
            key_mode_conversion_for_replay_playback(conversion, rule_mode, true),
            KeyModeConversionConfig::SevenToNine
        );
    }

    #[test]
    fn normal_play_keeps_score_disabling_conversion() {
        assert_eq!(
            key_mode_conversion_for_replay_playback(
                KeyModeConversionConfig::SevenToSix,
                SevenToNineRuleMode::Keys7,
                false,
            ),
            KeyModeConversionConfig::SevenToSix
        );
    }
}

#[cfg(test)]
mod rival_replication_tests {
    use super::*;
    use crate::config::profile_config::ChartReplicationModeConfig;
    use crate::screens::play_session::SRandomScheme;
    use crate::screens::play_start::BattleTargetArrangement;
    use crate::storage::network_db::IrRivalScoreRecord;

    fn rival_score(play_option: i32, arrange_1p: &str) -> IrRivalScoreRecord {
        IrRivalScoreRecord {
            chart_sha256: [7; 32],
            ln_mode: 1,
            ex_score: 1234,
            clear_type: 5,
            max_combo: 600,
            min_bp: 10,
            play_option,
            arrange_1p: arrange_1p.to_string(),
            arrange_2p: String::new(),
            double_option: "off".to_string(),
            play_seed: Some(42),
        }
    }

    #[test]
    fn structured_f_random_wins_over_legacy_play_option() {
        let score = rival_score(0, "f-random");

        assert_eq!(
            rival_arrange_options(&score),
            (ArrangeOption::FRandom, ArrangeOption::Normal, DoubleOption::Off)
        );
    }

    #[test]
    fn legacy_play_option_is_used_when_structured_option_is_missing() {
        let mut score = rival_score(121, "");
        score.arrange_2p.clear();
        score.double_option.clear();

        assert_eq!(
            rival_arrange_options(&score),
            (ArrangeOption::Mirror, ArrangeOption::Random, DoubleOption::Flip)
        );
    }

    fn battle_arrangement() -> BattleTargetArrangement {
        BattleTargetArrangement {
            arrange: ArrangeOption::Random,
            arrange_2p: ArrangeOption::Mirror,
            double_option: DoubleOption::Flip,
            arrange_seed: None,
            arrange_seed_2p: None,
            packed_seed: None,
            arrange_pattern: None,
            legacy_arrange_seed: false,
            s_random_scheme: SRandomScheme::Lm120HzV1,
            s_random_scheme_2p: None,
            h_random_threshold_ms: None,
        }
    }

    #[test]
    fn battle_target_keeps_decide_total_notes_on_the_primary_side() {
        let options = PlayStartOptions {
            session_mode: SessionMode::Normal,
            double_option: DoubleOption::Battle,
            battle_target: Some(crate::screens::play_start::BattleTarget {
                provider: "test".to_string(),
                score_id: "score".to_string(),
                player_id: "player".to_string(),
                player_name: "RIVAL".to_string(),
                rank: 1,
                ex_score: 100,
                gauge: None,
                playback: crate::screens::play_start::BattleTargetPlayback::Seed {
                    arrange: ArrangeOption::Normal,
                    arrange_2p: ArrangeOption::Normal,
                    double_option: DoubleOption::Off,
                    packed_seed: None,
                },
            }),
            ..Default::default()
        };

        assert_eq!(decide_total_notes_multiplier(KeyMode::K7, &options), 1);
        assert_eq!(
            decide_total_notes_multiplier(
                KeyMode::K7,
                &PlayStartOptions { double_option: DoubleOption::Battle, ..Default::default() }
            ),
            2
        );
    }

    #[test]
    fn battle_rival_option_copies_options_but_keeps_local_seeds() {
        let mut options = PlayStartOptions {
            arrange: ArrangeOption::SRandom,
            arrange_2p: ArrangeOption::RRandom,
            double_option: DoubleOption::Off,
            arrange_seed: Some(101),
            arrange_seed_2p: Some(202),
            random_trainer_seed: Some(303),
            s_random_scheme: SRandomScheme::Legacy40MsV1,
            arrange_pattern: Some(vec![1, 0, 2]),
            ..PlayStartOptions::default()
        };
        let mut arrangement = battle_arrangement();
        arrangement.packed_seed = Some(0x654321_123456);

        apply_battle_target_replication(
            &mut options,
            &arrangement,
            ChartReplicationModeConfig::RivalOption,
            KeyMode::K14,
        );

        assert_eq!(options.arrange, ArrangeOption::Random);
        assert_eq!(options.arrange_2p, ArrangeOption::Mirror);
        assert_eq!(options.double_option, DoubleOption::Flip);
        assert_eq!(options.arrange_seed, Some(101));
        assert_eq!(options.arrange_seed_2p, Some(202));
        assert_eq!(options.s_random_scheme, SRandomScheme::Legacy40MsV1);
        assert_eq!(options.arrange_pattern, None);
        assert_eq!(options.random_trainer_seed, None);
    }

    #[test]
    fn battle_rival_chart_unpacks_single_play_seed() {
        let mut options = PlayStartOptions {
            arrange_seed: Some(9),
            arrange_seed_2p: Some(8),
            ..PlayStartOptions::default()
        };
        let mut arrangement = battle_arrangement();
        arrangement.arrange_2p = ArrangeOption::Normal;
        arrangement.double_option = DoubleOption::Off;
        arrangement.packed_seed = Some(0x123456);

        apply_battle_target_replication(
            &mut options,
            &arrangement,
            ChartReplicationModeConfig::RivalChart,
            KeyMode::K7,
        );

        assert_eq!(options.arrange, ArrangeOption::Random);
        assert_eq!(options.arrange_seed, Some(0x123456));
        assert_eq!(options.arrange_seed_2p, None);
        assert!(!options.legacy_arrange_seed);
    }

    #[test]
    fn battle_rival_chart_unpacks_double_play_seed() {
        let mut options = PlayStartOptions::default();
        let mut arrangement = battle_arrangement();
        arrangement.packed_seed = Some(0x654321_123456);

        apply_battle_target_replication(
            &mut options,
            &arrangement,
            ChartReplicationModeConfig::RivalChart,
            KeyMode::K14,
        );

        assert_eq!(options.arrange_seed, Some(0x123456));
        assert_eq!(options.arrange_seed_2p, Some(0x654321));
        assert_eq!(options.double_option, DoubleOption::Flip);
    }

    #[test]
    fn battle_rival_chart_copies_replay_arrangement_metadata() {
        let mut options = PlayStartOptions {
            arrange_seed: Some(9),
            s_random_scheme: SRandomScheme::Lm120HzV1,
            ..PlayStartOptions::default()
        };
        let mut arrangement = battle_arrangement();
        arrangement.arrange = ArrangeOption::SRandom;
        arrangement.arrange_2p = ArrangeOption::Normal;
        arrangement.double_option = DoubleOption::Off;
        arrangement.arrange_seed = Some(42);
        arrangement.arrange_seed_2p = None;
        arrangement.arrange_pattern = Some(vec![2, 0, 1]);
        arrangement.legacy_arrange_seed = true;
        arrangement.s_random_scheme = SRandomScheme::Legacy40MsV1;
        arrangement.h_random_threshold_ms = Some(125);

        apply_battle_target_replication(
            &mut options,
            &arrangement,
            ChartReplicationModeConfig::RivalChart,
            KeyMode::K7,
        );

        assert_eq!(options.arrange, ArrangeOption::SRandom);
        assert_eq!(options.arrange_seed, Some(42));
        assert_eq!(options.arrange_pattern, Some(vec![2, 0, 1]));
        assert!(options.legacy_arrange_seed);
        assert_eq!(options.s_random_scheme, SRandomScheme::Legacy40MsV1);
        assert_eq!(options.h_random_threshold_ms, Some(125));
    }

    #[test]
    fn battle_rival_chart_keeps_local_seed_when_target_seed_is_missing() {
        let mut options = PlayStartOptions {
            arrange_seed: Some(77),
            arrange_seed_2p: Some(88),
            ..PlayStartOptions::default()
        };
        let arrangement = battle_arrangement();

        apply_battle_target_replication(
            &mut options,
            &arrangement,
            ChartReplicationModeConfig::RivalChart,
            KeyMode::K14,
        );

        assert_eq!(options.arrange, ArrangeOption::Random);
        assert_eq!(options.arrange_seed, Some(77));
        assert_eq!(options.arrange_seed_2p, Some(88));
    }

    #[test]
    fn battle_rival_chart_keeps_local_seed_when_target_seed_is_invalid() {
        let mut options =
            PlayStartOptions { arrange_seed: Some(77), ..PlayStartOptions::default() };
        let mut arrangement = battle_arrangement();
        arrangement.arrange_2p = ArrangeOption::Normal;
        arrangement.double_option = DoubleOption::Off;
        arrangement.packed_seed = Some(-1);

        apply_battle_target_replication(
            &mut options,
            &arrangement,
            ChartReplicationModeConfig::RivalChart,
            KeyMode::K7,
        );

        assert_eq!(options.arrange, ArrangeOption::Random);
        assert_eq!(options.arrange_seed, Some(77));
    }

    #[test]
    fn battle_replication_none_keeps_local_options() {
        let mut options = PlayStartOptions {
            arrange: ArrangeOption::Mirror,
            arrange_seed: Some(77),
            arrange_pattern: Some(vec![0, 1]),
            ..PlayStartOptions::default()
        };

        apply_battle_target_replication(
            &mut options,
            &battle_arrangement(),
            ChartReplicationModeConfig::None,
            KeyMode::K7,
        );

        assert_eq!(options.arrange, ArrangeOption::Mirror);
        assert_eq!(options.arrange_seed, Some(77));
        assert_eq!(options.arrange_pattern, Some(vec![0, 1]));
    }
}

#[cfg(test)]
mod first_play_score_key_tests {
    use super::*;
    use crate::ln_policy::{ChartLnProfile, LnScorePolicy};
    use crate::select_options::DoubleOptionScoreBucket;

    #[test]
    fn play_skin_score_key_matches_ln_double_and_rule_dimensions() {
        let options =
            PlayStartOptions { double_option: DoubleOption::Battle, ..PlayStartOptions::default() };
        let battle_key = play_skin_score_key(
            [7; 32],
            ChartLnProfile { has_undefined_ln: true, ..ChartLnProfile::default() },
            KeyMode::K7,
            &options,
            LnPolicySetting::ForceCn,
            RuleMode::Dx,
        );
        assert_eq!(battle_key.chart_sha256, [7; 32]);
        assert_eq!(battle_key.ln_policy, LnScorePolicy::ForceCn);
        assert_eq!(battle_key.double_option, DoubleOptionScoreBucket::Battle);
        assert_eq!(battle_key.rule_mode, RuleMode::Dx);

        let presentation_key = play_skin_score_key(
            [7; 32],
            ChartLnProfile::default(),
            KeyMode::K7,
            &PlayStartOptions {
                session_mode: SessionMode::AutoplayBattle,
                double_option: DoubleOption::Battle,
                ..PlayStartOptions::default()
            },
            LnPolicySetting::ForceLn,
            RuleMode::Beatoraja,
        );
        assert_eq!(presentation_key.double_option, DoubleOptionScoreBucket::Off);
    }
}

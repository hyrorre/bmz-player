use super::*;

/// Play 入場直後 (preload 完了前) の placeholder snapshot に、
/// セッション開始時と同じ初期ゲージ・レーン設定を反映する。
/// `install_active_play` でフルスナップショットに置き換わるまでの間、
/// グルーブゲージや緑数字が空表示になるのを防ぐ。
/// ゲージ選択ロジックは `build_game_session_with_input_backend` と揃えること。
pub fn apply_placeholder_session_visuals(
    snapshot: &mut bmz_render::snapshot::RenderSnapshot,
    profile: &ProfileConfig,
    key_mode: KeyMode,
    options: &PlaySessionOptions,
) {
    let play_config_key_mode = play_config_key_mode(key_mode, options);
    let rule_key_mode = if key_mode == KeyMode::K9
        && options.key_mode_conversion == KeyModeConversionConfig::SevenToNine
        && options.seven_to_nine_rule_mode == SevenToNineRuleMode::Keys7
    {
        KeyMode::K7
    } else {
        key_mode
    };
    let mode_config = profile.play_mode_config(play_config_key_mode);
    let hs_fix = options.hs_fix;
    let gauge_type =
        options.gauge_override.unwrap_or_else(|| gauge_type_from_config(profile.play.gauge));
    let gauge_auto_shift = if options.gauge_auto_shift != GaugeAutoShiftMode::Off {
        options.gauge_auto_shift
    } else if options.gauge_override.is_none() {
        gauge_auto_shift_from_config(profile.play.gauge, profile.play.gauge_auto_shift)
    } else {
        GaugeAutoShiftMode::Off
    };
    let bottom_shiftable_gauge = if options.gauge_auto_shift != GaugeAutoShiftMode::Off {
        options.bottom_shiftable_gauge
    } else {
        bottom_shiftable_gauge_from_config(profile.play.bottom_shiftable_gauge)
    };
    let gauge_property =
        options.gauge_property.unwrap_or_else(|| GaugeProperty::from_keymode(rule_key_mode));
    // TOTAL は譜面パース前で不明だが、init/max/border は TOTAL 非依存なので
    // ノーツ数由来のデフォルト TOTAL で代用して問題ない。
    let rule_mode = profile.play.rule_mode;
    let gauge_total = gauge_total_for_chart_and_rule_mode(None, snapshot.total_notes, rule_mode);
    let mut gauge = if gauge_auto_shift != GaugeAutoShiftMode::Off {
        GaugeState::new_with_auto_shift_property_and_rule_mode_and_keymode(
            gauge_type,
            gauge_auto_shift,
            gauge_total,
            snapshot.total_notes,
            gauge_property,
            rule_mode,
            rule_key_mode,
        )
    } else {
        GaugeState::new_with_property_and_rule_mode_and_keymode(
            gauge_type,
            gauge_total,
            snapshot.total_notes,
            gauge_property,
            rule_mode,
            rule_key_mode,
        )
    };
    gauge.set_bottom_shiftable_gauge(bottom_shiftable_gauge);
    if let Some(values) = &options.initial_gauge_values {
        gauge.set_initial_values(values);
    } else if let Some(initial) = options.initial_gauge_value {
        gauge.set_initial_value(initial);
    }
    let current = gauge.current();
    snapshot.gauge = current.value;
    snapshot.gauge_type = current.definition.gauge_type as i32;
    snapshot.gauge_auto_shift = gauge.auto_shift;
    snapshot.gauge_max = current.definition.max;
    snapshot.gauge_border = current.definition.border;

    let speed_locked = options.speed_constraint == bmz_core::course::CourseSpeedConstraint::NoSpeed;
    let lanecover_enabled = !speed_locked && lanecover_enabled_from_mode_config(&mode_config);
    snapshot.lift = if speed_locked { 0.0 } else { lift_from_mode_config(&mode_config) };
    snapshot.lane_cover = if !lanecover_enabled {
        0.0
    } else {
        crate::config::play::clamp_lane_cover_for_lift(
            lane_unit_to_f32(mode_config.sudden),
            snapshot.lift,
        )
    };
    let hispeed_mode = initial_hispeed_mode(&mode_config, hs_fix, speed_locked);
    snapshot.hispeed_mode_index = hispeed_mode_index(hispeed_mode);
    snapshot.base_hispeed_index = match mode_config.base_hispeed {
        BaseHispeedConfig::Classic => 0,
        BaseHispeedConfig::Normal => 1,
    };
    snapshot.normal_hispeed_level =
        crate::config::play::normalize_normal_hispeed_level(mode_config.normal_hispeed_level);
    snapshot.hispeed_config_index = mode_config.hispeed_config().index();
    let target_green_number = mode_config.target_green_number.max(1);
    snapshot.hispeed = if speed_locked {
        1.0
    } else {
        placeholder_hispeed_for_mode(
            &mode_config,
            hispeed_mode,
            target_green_number,
            snapshot.lane_cover,
            snapshot.lift,
            snapshot.now_bpm,
            options.playback_rate_percent,
        )
    };
    snapshot.lanecover_enabled = lanecover_enabled;
    snapshot.lift_enabled = mode_config.lift_enabled;
    snapshot.hidden_enabled = hidden_enabled_from_mode_config(&mode_config);
    snapshot.hispeed_auto_adjust = mode_config.hispeed_auto_adjust;
    snapshot.hidden_cover =
        if speed_locked { 0.0 } else { hidden_cover_from_mode_config(&mode_config) };

    snapshot.key_mode = key_mode;
    // session 構築時と同じく基準 BPM = initial_bpm (decide snapshot の now_bpm)。
    snapshot.main_bpm = snapshot.now_bpm;
    snapshot.fs_threshold_ms =
        bmz_render::chart_graph::rm_skin_fs_threshold_ms(snapshot.judge_rank, rule_key_mode);
    snapshot.judge_timing_offset_ms = (play_offsets_from_profile_for_mode(
        profile,
        play_config_key_mode,
    )
    .visual_offset_us
        / 1_000) as i32;
    snapshot.judge_timing_auto_adjust = profile.judge.visual_offset_auto_adjust;
    let replay_playback = options.replay_player.is_some();
    snapshot.practice_mode = options.session_mode.is_practice();
    snapshot.autoplay = !replay_playback
        && !snapshot.practice_mode
        && (options.session_mode.primary_autoplay() || profile.play.auto_play || options.autoplay);
    snapshot.replay_playback = replay_playback;
    snapshot.assist_flags = options.assist.flags();
    snapshot.assist_extra_note_depth = options.assist.extra_note_depth;
    snapshot.assist_mine_mode = options.assist.mine_mode as i64;
    snapshot.assist_scroll_mode = options.assist.scroll_mode as i64;
    snapshot.assist_long_note_mode = options.assist.long_note_mode as i64;
    snapshot.judge_area = options.assist.judge_area;
    snapshot.mark_processed_note = options.assist.mark_note;
    snapshot.bpm_guide = options.assist.bpm_guide;
    snapshot.score_save_enabled = options.session_mode.score_save_enabled()
        && !snapshot.autoplay
        && !snapshot.replay_playback
        && !snapshot.practice_mode
        && !options.score_save_disabled
        && options.assist_runtime.score_update_enabled();
    snapshot.bga_enabled =
        bga_enabled_from_profile(profile, snapshot.autoplay, snapshot.replay_playback);
    snapshot.bga_stretch = bga_stretch_from_profile(profile);
    snapshot.target_ex_score = options
        .resolved_target
        .as_ref()
        .map(|target| target.ex_score)
        .or_else(|| options.target.target_ex_score(snapshot.total_notes));
    snapshot.target = options
        .resolved_target
        .as_ref()
        .map(|target| target.name.clone())
        .unwrap_or_else(|| options.target.as_string());

    snapshot.note_display_duration_ms =
        crate::screens::play_snapshot::display_duration_ms_for_bpm_hispeed(
            crate::screens::play_snapshot::effective_bpm_for_playback_rate(
                f64::from(snapshot.now_bpm),
                options.playback_rate_percent,
            ) as f32,
            snapshot.hispeed,
            snapshot.lane_cover,
            snapshot.lift,
            1.0,
        )
        .round()
        .clamp(0.0, i32::MAX as f32) as i32;

    let initial_bpm = snapshot.now_bpm;
    let max_bpm = snapshot.max_bpm.max(initial_bpm);
    snapshot.adjusted_cover_progress = bmz_render::chart_graph::compute_adjusted_cover_progress(
        snapshot.hidden_enabled,
        snapshot.lane_cover,
        snapshot.lift,
        snapshot.hsfix_index,
        initial_bpm,
        max_bpm,
        initial_bpm,
    );
    snapshot.adjusted_rate = bmz_render::chart_graph::compute_adjusted_rate(
        snapshot.hidden_enabled,
        snapshot.lanecover_enabled,
        snapshot.hsfix_index,
        initial_bpm,
        max_bpm,
        initial_bpm,
    );
    snapshot.adjusted_rate_adot = snapshot.adjusted_rate.map(|rate| (rate * 100.0).floor() as i32);

    // プロファイルのスキンオフセット (位置調整)。スクラッチ回転角は session が
    // 必要なので install 後の refresh に任せる。
    let mut offsets = bmz_render::skin_offset::SkinOffsetValues::default();
    for offset in skin_offsets_from_profile(profile, key_mode, options.session_mode) {
        offsets.set(
            offset.id,
            bmz_render::skin_offset::SkinOffsetValue {
                x: offset.x,
                y: offset.y,
                w: offset.w,
                h: offset.h,
                r: offset.r,
                a: offset.a,
            },
        );
    }
    snapshot.skin_offsets = offsets;
}

pub fn build_game_session(
    chart: Arc<PlayableChart>,
    profile: &ProfileConfig,
    options: PlaySessionOptions,
) -> GameSession {
    build_game_session_with_input_backend(chart, profile, options, Box::new(NullInputBackend))
}

pub fn build_game_session_with_input_backend(
    chart: Arc<PlayableChart>,
    profile: &ProfileConfig,
    options: PlaySessionOptions,
    input_backend: Box<dyn InputBackend>,
) -> GameSession {
    let session_mode = options.session_mode;
    let battle_opponent_options = options.battle_opponent.clone();
    let battle_opponent_chart = options.opponent_chart.clone();
    let chart_key_mode = chart.metadata.key_mode;
    let play_config_key_mode = play_config_key_mode(chart_key_mode, &options);
    let battle_presentation = (session_mode.is_battle() || battle_opponent_options.is_some())
        && matches!(
            (play_config_key_mode, chart_key_mode),
            (KeyMode::K5, KeyMode::K10) | (KeyMode::K7, KeyMode::K14)
        );
    let mode_config = profile.play_mode_config(play_config_key_mode);
    let hs_fix = options.hs_fix;
    let seven_to_nine_7k_rule = chart_key_mode == KeyMode::K9
        && options.key_mode_conversion == KeyModeConversionConfig::SevenToNine
        && options.seven_to_nine_rule_mode == SevenToNineRuleMode::Keys7;
    let primary_key_mode = if seven_to_nine_7k_rule {
        KeyMode::K7
    } else if battle_presentation {
        match chart_key_mode {
            KeyMode::K10 => KeyMode::K5,
            KeyMode::K14 => KeyMode::K7,
            _ => chart_key_mode,
        }
    } else {
        chart_key_mode
    };
    let display_only_lane_mask =
        if battle_presentation { second_player_lane_mask() } else { [false; LANE_COUNT] };
    let replay_lane_mask = None;
    let replay_lane_projection = seven_to_nine_7k_rule.then(|| {
        seven_to_nine_replay_lane_projection(
            options.seven_to_nine_pattern,
            options.seven_to_nine_type,
        )
    });
    let gauge_type =
        options.gauge_override.unwrap_or_else(|| gauge_type_from_config(profile.play.gauge));
    let gauge_auto_shift = if options.gauge_auto_shift != GaugeAutoShiftMode::Off {
        options.gauge_auto_shift
    } else if options.gauge_override.is_none() {
        gauge_auto_shift_from_config(profile.play.gauge, profile.play.gauge_auto_shift)
    } else {
        GaugeAutoShiftMode::Off
    };
    let bottom_shiftable_gauge = if options.gauge_auto_shift != GaugeAutoShiftMode::Off {
        options.bottom_shiftable_gauge
    } else {
        bottom_shiftable_gauge_from_config(profile.play.bottom_shiftable_gauge)
    };
    let initial_gauge_value = options.initial_gauge_value;
    let initial_gauge_values = options.initial_gauge_values.clone();
    let initial_course_combo = options.initial_course_combo.unwrap_or(0);
    let replay_player = options.replay_player;
    let is_replay = replay_player.is_some();
    let is_full_replay = is_replay && replay_lane_mask.is_none();
    let autoplay_enabled = !is_replay
        && !session_mode.is_practice()
        && (session_mode.primary_autoplay() || profile.play.auto_play || options.autoplay);
    let autoplay = if autoplay_enabled {
        Some(AutoplayController::default())
    } else if session_mode == SessionMode::GBattle
        && battle_presentation
        && battle_opponent_options.is_none()
    {
        let mut lanes = Lane::ALL
            .into_iter()
            .filter(|lane| display_only_lane_mask[lane.index()])
            .collect::<Vec<_>>();
        if options.double_option == DoubleOption::BattleAutoScratch
            && !lanes.contains(&Lane::Scratch)
        {
            lanes.push(Lane::Scratch);
        }
        Some(AutoplayController::for_lanes(&lanes))
    } else if options.double_option == DoubleOption::BattleAutoScratch {
        Some(AutoplayController::for_lanes(&[Lane::Scratch, Lane::Scratch2]))
    } else {
        None
    };
    let input_offset_auto_adjust_enabled = profile.judge.visual_offset_auto_adjust;
    let input_offset_auto_adjust =
        if input_offset_auto_adjust_enabled && !autoplay_enabled && !is_full_replay {
            Some(InputOffsetAutoAdjustState::default())
        } else {
            None
        };
    // `chart` is built from the source file and already has the selected LN
    // policy, course override, and double option applied.  Derive the gameplay
    // denominator here instead of using the policy-independent library count.
    let scored_total_notes = if battle_presentation {
        bmz_gameplay::score::scored_note_count_excluding_lanes(&chart, &display_only_lane_mask)
    } else {
        scored_note_count(&chart)
    };
    let rule_mode = profile.play.rule_mode;
    let input_system = InputSystem {
        backend: input_backend,
        translator: Box::new(DefaultInputTranslator {
            binding: lane_binding_for_chart_with_slots(
                &profile.input,
                play_config_key_mode,
                options.gamepad_slots,
            ),
        }),
        bounce_filter: InputBounceFilter::new(input_bounce_config_from_profile(&profile.input)),
    };

    let timing_map = bmz_chart::timing::TimingMap::from_chart_timing_events(
        chart.metadata.initial_bpm,
        &chart.timing_events,
    );
    let speed_locked = options.speed_constraint == bmz_core::course::CourseSpeedConstraint::NoSpeed;
    let hispeed_mode = initial_hispeed_mode(&mode_config, hs_fix, speed_locked);
    let target_green_number = mode_config.target_green_number.max(1);
    let lift = if speed_locked { 0.0 } else { lift_from_mode_config(&mode_config) };
    let lane_cover = if speed_locked {
        0.0
    } else {
        crate::config::play::clamp_lane_cover_for_lift(lane_unit_to_f32(mode_config.sudden), lift)
    };
    let hsfix_base_bpm = hsfix_base_bpm_for_chart(&chart, &timing_map, hs_fix);
    let hispeed = if speed_locked {
        1.0
    } else {
        initial_hispeed_for_mode(
            &mode_config,
            hispeed_mode,
            target_green_number,
            lane_cover,
            lift,
            &chart,
            &timing_map,
            hs_fix,
            options.playback_rate_percent,
        )
    };

    // Course judge constraints narrow the judge window so the corresponding
    // judge band is unreachable: NoGood zeroes good_us, NoGreat zeroes both
    // great_us and good_us.  Mirrors beatoraja JudgeManager's *JudgeWindowRate
    // = 0 path.
    let base_judge_windows = apply_judge_constraint_to_windows(
        crate::assist::apply_custom_judge_windows(
            judge_windows_for_keymode_and_rule_mode(primary_key_mode, rule_mode),
            options.assist,
        ),
        options.judge_constraint,
    );
    let base_judge_window = base_judge_windows.note;

    let gauge_total =
        gauge_total_for_chart_and_rule_mode(chart.metadata.total, scored_total_notes, rule_mode);
    // 単曲時はチャートのキーモードから GaugeProperty を導出、コース時は
    // `apply_course_constraints` が CourseGaugeConstraint から決めた値を使う。
    let gauge_property =
        options.gauge_property.unwrap_or_else(|| GaugeProperty::from_keymode(primary_key_mode));
    let mut gauge = {
        if gauge_auto_shift != GaugeAutoShiftMode::Off {
            let mut gauge = GaugeState::new_with_auto_shift_property_and_rule_mode_and_keymode(
                gauge_type,
                gauge_auto_shift,
                gauge_total,
                scored_total_notes,
                gauge_property,
                rule_mode,
                primary_key_mode,
            );
            gauge.set_bottom_shiftable_gauge(bottom_shiftable_gauge);
            gauge
        } else {
            GaugeState::new_with_property_and_rule_mode_and_keymode(
                gauge_type,
                gauge_total,
                scored_total_notes,
                gauge_property,
                rule_mode,
                primary_key_mode,
            )
        }
    };
    // Course play carries the previous chart's gauge value over; this overrides
    // the initial value computed by GaugeState::new* above.
    if let Some(values) = &initial_gauge_values {
        gauge.set_initial_values(values);
    } else if let Some(initial) = initial_gauge_value {
        gauge.set_initial_value(initial);
    }
    let opponent_gauge = battle_presentation.then(|| {
        if let Some(opponent_gauge_type) = options.opponent_gauge_override {
            GaugeState::new_with_property_and_rule_mode_and_keymode(
                opponent_gauge_type,
                gauge_total,
                scored_total_notes,
                gauge_property,
                rule_mode,
                primary_key_mode,
            )
        } else {
            gauge.clone()
        }
    });
    let opponent_score =
        battle_presentation.then(|| ScoreState::for_rule_mode(primary_key_mode, rule_mode));
    let battle_opponent =
        battle_opponent_options.zip(battle_opponent_chart).map(|(opponent, chart)| {
            let key_mode = chart.metadata.key_mode;
            let scored_total_notes = scored_note_count(&chart);
            let base_judge_windows = judge_windows_for_keymode_and_rule_mode(key_mode, rule_mode);
            let gauge_type = opponent.gauge.unwrap_or(gauge_type);
            let gauge_total = gauge_total_for_chart_and_rule_mode(
                chart.metadata.total,
                scored_total_notes,
                rule_mode,
            );
            bmz_gameplay::session::BattleOpponentSession {
                judge: JudgeEngine::new_with_window_set_algorithm_and_keymode(
                    scale_judge_windows_for_playback_rate(
                        judge_windows_for_rule_mode_and_keymode(
                            base_judge_windows,
                            judge_percent_at_time_for_keymode(
                                chart.metadata.judge_rank_spec,
                                &chart.judge_rank_events,
                                TimeUs(0),
                                key_mode,
                                rule_mode,
                            ),
                            rule_mode,
                            key_mode,
                        ),
                        options.playback_rate_percent,
                    ),
                    rule_mode,
                    judge_algorithm_from_config(profile.judge.judge_algorithm),
                    key_mode,
                ),
                gauge: GaugeState::new_with_property_and_rule_mode_and_keymode(
                    gauge_type,
                    gauge_total,
                    scored_total_notes,
                    GaugeProperty::from_keymode(key_mode),
                    rule_mode,
                    key_mode,
                ),
                chart,
                key_mode,
                scored_total_notes,
                base_judge_windows,
                rule_mode,
                score: ScoreState::for_rule_mode(key_mode, rule_mode),
                autoplay: opponent.replay_player.is_none().then(AutoplayController::default),
                replay_player: opponent.replay_player,
                display_uses_primary_arrangement: session_mode == SessionMode::GBattle,
                publish_display_judgements: session_mode.uses_battle_skin(),
                gauge_increase_started_at: None,
                gauge_max_started_at: None,
                full_combo_started_at: None,
                lane_keyon_started_at: Default::default(),
            }
        });

    let mut audio_clock = AudioClock::stopped(options.sample_rate);
    let _ = audio_clock.set_playback_rate_percent(options.playback_rate_percent);

    let mut judge = JudgeEngine::new_with_window_set_algorithm_and_keymode(
        scale_judge_windows_for_playback_rate(
            judge_windows_for_rule_mode_and_keymode(
                base_judge_windows,
                judge_percent_at_time_for_keymode(
                    chart.metadata.judge_rank_spec,
                    &chart.judge_rank_events,
                    TimeUs(0),
                    primary_key_mode,
                    rule_mode,
                ),
                rule_mode,
                primary_key_mode,
            ),
            options.playback_rate_percent,
        ),
        rule_mode,
        judge_algorithm_from_config(profile.judge.judge_algorithm),
        primary_key_mode,
    );
    if let Some(projection) = &replay_lane_projection {
        judge.set_scratch_lane_mask(projection.playback_scratch_lane_mask);
    }

    GameSession {
        gauge,
        opponent_gauge,
        judge,
        base_judge_window,
        base_judge_windows,
        rule_mode,
        audio_clock,
        chart,
        play_config_key_mode,
        primary_key_mode,
        scored_total_notes,
        assist: options.assist_runtime,
        timing_map,
        input_system,
        score: ScoreState::for_rule_mode(primary_key_mode, rule_mode),
        opponent_score,
        battle_opponent,
        course_combo_carry: initial_course_combo,
        course_combo_carry_active: initial_course_combo > 0,
        course_max_combo: initial_course_combo,
        replay_recorder: ReplayRecorder::default(),
        replay_player,
        replay_lane_projection,
        replay_lane_mask,
        display_only_lane_mask,
        autoplay,
        recent_inputs: Vec::new(),
        lane_keyon_started_at: Default::default(),
        lane_keyoff_started_at: Default::default(),
        lane_scratch_direction: Default::default(),
        lane_scratch_angle_delta_ms: Default::default(),
        scratch_angle_last_render_at: None,
        lane_auto_release_at: Default::default(),
        recent_judgements: Vec::new(),
        recent_display_judgements: Vec::new(),
        pending_skin_events: Vec::new(),
        next_skin_event_sequence: 0,
        result_judgements: Default::default(),
        hit_error_ring: HitErrorRing::default(),
        input_offset_auto_adjust_enabled,
        input_offset_auto_adjust,
        gauge_increase_started_at: None,
        opponent_gauge_increase_started_at: None,
        gauge_max_started_at: None,
        opponent_gauge_max_started_at: None,
        full_combo_started_at: None,
        opponent_full_combo_started_at: None,
        bgm_scheduler: BgmScheduler::default(),
        auto_keysound_scheduler: AutoKeysoundScheduler::default(),
        offsets: play_offsets_from_profile_for_mode(profile, play_config_key_mode),
        audio_mix: audio_mix_from_profile(profile),
        hispeed,
        hispeed_mode,
        base_hispeed_mode: base_hispeed_mode(mode_config.base_hispeed),
        floating_policy: floating_policy(mode_config.floating_policy),
        normal_hispeed_level: crate::config::play::normalize_normal_hispeed_level(
            mode_config.normal_hispeed_level,
        ),
        target_green_number,
        constant_enabled: mode_config.constant_enabled
            && session_mode != crate::select_options::SessionMode::Practice,
        constant_fade_ms: mode_config.constant_fade_ms,
        guide_se_enabled: profile.play.guide_se,
        note_retention: profile.play.note_retention,
        hsfix_base_bpm,
        lift,
        lane_cover,
        lane_cover_visible: lanecover_enabled_from_mode_config(&mode_config),
        lane_cover_changing: false,
        lanecover_enabled: lanecover_enabled_from_mode_config(&mode_config),
        lift_enabled: mode_config.lift_enabled,
        hidden_enabled: hidden_enabled_from_mode_config(&mode_config),
        hispeed_auto_adjust: mode_config.hispeed_auto_adjust,
        hidden_cover: if speed_locked { 0.0 } else { hidden_cover_from_mode_config(&mode_config) },
        skin_offsets: skin_offsets_from_profile(profile, play_config_key_mode, session_mode),
        bga_enabled: bga_enabled_from_profile(profile, autoplay_enabled, is_replay),
        poor_bga_duration_us: poor_bga_duration_us_from_profile(profile),
        bga_stretch: bga_stretch_from_profile(profile),
        show_ln_tail_cap: profile.play.show_ln_tail_cap,
        lane_hcn_timer: [None; bmz_core::lane::LANE_COUNT],
        lane_hcn_keysound_muted: [None; bmz_core::lane::LANE_COUNT],
        pending_keysounds: Vec::new(),
        pending_keysound_volumes: Vec::new(),
        hsfix_index: hsfix_index_from_option(hs_fix),
        input_timestamp_anchor: None,
        pending_mine_hits: Vec::new(),
        session_mode_index: crate::skin_extension::session_mode_index(session_mode) as u8,
        state: PlayState::Ready,
        last_hcn_gauge_at: None,
    }
}

pub(super) fn clamp_hispeed(hispeed: f32) -> f32 {
    crate::config::play::clamp_hispeed(hispeed)
}

pub(super) fn hsfix_index_from_option(option: HsFixOption) -> i32 {
    match option {
        HsFixOption::Off => 0,
        HsFixOption::StartBpm => 1,
        HsFixOption::MaxBpm => 2,
        HsFixOption::MainBpm => 3,
        HsFixOption::MinBpm => 4,
    }
}

pub(super) fn play_config_key_mode(
    chart_key_mode: KeyMode,
    options: &PlaySessionOptions,
) -> KeyMode {
    options.play_config_key_mode.unwrap_or({
        if matches!(options.double_option, DoubleOption::Battle | DoubleOption::BattleAutoScratch) {
            match chart_key_mode {
                KeyMode::K10 => KeyMode::K5,
                KeyMode::K14 => KeyMode::K7,
                _ => chart_key_mode,
            }
        } else {
            chart_key_mode
        }
    })
}

pub(super) fn apply_judge_constraint_to_windows(
    windows: JudgeWindows,
    constraint: bmz_core::course::CourseJudgeConstraint,
) -> JudgeWindows {
    JudgeWindows {
        note: apply_judge_constraint_to_window(windows.note, constraint),
        scratch: apply_judge_constraint_to_window(windows.scratch, constraint),
        long_note_end: apply_judge_constraint_to_window(windows.long_note_end, constraint),
        long_scratch_end: apply_judge_constraint_to_window(windows.long_scratch_end, constraint),
        long_note_release_margin_us: windows.long_note_release_margin_us,
        long_scratch_release_margin_us: windows.long_scratch_release_margin_us,
    }
}

pub(super) fn apply_judge_constraint_to_window(
    mut window: JudgeWindow,
    constraint: bmz_core::course::CourseJudgeConstraint,
) -> JudgeWindow {
    match constraint {
        bmz_core::course::CourseJudgeConstraint::Normal => {}
        bmz_core::course::CourseJudgeConstraint::NoGood => {
            window.good_us = 0;
        }
        bmz_core::course::CourseJudgeConstraint::NoGreat => {
            window.great_us = 0;
            window.good_us = 0;
        }
    }
    window
}

pub(super) const fn base_hispeed_mode(base: BaseHispeedConfig) -> HispeedMode {
    match base {
        BaseHispeedConfig::Normal => HispeedMode::Normal,
        BaseHispeedConfig::Classic => HispeedMode::Classic,
    }
}

pub(super) const fn floating_policy(policy: FloatingPolicyConfig) -> FloatingPolicy {
    match policy {
        FloatingPolicyConfig::Disabled => FloatingPolicy::Disabled,
        FloatingPolicyConfig::Toggle => FloatingPolicy::Toggle,
        FloatingPolicyConfig::Locked => FloatingPolicy::Locked,
    }
}

pub(super) fn initial_hispeed_mode(
    config: &PlayModeConfig,
    hs_fix: HsFixOption,
    speed_locked: bool,
) -> HispeedMode {
    if speed_locked {
        return HispeedMode::Classic;
    }
    match config.floating_policy {
        FloatingPolicyConfig::Disabled => base_hispeed_mode(config.base_hispeed),
        FloatingPolicyConfig::Locked => HispeedMode::Floating,
        FloatingPolicyConfig::Toggle => {
            if hs_fix == HsFixOption::Off {
                base_hispeed_mode(config.base_hispeed)
            } else {
                HispeedMode::Floating
            }
        }
    }
}

pub(super) fn hispeed_mode_index(mode: HispeedMode) -> i32 {
    match mode {
        HispeedMode::Normal | HispeedMode::Classic => 0,
        HispeedMode::Floating => 1,
    }
}

pub(super) fn initial_hispeed_for_mode(
    mode_config: &PlayModeConfig,
    hispeed_mode: HispeedMode,
    target_green_number: u32,
    lane_cover: f32,
    lift: f32,
    chart: &PlayableChart,
    timing_map: &bmz_chart::timing::TimingMap,
    hs_fix: HsFixOption,
    playback_rate_percent: u16,
) -> f32 {
    let now_bpm = crate::screens::play_snapshot::effective_bpm_for_playback_rate(
        hsfix_base_bpm_for_chart(chart, timing_map, hs_fix),
        playback_rate_percent,
    );
    let scroll_multiplier =
        crate::screens::play_snapshot::current_scroll_multiplier(chart, timing_map, TimeUs(0));
    match hispeed_mode {
        HispeedMode::Classic => clamp_hispeed(mode_config.hispeed),
        HispeedMode::Normal => {
            let green =
                crate::config::play::normal_hispeed_green_number(mode_config.normal_hispeed_level);
            clamp_hispeed(crate::screens::play_snapshot::hispeed_for_green_number_values(
                green as f32,
                1.0,
                now_bpm,
                scroll_multiplier,
            ))
        }
        HispeedMode::Floating => {
            let visible_max = crate::config::play::visible_lane_fraction(lane_cover, lift);
            clamp_hispeed(crate::screens::play_snapshot::hispeed_for_green_number_values(
                target_green_number as f32,
                visible_max,
                now_bpm,
                scroll_multiplier,
            ))
        }
    }
}

pub(super) fn hsfix_base_bpm_for_chart(
    chart: &PlayableChart,
    timing_map: &bmz_chart::timing::TimingMap,
    hs_fix: HsFixOption,
) -> f64 {
    match hs_fix {
        HsFixOption::Off | HsFixOption::StartBpm => chart.metadata.initial_bpm,
        HsFixOption::MinBpm => chart
            .timing_events
            .iter()
            .filter_map(|event| match event.kind {
                TimingEventKind::BpmChange { bpm } => Some(bpm),
                TimingEventKind::Stop { .. } => None,
            })
            .fold(chart.metadata.initial_bpm, f64::min),
        HsFixOption::MaxBpm => chart
            .timing_events
            .iter()
            .filter_map(|event| match event.kind {
                TimingEventKind::BpmChange { bpm } => Some(bpm),
                TimingEventKind::Stop { .. } => None,
            })
            .fold(chart.metadata.initial_bpm, f64::max),
        HsFixOption::MainBpm => main_bpm_for_chart(chart, timing_map),
    }
}

pub(super) fn main_bpm_for_chart(
    chart: &PlayableChart,
    timing_map: &bmz_chart::timing::TimingMap,
) -> f64 {
    let mut counted = std::collections::HashSet::new();
    let mut counts: Vec<(f64, u32)> = Vec::new();
    for note in chart.lane_notes.iter().flatten() {
        if matches!(note.kind, NoteKind::Invisible | NoteKind::Mine) {
            continue;
        }
        counted.insert(note.id);
        let bpm = timing_map.bpm_at_time(note.time);
        if let Some((_, count)) =
            counts.iter_mut().find(|(value, _)| value.to_bits() == bpm.to_bits())
        {
            *count = count.saturating_add(1);
        } else {
            counts.push((bpm, 1));
        }
    }
    for long in &chart.long_notes {
        if !counted.insert(long.start_note_id) {
            continue;
        }
        let bpm = timing_map.bpm_at_time(long.start_time);
        if let Some((_, count)) =
            counts.iter_mut().find(|(value, _)| value.to_bits() == bpm.to_bits())
        {
            *count = count.saturating_add(1);
        } else {
            counts.push((bpm, 1));
        }
    }
    counts
        .into_iter()
        .max_by_key(|(_, count)| *count)
        .map(|(bpm, _)| bpm)
        .unwrap_or(chart.metadata.initial_bpm)
}

pub(super) fn placeholder_hispeed_for_mode(
    mode_config: &PlayModeConfig,
    hispeed_mode: HispeedMode,
    target_green_number: u32,
    lane_cover: f32,
    lift: f32,
    now_bpm: f32,
    playback_rate_percent: u16,
) -> f32 {
    let now_bpm = crate::screens::play_snapshot::effective_bpm_for_playback_rate(
        f64::from(now_bpm),
        playback_rate_percent,
    );
    match hispeed_mode {
        HispeedMode::Classic => clamp_hispeed(mode_config.hispeed),
        HispeedMode::Normal => {
            let green =
                crate::config::play::normal_hispeed_green_number(mode_config.normal_hispeed_level);
            clamp_hispeed(crate::screens::play_snapshot::hispeed_for_green_number_values(
                green as f32,
                1.0,
                now_bpm,
                1.0,
            ))
        }
        HispeedMode::Floating => {
            let visible_max = crate::config::play::visible_lane_fraction(lane_cover, lift);
            clamp_hispeed(crate::screens::play_snapshot::hispeed_for_green_number_values(
                target_green_number as f32,
                visible_max,
                now_bpm,
                1.0,
            ))
        }
    }
}

pub(crate) fn judge_algorithm_from_config(value: JudgeAlgorithmConfig) -> JudgeAlgorithm {
    match value {
        JudgeAlgorithmConfig::Combo => JudgeAlgorithm::Combo,
        JudgeAlgorithmConfig::Duration => JudgeAlgorithm::Duration,
        JudgeAlgorithmConfig::Lowest => JudgeAlgorithm::Lowest,
    }
}

pub(super) fn hidden_cover_from_mode_config(config: &PlayModeConfig) -> f32 {
    if config.lane_effect.hidden_enabled() { lane_unit_to_f32(config.hidden) } else { 0.0 }
}

pub(super) fn lanecover_enabled_from_mode_config(config: &PlayModeConfig) -> bool {
    config.lane_effect.sudden_enabled()
}

pub(super) fn lift_from_mode_config(config: &PlayModeConfig) -> f32 {
    if config.lift_enabled { lane_unit_to_f32(config.lift) } else { 0.0 }
}

pub(super) fn hidden_enabled_from_mode_config(config: &PlayModeConfig) -> bool {
    config.lane_effect.hidden_enabled()
}

pub(super) fn poor_bga_duration_us_from_profile(profile: &ProfileConfig) -> i64 {
    i64::from(profile.play.misslayer_duration_ms.min(5_000)) * 1_000
}

pub(super) fn bga_stretch_from_profile(profile: &ProfileConfig) -> i32 {
    match profile.play.bga_expand {
        BgaExpandConfig::Full => 0,
        BgaExpandConfig::KeepAspect => 1,
        BgaExpandConfig::Off => 8,
    }
}

pub(super) fn bga_enabled_from_profile(
    profile: &ProfileConfig,
    autoplay: bool,
    replay: bool,
) -> bool {
    match profile.play.bga {
        BgaModeConfig::On => true,
        BgaModeConfig::Auto => autoplay || replay,
        BgaModeConfig::Off => false,
    }
}

pub(super) fn skin_offsets_from_profile(
    profile: &ProfileConfig,
    key_mode: KeyMode,
    session_mode: SessionMode,
) -> Vec<PlaySkinOffset> {
    // 各 key mode のアクティブな編集値だけを使う。`skin.history` はスキン切替時に
    // このスロットへ復元するためのキャッシュであり、実行時に直接参照しない。
    play_skin_selection_for_session(&profile.skin, key_mode, session_mode)
        .offsets
        .iter()
        .map(|offset| PlaySkinOffset {
            id: offset.id,
            x: offset.x,
            y: offset.y,
            w: offset.w,
            h: offset.h,
            r: offset.r,
            a: offset.a,
        })
        .collect()
}

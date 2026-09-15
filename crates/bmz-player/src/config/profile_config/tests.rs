use super::*;

#[test]
fn ui_scale_preserves_old_profiles_and_roundtrips() {
    let mut profile = ProfileConfig::new_default("default", "Player", 0);
    assert_eq!(profile.ui.scale_percent, 100);
    let mut value = toml::Value::try_from(&profile).unwrap();
    value.get_mut("ui").unwrap().as_table_mut().unwrap().remove("scale_percent");
    let decoded: ProfileConfig = value.try_into().unwrap();
    assert_eq!(decoded.ui.zoom_factor(), 1.0);

    profile.ui.scale_percent = 125;
    let encoded = toml::to_string(&profile).unwrap();
    let decoded: ProfileConfig = toml::from_str(&encoded).unwrap();
    assert_eq!(decoded.ui.scale_percent, 125);
    assert_eq!(decoded.ui.zoom_factor(), 1.25);

    profile.ui.scale_percent = 0;
    assert_eq!(profile.ui.zoom_factor(), 1.0);
    profile.ui.scale_percent = u32::MAX;
    assert_eq!(profile.ui.zoom_factor(), 2.0);
}

#[test]
fn chart_replication_mode_uses_beatoraja_names_and_defaults_to_rival_chart() {
    #[derive(Serialize, Deserialize)]
    struct ModeWrapper {
        mode: ChartReplicationModeConfig,
    }

    let encoded =
        toml::to_string(&ModeWrapper { mode: ChartReplicationModeConfig::RivalOption }).unwrap();
    assert_eq!(encoded.trim(), "mode = \"RIVALOPTION\"");
    assert_eq!(
        toml::from_str::<ModeWrapper>("mode = \"RIVALCHART\"").unwrap().mode,
        ChartReplicationModeConfig::RivalChart
    );

    let profile = ProfileConfig::new_default("default", "Player", 0);
    let mut value = toml::Value::try_from(&profile).unwrap();
    value
        .get_mut("rival")
        .and_then(toml::Value::as_table_mut)
        .unwrap()
        .remove("chart_replication_mode");
    let decoded: ProfileConfig = value.try_into().unwrap();
    assert_eq!(decoded.rival.chart_replication_mode, ChartReplicationModeConfig::RivalChart);
}

#[test]
fn legacy_score_judge_algorithm_is_loaded_as_duration() {
    let judge: JudgeConfig = toml::from_str(
        r#"
            input_offset_us = 0
            visual_offset_us = 0
            judge_algorithm = "Score"
            fast_slow_display_threshold_ms = 0
            fast_slow_display_scope = "Auto"
            "#,
    )
    .unwrap();

    assert_eq!(judge.judge_algorithm, JudgeAlgorithmConfig::Duration);
}

#[test]
fn play_defaults_uses_default_misslayer_duration_for_old_profiles() {
    let play: PlayDefaultsConfig = toml::from_str(
        r#"
            gauge = "Normal"
            random = "Off"
            lane_effect = "Off"
            assist = "None"
            auto_play = false
            "#,
    )
    .unwrap();

    assert_eq!(play.target, TargetOptionConfig::None);
    assert_eq!(play.rule_mode, RuleMode::Beatoraja);
    assert_eq!(play.ln_mode_policy, LnPolicySetting::AutoLn);
    assert_eq!(play.key_mode_conversion, KeyModeConversionConfig::Off);
    assert_eq!(play.seven_to_nine_pattern, SevenToNinePattern::Sc9Key1To7);
    assert_eq!(play.seven_to_nine_type, SevenToNineType::Fixed);
    assert_eq!(play.seven_to_nine_rule_mode, SevenToNineRuleMode::Keys7);
    assert!(!play.seven_to_six);
    assert_eq!(play.bga, BgaModeConfig::On);
    assert_eq!(play.bga_expand, BgaExpandConfig::KeepAspect);
    assert_eq!(play.misslayer_duration_ms, 500);
    assert_eq!(play.play_exit_hold_ms, 1000);
    assert_eq!(play.bottom_shiftable_gauge, BottomShiftableGaugeConfig::AssistEasy);
    assert!(!play.note_retention);
}

#[test]
fn note_retention_defaults_off_and_roundtrips() {
    let mut play = ProfileConfig::new_default("default", "Default", 0).play;
    assert!(!play.note_retention);

    play.note_retention = true;
    let encoded = toml::to_string(&play).unwrap();
    let decoded: PlayDefaultsConfig = toml::from_str(&encoded).unwrap();

    assert!(decoded.note_retention);
}

#[test]
fn key_mode_conversion_roundtrips_in_play_defaults() {
    let mut play = ProfileConfig::new_default("default", "Default", 0).play;
    play.key_mode_conversion = KeyModeConversionConfig::SevenToNine;
    play.seven_to_nine_pattern = SevenToNinePattern::Sc1Key3To9;
    play.seven_to_nine_type = SevenToNineType::Alternation;
    play.seven_to_nine_rule_mode = SevenToNineRuleMode::Keys9;

    let encoded = toml::to_string(&play).unwrap();
    let decoded: PlayDefaultsConfig = toml::from_str(&encoded).unwrap();

    assert_eq!(decoded.key_mode_conversion, KeyModeConversionConfig::SevenToNine);
    assert_eq!(decoded.seven_to_nine_pattern, SevenToNinePattern::Sc1Key3To9);
    assert_eq!(decoded.seven_to_nine_type, SevenToNineType::Alternation);
    assert_eq!(decoded.seven_to_nine_rule_mode, SevenToNineRuleMode::Keys9);
}

#[test]
fn legacy_seven_to_six_migrates_to_key_mode_conversion() {
    let mut profile = ProfileConfig::new_default("default", "Default", 0);
    profile.play.seven_to_six = true;

    profile.migrate_legacy_key_mode_conversion();

    assert_eq!(profile.play.key_mode_conversion, KeyModeConversionConfig::SevenToSix);
    assert!(!profile.play.seven_to_six);
}

#[test]
fn seven_to_nine_type_cycles_in_beatoraja_order() {
    assert_eq!(SevenToNineType::Fixed.next(true), SevenToNineType::NoMashing);
    assert_eq!(SevenToNineType::NoMashing.next(true), SevenToNineType::Alternation);
    assert_eq!(SevenToNineType::Alternation.next(true), SevenToNineType::Fixed);
    assert_eq!(SevenToNineType::Fixed.next(false), SevenToNineType::Alternation);
}

#[test]
fn assist_config_migrates_legacy_value_and_roundtrips_all_modifiers() {
    #[derive(Deserialize)]
    struct AssistWrapper {
        value: AssistOptionConfig,
    }

    let legacy = toml::from_str::<AssistWrapper>("value = \"LegacyNote\"").unwrap().value;
    assert_eq!(legacy.long_note_mode, AssistLongNoteMode::Remove);

    let config = AssistOptionConfig {
        expand_judge: true,
        judge_area: true,
        mark_note: true,
        bpm_guide: true,
        scroll_mode: AssistScrollMode::Add,
        long_note_mode: AssistLongNoteMode::AddAll,
        mine_mode: AssistMineMode::AddNear,
        key_pgreat_rate: 321,
        extra_note_depth: 3,
        extra_note_scratch: true,
        ..Default::default()
    };
    let encoded = toml::to_string(&config).unwrap();
    let decoded: AssistOptionConfig = toml::from_str(&encoded).unwrap();
    assert_eq!(decoded, config);
}

#[test]
fn lane_view_migrates_old_hispeed_fields_to_classic_floating_defaults() {
    let lane: LaneViewConfig = toml::from_str(
        r#"
            hispeed = 2.0
            hispeed_mode = "Normal"
            sudden = 0
            lift = 0
            hidden = 0
            target_green_number = 300
            "#,
    )
    .unwrap();

    assert_eq!(lane.hispeed, 2.0);
    assert_eq!(lane.base_hispeed, BaseHispeedConfig::Classic);
    assert_eq!(lane.floating_policy, FloatingPolicyConfig::Toggle);
    assert_eq!(lane.normal_hispeed_level, 18);
    assert_eq!(lane.classic_hispeed_step, 0.25);
    assert_eq!(lane.floating_hispeed_step, 0.50);
    assert!(lane.lift_enabled);
    assert!(lane.hispeed_auto_adjust);

    let serialized = toml::to_string(&lane).unwrap();
    assert!(serialized.contains("classic_hispeed = 2.0"));
    assert!(serialized.contains("base_hispeed = \"Classic\""));
    assert!(serialized.contains("floating_policy = \"Toggle\""));
    assert!(serialized.contains("normal_hispeed_level = 18"));
    assert!(serialized.contains("classic_hispeed_step = 0.25"));
    assert!(serialized.contains("floating_hispeed_step = 0.5"));
    assert!(serialized.contains("floating_target_green = 300"));
    assert!(serialized.contains("lift_enabled = true"));
    assert!(serialized.contains("hispeed_auto_adjust = true"));
}

#[test]
fn lane_view_preserves_explicit_hispeed_auto_adjust_off() {
    let lane: LaneViewConfig = toml::from_str(
        r#"
            hispeed = 2.0
            sudden = 0
            lift = 0
            hidden = 0
            target_green_number = 300
            hispeed_auto_adjust = false
            "#,
    )
    .unwrap();

    assert!(!lane.hispeed_auto_adjust);
}

#[test]
fn removed_grade_diff_display_is_ignored_and_not_serialized() {
    for value in ["Next", "Nearest"] {
        let toml = format!(
            r#"
                gauge = "Normal"
                random = "Off"
                target = "None"
                grade_diff_display = "{value}"
                lane_effect = "Off"
                assist = "None"
                auto_play = false
                "#
        );
        let play: PlayDefaultsConfig = toml::from_str(&toml).unwrap();
        assert!(!toml::to_string(&play).unwrap().contains("grade_diff_display"));
    }
}

#[test]
fn target_option_uses_beatoraja_ids_with_legacy_aliases() {
    fn parse_target(value: &str) -> TargetOptionConfig {
        let toml = format!(
            r#"
                gauge = "Normal"
                random = "Off"
                target = "{value}"
                lane_effect = "Off"
                assist = "None"
                auto_play = false
                "#
        );
        toml::from_str::<PlayDefaultsConfig>(&toml).unwrap().target
    }

    assert_eq!(parse_target("RANK_AAA"), TargetOptionConfig::RankAaa);
    assert_eq!(parse_target("AAA"), TargetOptionConfig::RankAaa);
    assert_eq!(parse_target("RIVAL_TOP"), TargetOptionConfig::RivalTop);
    assert_eq!(parse_target("Rival"), TargetOptionConfig::RivalTop);
    assert_eq!(parse_target("RIVAL_3"), TargetOptionConfig::RivalIndex(3));

    let mut play = PlayDefaultsConfig {
        target: TargetOptionConfig::RivalIndex(2),
        ..ProfileConfig::new_default("default", "Default", 0).play
    };
    let serialized = toml::to_string(&play).unwrap();
    assert!(serialized.contains(r#"target = "RIVAL_2""#));

    play.target = TargetOptionConfig::RankAaMinus;
    let serialized = toml::to_string(&play).unwrap();
    assert!(serialized.contains(r#"target = "RANK_AA-""#));
}

#[test]
fn select_state_uses_defaults_for_old_profiles() {
    // `[select]` セクションが無い旧 profile.toml でも既定値になる。
    let select: SelectStateConfig = toml::from_str("").unwrap();

    assert_eq!(select.mode_filter, "ALL");
    assert_eq!(select.difficulty_filter, "ALL");
    assert_eq!(select.sort, "TITLE");
    assert_eq!(select.difficulty_table_level_display, DifficultyTableLevelDisplay::Table);
    assert!(!select.random_select);
    assert_eq!(select.random_mix, RandomMixConfig::default());
}

#[test]
fn select_state_roundtrips_through_toml() {
    let select = SelectStateConfig {
        mode_filter: "7K".to_string(),
        difficulty_filter: "HYPER".to_string(),
        sort: "LEVEL".to_string(),
        difficulty_table_level_display: DifficultyTableLevelDisplay::Chart,
        random_select: true,
        random_select_no_play: true,
        random_select_failed: true,
        random_select_not_easy: true,
        random_select_not_clear: true,
        random_select_not_hard: true,
        random_select_not_ex_hard: true,
        random_select_not_full_combo: true,
        random_mix: RandomMixConfig {
            target_level: 12,
            max_level: 15,
            min_level: 8,
            bpm_range: 20,
            max_bpm: 220,
            min_bpm: 100,
            stages: 4,
        },
    };

    let toml = toml::to_string(&select).unwrap();
    let parsed: SelectStateConfig = toml::from_str(&toml).unwrap();

    assert_eq!(parsed.mode_filter, "7K");
    assert_eq!(parsed.difficulty_filter, "HYPER");
    assert_eq!(parsed.sort, "LEVEL");
    assert_eq!(parsed.difficulty_table_level_display, DifficultyTableLevelDisplay::Chart);
    assert!(parsed.random_select);
    assert_eq!(parsed.random_select_flags(), [true; 8]);
    assert_eq!(parsed.random_mix.target_level, 12);
    assert_eq!(parsed.random_mix.stages, 4);
}

#[test]
fn skin_config_separates_result_and_course_result_slots() {
    let skin: SkinConfig = toml::from_str(
        r#"
            result = "data/skins/result/result.luaskin"

            [result_options]
            Layout = "A"

            [result_files]
            Background = "normal.png"
            "#,
    )
    .unwrap();

    assert_eq!(skin.result, "data/skins/result/result.luaskin");
    assert!(skin.course_result.is_empty());
    assert_eq!(skin.result_options.get("Layout").map(String::as_str), Some("A"));
    assert!(skin.course_result_options.is_empty());

    let mut skin = skin;
    skin.course_result = "data/skins/result/course_result.luaskin".to_string();
    skin.course_result_options.insert("Layout".to_string(), "Course".to_string());
    skin.course_result_files.insert("Background".to_string(), "course.png".to_string());
    let toml = toml::to_string(&skin).unwrap();

    assert!(toml.contains("course_result = \"data/skins/result/course_result.luaskin\""));
    assert!(toml.contains("[course_result_options]"));
    assert!(toml.contains("[course_result_files]"));
}

#[test]
fn skin_config_migrates_legacy_offsets_to_each_slot() {
    let mut skin: SkinConfig = toml::from_str(
        r#"
            [[offsets]]
            id = 30
            h = 12

            [[play7_offsets]]
            id = 30
            h = 7
            "#,
    )
    .unwrap();

    skin.migrate_legacy_offsets();

    assert_eq!(skin.select_offsets[0].h, 12);
    assert_eq!(skin.play4_offsets[0].h, 12);
    assert_eq!(skin.play7_offsets[0].h, 7);
    assert_eq!(skin.course_result_offsets[0].h, 12);

    let serialized = toml::to_string(&skin).unwrap();
    assert!(!serialized.contains("[[offsets]]"));
    assert!(serialized.contains("[[select_offsets]]"));
    assert!(serialized.contains("[[play7_offsets]]"));
}

#[test]
fn skin_offset_config_round_trips_optional_name_and_reads_legacy_id_only_entries() {
    let legacy: SkinOffsetConfig = toml::from_str(
        r#"
            id = 30
            h = 12
            "#,
    )
    .unwrap();
    assert_eq!(legacy.name, None);
    assert!(!toml::to_string(&legacy).unwrap().contains("name"));

    let named = SkinOffsetConfig {
        name: Some("Notes offset".to_string()),
        id: 30,
        h: -24,
        ..Default::default()
    };
    let serialized = toml::to_string(&named).unwrap();
    let restored: SkinOffsetConfig = toml::from_str(&serialized).unwrap();

    assert!(serialized.contains("name = \"Notes offset\""));
    assert_eq!(restored, named);
}

#[test]
fn default_profile_stores_select_start_in_bindings() {
    let profile = ProfileConfig::new_default("default", "Default", 1);

    assert_eq!(profile.play.ln_mode_policy, LnPolicySetting::AutoLn);
    assert!(profile.lane.hispeed_auto_adjust);
    assert!(profile.input.start_key.is_none());
    assert!(profile.input.ui.bindings.iter().any(|entry| {
        entry.device == "keyboard"
            && entry.control == "Q"
            && entry.action == Some(InputActionConfig::E1)
    }));
}

#[test]
fn default_profile_uses_normalized_quieter_audio_and_prefetches_ir_rankings() {
    let profile = ProfileConfig::new_default("default", "Default", 1);

    assert_eq!(profile.audio_mix.master_volume, 50);
    assert!(profile.audio_mix.normalize_chart_volume);
    assert!(profile.audio_mix.normalize_system_bgm_volume);
    assert_eq!(profile.audio_mix.key_volume, 50);
    assert_eq!(profile.audio_mix.bgm_volume, 50);
    assert_eq!(profile.audio_mix.preview_volume, 50);
    assert_eq!(profile.audio_mix.system_bgm_volume, 50);
    assert_eq!(profile.audio_mix.system_se_volume, 50);
    assert!(profile.ir.prefetch_global_ranking_on_score_submit);
    assert!(profile.ir.prefetch_rival_ranking_on_score_submit);
    assert_eq!(profile.ir.providers[0], IrProviderConfig::bmz_ir());
    assert_eq!(profile.ir.providers[1], IrProviderConfig::rian_ir());
    assert_eq!(profile.ir.providers[2], IrProviderConfig::bms_ir());
    assert!(!profile.ir.providers[2].enabled);
}

#[test]
fn existing_audio_mix_defaults_system_bgm_normalization_on() {
    let audio_mix: AudioMixConfig = toml::from_str(
        r#"
        master_volume = 50
        key_volume = 50
        bgm_volume = 50
        preview_volume = 50
        system_bgm_volume = 50
        system_se_volume = 50
        "#,
    )
    .unwrap();

    assert!(audio_mix.normalize_system_bgm_volume);
}

#[test]
fn ui_language_keeps_string_storage_with_canonical_locale_code() {
    let ui: UiConfig = toml::from_str(
        r#"
            language = "ZH_hant_hk"
            theme = "default"
            show_fps = false
            confirm_on_exit = false
            "#,
    )
    .unwrap();

    assert_eq!(ui.language, "zh-HK");
    assert_eq!(ui.locale(), AppLocale::ZhHk);
    assert!(toml::to_string(&ui).unwrap().contains("language = \"zh-HK\""));
}

#[test]
fn ui_language_defaults_to_os_and_preserves_unsupported_value_fallback() {
    let missing: UiConfig = toml::from_str(
        r#"
            theme = "default"
            show_fps = false
            confirm_on_exit = false
            "#,
    )
    .unwrap();
    assert_eq!(missing.language, AppLocale::system_default().code());
    assert_eq!(ProfileConfig::new_default("test", "Test", 0).ui.language, missing.language);

    let unsupported: UiConfig = toml::from_str(
        r#"
            language = "fr"
            theme = "default"
            show_fps = false
            confirm_on_exit = false
            "#,
    )
    .unwrap();
    assert_eq!(unsupported.language, "ja");
}

#[test]
fn ui_language_setter_uses_profile_compatible_string() {
    let mut profile = ProfileConfig::new_default("default", "Default", 1);
    profile.ui.set_locale(AppLocale::Ko);

    assert_eq!(profile.ui.language, "ko");
    assert_eq!(profile.ui.locale(), AppLocale::Ko);
}

#[test]
fn ir_provider_defaults_to_always_send_policy() {
    let ir: IrConfig = toml::from_str(
        r#"
            primary_provider = "bmz-official"

            [[providers]]
            provider = "bmz-official"
            enabled = true
            "#,
    )
    .unwrap();

    assert_eq!(IrSendPolicyConfig::default(), IrSendPolicyConfig::Always);
    assert_eq!(ir.providers[0].send_policy, IrSendPolicyConfig::Always);
}

#[test]
fn ir_provider_normalization_moves_builtins_first_and_preserves_accounts_and_custom_entries() {
    let mut bmz = IrProviderConfig::bmz_ir();
    bmz.provider = "bmz-official".to_string();
    bmz.base_url = "https://bmz-player.hyrorre.workers.dev".to_string();
    bmz.provider_key = "bmz-account".to_string();
    bmz.account_id = "alice".to_string();
    bmz.enabled = true;
    bmz.last_login_at = Some(10);

    let mut rian = IrProviderConfig::rian_ir();
    rian.provider = "rianIR".to_string();
    rian.base_url = "https://rianir.link/api/".to_string();
    rian.provider_key = "rian-account".to_string();
    rian.account_id = "bob".to_string();
    rian.last_success_at = Some(20);

    let mut bms_ir = IrProviderConfig::bms_ir();
    bms_ir.provider = "BMSIR".to_string();
    bms_ir.base_url = "https://www.bms-ir.org/ignored/path".to_string();
    bms_ir.provider_key = "legacy-bms-key".to_string();
    bms_ir.account_id = "1234".to_string();
    bms_ir.account_display_name = "1234".to_string();
    bms_ir.enabled = true;
    bms_ir.role = IrProviderRoleConfig::Primary;

    let mut custom = IrProviderConfig::custom();
    custom.base_url = "http://localhost:3000/".to_string();
    custom.send_policy = IrSendPolicyConfig::CompleteSong;

    let duplicate = IrProviderConfig::bmz_ir();
    let mut ir = IrConfig {
        providers: vec![custom.clone(), duplicate.clone(), bms_ir, rian, bmz],
        ..IrConfig::default()
    };

    assert!(ir.normalize_builtin_providers());
    assert_eq!(ir.providers.len(), 5);
    assert_eq!(ir.providers[0].provider, "bmz");
    assert_eq!(ir.providers[0].base_url, "https://bmz-player.hyrorre.workers.dev/");
    assert_eq!(ir.providers[0].provider_key, "bmz-account");
    assert_eq!(ir.providers[0].account_id, "alice");
    assert_eq!(ir.providers[0].last_login_at, Some(10));
    assert_eq!(ir.providers[1].provider, "rian-ir");
    assert_eq!(ir.providers[1].base_url, "https://rianir.link/");
    assert_eq!(ir.providers[1].provider_key, "rian-account");
    assert_eq!(ir.providers[1].account_id, "bob");
    assert_eq!(ir.providers[1].last_success_at, Some(20));
    assert_eq!(ir.providers[2].provider, "bms-ir");
    assert_eq!(ir.providers[2].base_url, "https://www.bms-ir.org");
    assert_eq!(ir.providers[2].provider_key, "legacy-bms-key");
    assert_eq!(ir.providers[2].account_id, "1234");
    assert_eq!(ir.providers[2].account_display_name, "1234");
    assert!(ir.providers[2].enabled);
    assert_eq!(ir.providers[2].role, IrProviderRoleConfig::Primary);
    assert_eq!(ir.providers[3], custom);
    assert_eq!(ir.providers[4], duplicate);
    assert!(!ir.normalize_builtin_providers());
}

#[test]
fn ir_provider_normalization_adds_missing_builtins_before_custom_entries() {
    let mut custom = IrProviderConfig::custom();
    custom.provider = "rian-ir".to_string();
    custom.base_url = "https://custom.example/api/".to_string();
    let mut ir = IrConfig { providers: vec![custom.clone()], ..IrConfig::default() };

    assert!(ir.normalize_builtin_providers());
    assert_eq!(
        ir.providers,
        vec![
            IrProviderConfig::bmz_ir(),
            IrProviderConfig::rian_ir(),
            IrProviderConfig::bms_ir(),
            custom,
        ]
    );
}

#[test]
fn judge_config_serializes_visual_offset_auto_adjust_key() {
    let mut profile = ProfileConfig::new_default("default", "Default", 1);
    profile.judge.visual_offset_auto_adjust = true;

    let toml = toml::to_string(&profile).unwrap();

    assert!(toml.contains("visual_offset_auto_adjust = true"));
    assert!(!toml.contains("input_offset_auto_adjust"));
}

#[test]
fn replay_slot_rule_image_index_matches_beatoraja_autosave_rows() {
    use super::ReplaySlotRule;

    assert_eq!(ReplaySlotRule::Disabled.image_index(), 0);
    assert_eq!(ReplaySlotRule::ScoreUpdate.image_index(), 1);
    assert_eq!(ReplaySlotRule::BpUpdate.image_index(), 3);
    assert_eq!(ReplaySlotRule::MaxComboUpdate.image_index(), 5);
    assert_eq!(ReplaySlotRule::ClearUpdate.image_index(), 7);
    assert_eq!(ReplaySlotRule::Always.image_index(), 10);
    assert_eq!(replay_slot_rule_indices(&default_slot_rules()), [10, 1, 3, 0]);
}

#[test]
fn replay_slot_rule_empty_string_disables_slot() {
    let profile: ProfileConfig = toml::from_str(
        r#"
            version = 1
            id = "default"
            display_name = "Default"
            player_name = "NONAME"
            created_at = 1
            updated_at = 1

            [play]
            gauge = "Normal"
            random = "Off"
            lane_effect = "Off"
            assist = "None"
            auto_play = false

            [judge]
            input_offset_us = 0
            visual_offset_us = 0
            judge_algorithm = "Combo"
            fast_slow_display_threshold_ms = 0
            fast_slow_display_scope = "Auto"

            [lane]
            hispeed = 2.0
            hispeed_mode = "Normal"
            sudden = 0
            lift = 0
            hidden = 0
            target_green_number = 300

            [input]
            scratch_mode = "Normal"
            analog_scratch_sensitivity = 1.0
            analog_scratch_timeout_ms = 500

            [rival]
            active_rival = ""
            entries = []

            [replay]
            auto_save = true
            compress = false
            slot_rules = ["Always", "ScoreUpdate", "BpUpdate", ""]

            [ir]
            primary_provider = ""
            providers = []

            [ui]
            language = "ja"
            theme = "default"
            show_fps = false
            confirm_on_exit = false

            [audio_mix]
            normalize_chart_volume = true
            normalize_system_bgm_volume = true
            master_volume = 50
            key_volume = 50
            bgm_volume = 50
            preview_volume = 50
            system_bgm_volume = 50
            system_se_volume = 50

            [system_sound]
            bgm_dir = "data/bgm"
            se_dir = "data/se"
            default_sound_dir = "data/defaultsound"
            "#,
    )
    .unwrap();

    assert_eq!(profile.replay.slot_rules[3], ReplaySlotRule::Disabled);
    assert!(profile.ir.prefetch_global_ranking_on_score_submit);
    assert!(profile.ir.prefetch_rival_ranking_on_score_submit);
    assert!(profile.audio_mix.normalize_system_bgm_volume);
}

#[test]
fn default_gamepad_ui_bindings_use_thumb_buttons_without_dpad_enter_back() {
    let bindings = default_ui_bindings();

    assert!(bindings.iter().any(|entry| {
        entry.device == "gamepad"
            && entry.control == "Button9"
            && entry.action == Some(InputActionConfig::E1)
    }));
    assert!(bindings.iter().any(|entry| {
        entry.device == "gamepad"
            && entry.control == "Button12"
            && entry.action == Some(InputActionConfig::E4)
    }));
    assert!(!bindings.iter().any(|entry| {
        entry.device == "gamepad"
            && matches!(entry.control.as_str(), "DPadLeft" | "DPadRight")
            && matches!(entry.action, Some(InputActionConfig::E2 | InputActionConfig::SelectEnter))
    }));
    assert!(!bindings.iter().any(|entry| {
        matches!(
            entry.action,
            Some(InputActionConfig::SelectEnter | InputActionConfig::SelectOptionBga)
        )
    }));
    for (control, action) in [
        ("F3", InputActionConfig::SelectOpenFolder),
        ("F5", InputActionConfig::SelectReload),
        ("F10", InputActionConfig::SelectAutoplayFolder),
        ("F11", InputActionConfig::SelectOpenIr),
        ("0", InputActionConfig::SelectDifficultyFilter),
        ("1", InputActionConfig::SelectModeFilter),
        ("2", InputActionConfig::SelectSort),
        ("3", InputActionConfig::SelectLnMode),
        ("4", InputActionConfig::SelectReplayCycle),
        ("5", InputActionConfig::SelectReplayPlay),
        ("6", InputActionConfig::SelectOpenKeyConfig),
        ("F12", InputActionConfig::Screenshot),
        ("7", InputActionConfig::SelectRivalCycle),
        ("8", InputActionConfig::SelectSameFolder),
        ("9", InputActionConfig::SelectOpenDocuments),
    ] {
        assert!(bindings.iter().any(|entry| {
            entry.device == "keyboard" && entry.control == control && entry.action == Some(action)
        }));
    }
    for control in [
        "Numpad0", "Numpad1", "Numpad2", "Numpad3", "Numpad4", "Numpad5", "Numpad6", "Numpad7",
        "Numpad8", "Numpad9",
    ] {
        assert!(
            !bindings.iter().any(|entry| entry.device == "keyboard" && entry.control == control)
        );
    }
}

#[test]
fn v3_profile_migrates_play_shortcuts_once() {
    let mut input = crate::config::play_input::default_profile_input();
    input.ui.version = 3;
    input.ui.bindings.retain(|entry| {
        !entry.action.is_some_and(|action| PLAY_KEYBOARD_SHORTCUT_ACTIONS.contains(&action))
    });
    crate::config::play_input::normalize_profile_input(&mut input);
    for &action in PLAY_KEYBOARD_SHORTCUT_ACTIONS {
        assert_eq!(
            input.ui.bindings.iter().filter(|entry| entry.action == Some(action)).count(),
            1
        );
    }
    input.ui.bindings.retain(|entry| {
        !entry.action.is_some_and(|action| PLAY_KEYBOARD_SHORTCUT_ACTIONS.contains(&action))
    });
    crate::config::play_input::normalize_profile_input(&mut input);
    assert!(!input.ui.bindings.iter().any(|entry| {
        entry.action.is_some_and(|action| PLAY_KEYBOARD_SHORTCUT_ACTIONS.contains(&action))
    }));
}

#[test]
fn input_normalization_migrates_shortcuts_once_and_preserves_later_clears() {
    let mut input = crate::config::play_input::default_profile_input();
    input.ui.version = 0;
    input.ui.bindings.retain(|entry| {
        !entry.action.is_some_and(|action| CONFIGURABLE_SHORTCUT_ACTIONS.contains(&action))
    });

    crate::config::play_input::normalize_profile_input(&mut input);

    assert_eq!(input.ui.version, UI_INPUT_BINDING_VERSION);
    for &action in CONFIGURABLE_SHORTCUT_ACTIONS {
        assert!(input.ui.bindings.iter().any(|entry| entry.action == Some(action)));
    }

    input.ui.bindings.retain(|entry| entry.action != Some(InputActionConfig::Screenshot));
    crate::config::play_input::normalize_profile_input(&mut input);
    assert!(
        !input
            .ui
            .bindings
            .iter()
            .any(|entry| { entry.action == Some(InputActionConfig::Screenshot) })
    );
}

#[test]
fn input_normalization_adds_later_shortcuts_without_restoring_v1_clears() {
    let mut input = crate::config::play_input::default_profile_input();
    input.ui.version = 1;
    input.ui.bindings.retain(|entry| {
        !matches!(
            entry.action,
            Some(InputActionConfig::SelectOpenKeyConfig | InputActionConfig::Screenshot)
        )
    });

    crate::config::play_input::normalize_profile_input(&mut input);

    assert_eq!(input.ui.version, UI_INPUT_BINDING_VERSION);
    assert!(input.ui.bindings.iter().any(|entry| {
        entry.device == "keyboard"
            && entry.control == "6"
            && entry.action == Some(InputActionConfig::SelectOpenKeyConfig)
    }));
    for action in [
        InputActionConfig::SelectModeFilter,
        InputActionConfig::SelectSort,
        InputActionConfig::SelectLnMode,
        InputActionConfig::SelectReplayCycle,
        InputActionConfig::SelectSameFolder,
    ] {
        assert!(input.ui.bindings.iter().any(|entry| entry.action == Some(action)));
    }
    assert!(
        !input.ui.bindings.iter().any(|entry| entry.action == Some(InputActionConfig::Screenshot))
    );
}

#[test]
fn input_normalization_removes_duplicate_keypad_defaults() {
    let mut input = crate::config::play_input::default_profile_input();
    input.ui.version = 2;
    input.ui.bindings.retain(|entry| {
        !matches!(
            entry.action,
            Some(
                InputActionConfig::SelectModeFilter
                    | InputActionConfig::SelectSort
                    | InputActionConfig::SelectLnMode
            )
        ) && !matches!(
            (entry.control.as_str(), entry.action),
            ("0", Some(InputActionConfig::SelectDifficultyFilter))
                | ("4", Some(InputActionConfig::SelectReplayCycle))
                | ("5", Some(InputActionConfig::SelectReplayPlay))
                | ("8", Some(InputActionConfig::SelectSameFolder))
                | ("9", Some(InputActionConfig::SelectOpenDocuments))
        )
    });
    // Recreate legacy profiles that stored keypad shortcuts as defaults before
    // the top-row bindings became canonical.
    for (control, action) in [
        ("Numpad0", InputActionConfig::SelectDifficultyFilter),
        ("Numpad4", InputActionConfig::SelectReplayCycle),
        ("Numpad5", InputActionConfig::SelectReplayPlay),
        ("Numpad7", InputActionConfig::SelectRivalCycle),
        ("Numpad8", InputActionConfig::SelectSameFolder),
        ("Numpad9", InputActionConfig::SelectOpenDocuments),
    ] {
        input.ui.bindings.push(BindingConfigEntry {
            device: "keyboard".to_string(),
            control: control.to_string(),
            keyboard_slot: None,
            lane: None,
            action: Some(action),
            scratch: None,
        });
    }

    crate::config::play_input::normalize_profile_input(&mut input);

    for (control, action) in [
        ("0", InputActionConfig::SelectDifficultyFilter),
        ("1", InputActionConfig::SelectModeFilter),
        ("2", InputActionConfig::SelectSort),
        ("3", InputActionConfig::SelectLnMode),
        ("4", InputActionConfig::SelectReplayCycle),
        ("5", InputActionConfig::SelectReplayPlay),
        ("8", InputActionConfig::SelectSameFolder),
        ("9", InputActionConfig::SelectOpenDocuments),
    ] {
        assert!(
            input.ui.bindings.iter().any(|entry| {
                entry.device == "keyboard"
                    && entry.control == control
                    && entry.action == Some(action)
            }),
            "missing {control} for {action:?}"
        );
    }
    for (control, action) in [
        ("Numpad0", InputActionConfig::SelectDifficultyFilter),
        ("Numpad4", InputActionConfig::SelectReplayCycle),
        ("Numpad5", InputActionConfig::SelectReplayPlay),
        ("Numpad7", InputActionConfig::SelectRivalCycle),
        ("Numpad8", InputActionConfig::SelectSameFolder),
        ("Numpad9", InputActionConfig::SelectOpenDocuments),
    ] {
        assert!(!input.ui.bindings.iter().any(|entry| {
            entry.device == "keyboard" && entry.control == control && entry.action == Some(action)
        }));
    }
}

#[test]
fn input_normalization_preserves_custom_keypad_aliases() {
    let mut input = crate::config::play_input::default_profile_input();
    input.ui.version = 3;
    input.ui.bindings.push(BindingConfigEntry {
        device: "keyboard".to_string(),
        control: "Numpad4".to_string(),
        keyboard_slot: None,
        lane: None,
        action: Some(InputActionConfig::SelectReplayCycle),
        scratch: None,
    });
    input.ui.bindings.push(BindingConfigEntry {
        device: "keyboard".to_string(),
        control: "R".to_string(),
        keyboard_slot: None,
        lane: None,
        action: Some(InputActionConfig::SelectReplayCycle),
        scratch: None,
    });
    input.ui.bindings.push(BindingConfigEntry {
        device: "keyboard".to_string(),
        control: "Numpad8".to_string(),
        keyboard_slot: None,
        lane: None,
        action: Some(InputActionConfig::SelectSameFolder),
        scratch: None,
    });

    crate::config::play_input::normalize_profile_input(&mut input);

    assert!(input.ui.bindings.iter().any(|entry| {
        entry.device == "keyboard"
            && entry.control == "Numpad4"
            && entry.action == Some(InputActionConfig::SelectReplayCycle)
    }));
    assert!(input.ui.bindings.iter().any(|entry| {
        entry.device == "keyboard"
            && entry.control == "R"
            && entry.action == Some(InputActionConfig::SelectReplayCycle)
    }));
    assert!(!input.ui.bindings.iter().any(|entry| {
        entry.device == "keyboard"
            && entry.control == "Numpad8"
            && entry.action == Some(InputActionConfig::SelectSameFolder)
    }));
}

#[test]
fn input_normalization_preserves_v2_custom_digit_shortcuts() {
    let mut input = crate::config::play_input::default_profile_input();
    input.ui.version = 2;
    for (action, control) in [
        (InputActionConfig::SelectReplayCycle, "A"),
        (InputActionConfig::SelectSameFolder, "B"),
        (InputActionConfig::SelectOpenDocuments, "G"),
    ] {
        input.ui.bindings.retain(|entry| entry.action != Some(action));
        input.ui.bindings.push(BindingConfigEntry {
            device: "keyboard".to_string(),
            control: control.to_string(),
            keyboard_slot: None,
            lane: None,
            action: Some(action),
            scratch: None,
        });
    }

    crate::config::play_input::normalize_profile_input(&mut input);

    for (action, control) in [
        (InputActionConfig::SelectReplayCycle, "A"),
        (InputActionConfig::SelectSameFolder, "B"),
        (InputActionConfig::SelectOpenDocuments, "G"),
    ] {
        let controls = input
            .ui
            .bindings
            .iter()
            .filter(|entry| entry.action == Some(action))
            .map(|entry| entry.control.as_str())
            .collect::<Vec<_>>();
        assert_eq!(controls, vec![control]);
    }
}

#[test]
fn input_config_reads_legacy_start_key_and_migrates_common_analog_settings() {
    let mut input: ProfileInputConfig = toml::from_str(
        r#"
            scratch_mode = "Normal"
            start_key = "E"
            analog_scratch_sensitivity = 1.0
            analog_scratch_threshold = 321
            analog_scratch_timeout_ms = 500

            [[bindings]]
            device = "keyboard"
            control = "Z"
            lane = "Key1"
            "#,
    )
    .unwrap();
    assert_eq!(input.legacy_bindings[0].lane, Some(LaneConfig::Key1));
    crate::config::play_input::normalize_profile_input(&mut input);

    assert_eq!(input.start_key.as_deref(), Some("E"));
    assert_eq!(input.scratch_mode, ScratchInputMode::Normal);
    assert!(input.legacy_bindings.is_empty());
    assert_eq!(input.analog_scratch_timeout_ms, 500);
    assert_eq!(input.gamepad1.analog_scratch_sensitivity, 1.0);
    assert_eq!(input.gamepad2.analog_scratch_sensitivity, 1.0);
    assert_eq!(input.gamepad1.analog_scratch_threshold, 321);
    assert_eq!(input.gamepad2.analog_scratch_threshold, 321);
    assert_eq!(input.keyboard_release_bounce_ms, 0);
    assert_eq!(input.controller_release_bounce_ms, 0);
}

#[test]
fn input_config_serializes_select_actions_without_start_key() {
    let profile = ProfileConfig::new_default("default", "Default", 1);

    let toml = toml::to_string(&profile.input).unwrap();

    assert!(!toml.contains("start_key"));
    assert!(!toml.contains("scratch_mode"));
    assert!(!toml.contains("analog_scratch_timeout_ms"));
    assert!(toml.contains("[gamepad1]"));
    assert!(toml.contains("[gamepad2]"));
    assert_eq!(toml.matches("analog_scratch = true").count(), 2);
    assert_eq!(toml.matches("analog_scratch_threshold = 100").count(), 2);
    assert!(toml.contains("keyboard_release_bounce_ms = 0"));
    assert!(toml.contains("controller_release_bounce_ms = 0"));
    assert!(toml.contains("action = \"E1\""));
    assert!(toml.contains("action = \"E2\""));
    assert!(toml.contains("action = \"E3\""));
    assert!(toml.contains("action = \"E4\""));
    assert!(toml.contains("control = \"Axis1+\"\nlane = \"Scratch\"\nscratch = \"up\""));
    assert!(toml.contains("control = \"Axis1-\"\nlane = \"Scratch\"\nscratch = \"down\""));
}

#[test]
fn input_config_roundtrips_per_player_analog_scratch_settings() {
    let mut profile = ProfileConfig::new_default("default", "Default", 1);
    profile.input.gamepad1.analog_scratch = false;
    profile.input.gamepad1.analog_scratch_sensitivity = 1.7;
    profile.input.gamepad2.analog_scratch_threshold = 432;

    let toml = toml::to_string(&profile.input).unwrap();
    let decoded: ProfileInputConfig = toml::from_str(&toml).unwrap();

    assert!(!decoded.gamepad1.analog_scratch);
    assert_eq!(decoded.gamepad1.analog_scratch_sensitivity, 1.7);
    assert!(decoded.gamepad2.analog_scratch);
    assert_eq!(decoded.gamepad2.analog_scratch_threshold, 432);
    assert!(!toml.contains("legacy_analog_scratch"));
}

#[test]
fn keyboard_binding_slot_is_optional_and_roundtrips_through_toml() {
    let legacy: BindingConfigEntry = toml::from_str(
        r#"
            device = "keyboard"
            control = "Z"
            lane = "Key1"
            "#,
    )
    .unwrap();
    assert_eq!(legacy.keyboard_slot, None);

    let tagged =
        BindingConfigEntry { keyboard_slot: Some(KeyboardBindingSlotConfig::Secondary), ..legacy };
    let serialized = toml::to_string(&tagged).unwrap();
    let restored: BindingConfigEntry = toml::from_str(&serialized).unwrap();

    assert!(serialized.contains("keyboard_slot = \"secondary\""));
    assert_eq!(restored.keyboard_slot, Some(KeyboardBindingSlotConfig::Secondary));
}

#[test]
fn input_release_bounce_settings_roundtrip_through_toml() {
    let mut input = crate::config::play_input::default_profile_input();
    input.keyboard_release_bounce_ms = 3;
    input.controller_release_bounce_ms = 8;

    let toml = toml::to_string(&input).unwrap();
    let decoded: ProfileInputConfig = toml::from_str(&toml).unwrap();

    assert_eq!(decoded.keyboard_release_bounce_ms, 3);
    assert_eq!(decoded.controller_release_bounce_ms, 8);
}

#[test]
fn play_mode_input_hispeed_directions_roundtrip_through_toml() {
    let mut hispeed = BTreeMap::new();
    hispeed.insert(LaneConfig::Key1, HispeedDirectionConfig::Down);
    hispeed.insert(LaneConfig::Key6, HispeedDirectionConfig::Up);
    let config = PlayModeInputConfig {
        inherit: None,
        bindings: vec![BindingConfigEntry {
            device: "keyboard".to_string(),
            control: "Z".to_string(),
            keyboard_slot: None,
            lane: Some(LaneConfig::Key1),
            action: None,
            scratch: None,
        }],
        hispeed,
    };

    let toml = toml::to_string(&config).unwrap();
    let parsed: PlayModeInputConfig = toml::from_str(&toml).unwrap();

    assert!(toml.contains("[hispeed]"));
    assert!(toml.contains("Key1 = \"Down\""));
    assert!(toml.contains("Key6 = \"Up\""));
    assert_eq!(parsed.bindings.len(), 1);
    assert_eq!(parsed.hispeed.get(&LaneConfig::Key1), Some(&HispeedDirectionConfig::Down));
    assert_eq!(parsed.hispeed.get(&LaneConfig::Key6), Some(&HispeedDirectionConfig::Up));
}

#[test]
fn play_mode_input_omits_empty_hispeed_directions_and_reads_old_profiles() {
    let config: PlayModeInputConfig = toml::from_str("inherit = \"7k\"").unwrap();

    assert!(config.hispeed.is_empty());
    assert!(!toml::to_string(&config).unwrap().contains("hispeed"));
}

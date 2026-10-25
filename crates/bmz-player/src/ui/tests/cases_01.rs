use super::*;

#[test]
fn ir_login_forms_keep_provider_credentials_independent() {
    let mut state = IrLoginUiState::default();
    {
        let form = state.provider_form_mut(0);
        form.email = "alice@example.com".to_string();
        form.password = "alice-password".to_string();
    }
    {
        let form = state.provider_form_mut(1);
        form.email = "bob".to_string();
        form.password = "bob-password".to_string();
    }

    assert_eq!(state.provider_form_mut(0).email, "alice@example.com");
    assert_eq!(state.provider_form_mut(0).password, "alice-password");
    assert_eq!(state.provider_form_mut(1).email, "bob");
    assert_eq!(state.provider_form_mut(1).password, "bob-password");
}

#[test]
fn ir_login_forms_follow_provider_removal() {
    let mut state = IrLoginUiState::default();
    state.provider_form_mut(0).email = "first".to_string();
    state.provider_form_mut(1).email = "second".to_string();
    state.busy_form_index = Some(1);

    state.remove_provider_form(0);

    assert_eq!(state.provider_form_mut(0).email, "second");
    assert_eq!(state.busy_form_index, Some(0));
}

#[test]
fn ir_device_key_idle_state_does_not_mark_unconfigured_provider_busy() {
    let state = IrDeviceKeyUiState::default();

    assert!(!state.is_busy_for(0, None, "bmz", "https://example.com"));
}

#[test]
fn ir_device_key_busy_state_matches_only_its_provider_target() {
    let state = IrDeviceKeyUiState {
        busy_provider: Some("bmz-dev".to_string()),
        busy_provider_index: Some(1),
        busy_target: Some(IrProviderUiTarget::new(
            "bmz".to_string(),
            "https://example.com".to_string(),
        )),
        ..IrDeviceKeyUiState::default()
    };

    assert!(state.is_busy_for(1, Some("bmz-dev"), "bmz", "https://example.com"));
    assert!(!state.is_busy_for(0, Some("bmz-dev"), "bmz", "https://example.com"));
    assert!(!state.is_busy_for(1, None, "bmz", "https://example.com"));
    assert!(!state.is_busy_for(1, Some("bmz-dev"), "bmz", "https://other.example.com"));
}

#[test]
fn ir_login_channel_disconnect_clears_busy_state() {
    let (sender, receiver) = std::sync::mpsc::channel();
    drop(sender);
    let mut state = IrLoginUiState::default();
    state.busy = true;
    state.busy_form_index = Some(0);
    state.busy_target =
        Some(IrProviderUiTarget::new("bmz".to_string(), "https://example.com".to_string()));
    state.receiver = Some(receiver);
    let mut profile = ProfileConfig::new_default("default", "Default", 1);

    assert!(!state.poll(&mut profile, Localizer::new(AppLocale::En)));
    assert!(!state.busy);
    assert!(state.busy_form_index.is_none());
    assert!(state.busy_target.is_none());
    assert!(state.receiver.is_none());
    assert!(state.message.as_ref().is_some_and(|message| !message.ok));
}

#[test]
fn ir_device_key_channel_disconnect_clears_busy_state() {
    let (sender, receiver) = std::sync::mpsc::channel();
    drop(sender);
    let mut state = IrDeviceKeyUiState {
        busy_provider: Some("bmz-dev".to_string()),
        busy_provider_index: Some(0),
        busy_target: Some(IrProviderUiTarget::new(
            "bmz".to_string(),
            "https://example.com".to_string(),
        )),
        receiver: Some(receiver),
        ..IrDeviceKeyUiState::default()
    };

    state.poll(Localizer::new(AppLocale::En));

    assert!(state.busy_provider.is_none());
    assert!(state.busy_provider_index.is_none());
    assert!(state.busy_target.is_none());
    assert!(state.receiver.is_none());
    assert!(state.message.as_ref().is_some_and(|message| !message.ok));
}

#[test]
fn optional_timestamp_formats_datetime_to_minute_precision() {
    assert_eq!(format_optional_timestamp(None), "-");
    assert_eq!(format_datetime_minute(2026, 8, 4, 9, 7), "2026-08-04 09:07");

    let epoch = format_optional_timestamp(Some(0));
    assert_eq!(epoch.len(), 16);
    assert_eq!(&epoch[4..5], "-");
    assert_eq!(&epoch[7..8], "-");
    assert_eq!(&epoch[10..11], " ");
    assert_eq!(&epoch[13..14], ":");
}

#[test]
fn decide_and_play_restrict_settings_panels() {
    assert!(!scene_restricts_settings("Select"));
    assert!(scene_restricts_settings("Decide"));
    assert!(scene_restricts_settings("Play"));
    assert!(!scene_restricts_settings("Result"));
}

#[test]
fn hidden_egui_uses_idle_frame_on_every_scene_until_an_overlay_needs_full_state() {
    assert!(!egui_frame_needs_full_state(false, false, false, false, "Select"));
    assert!(!egui_frame_needs_full_state(false, false, false, false, "Decide"));
    assert!(!egui_frame_needs_full_state(false, false, false, false, "Play"));
    assert!(!egui_frame_needs_full_state(false, false, false, false, "Result"));
    assert!(egui_frame_needs_full_state(true, false, false, false, "Play"));
    assert!(egui_frame_needs_full_state(false, true, false, false, "Play"));
    assert!(egui_frame_needs_full_state(false, false, true, false, "Select"));
    assert!(egui_frame_needs_full_state(false, false, false, true, "Select"));
    assert!(egui_frame_needs_full_state(true, false, false, true, "Play"));
    assert!(!egui_frame_needs_full_state(false, false, false, true, "Play"));
}

#[test]
fn f1_menu_only_blocks_window_input_when_egui_consumes_it() {
    assert!(!egui_consumed_game_input(true, false, false, false, false, false));
    assert!(egui_consumed_game_input(true, false, false, false, false, true));
    assert!(!egui_consumed_game_input(false, false, false, false, false, true));
    assert!(egui_consumed_game_input(false, true, false, false, false, true));
}

/// 表示中の BMZ メニューに実際の pointer / key 入力を与え、egui-winit が consumed の判定に使う
/// egui の状態から、選曲画面へ渡る入力と止まる入力を確かめる。
#[test]
fn open_f1_menu_passes_unfocused_keys_and_keeps_pointer_input_over_the_menu() {
    let ctx = egui::Context::default();
    let text = Localizer::new(AppLocale::En);
    let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1280.0, 800.0));
    let frame = |events: Vec<egui::Event>| {
        let input = egui::RawInput { screen_rect: Some(screen), events, ..Default::default() };
        let _ = ctx.run_ui(input, |ui| {
            build_menu_window(ui.ctx(), &mut true, "Default", text, |ui| {
                ui.label("page body");
            });
        });
    };
    for _ in 0..3 {
        frame(Vec::new());
    }
    let window =
        ctx.memory(|memory| memory.area_rect(egui::Id::new("settings_workspace"))).unwrap();
    let inside = window.center();
    let outside = egui::pos2(screen.right() - 4.0, screen.bottom() - 4.0);
    assert!(!window.contains(outside), "probe point must be outside the menu: {window:?}");
    let menu_consumed =
        |consumed| egui_consumed_game_input(true, false, false, false, false, consumed);

    // メニュー上のホイール・クリックは egui が使うので選曲画面へ渡さない。
    frame(vec![egui::Event::PointerMoved(inside)]);
    assert!(menu_consumed(ctx.egui_wants_pointer_input()));
    // メニュー本文をクリックしてもキーボードフォーカスは取らず、矢印キー等は選曲画面へ渡る。
    let click = |pressed| egui::Event::PointerButton {
        pos: inside,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    frame(vec![click(true)]);
    frame(vec![click(false)]);
    frame(Vec::new());
    assert!(!menu_consumed(ctx.egui_wants_keyboard_input()));
    // メニューの外の pointer 入力も選曲画面へ渡る。
    frame(vec![egui::Event::PointerMoved(outside)]);
    assert!(!menu_consumed(ctx.egui_wants_pointer_input()));
}

#[test]
fn escape_closes_the_menu_only_after_text_input_and_popups_are_done() {
    let ctx = egui::Context::default();
    let text = Localizer::new(AppLocale::En);
    let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1280.0, 800.0));
    let combo_rect = std::cell::Cell::new(egui::Rect::NOTHING);
    let focus_text = std::cell::Cell::new(false);
    let frame = |events: Vec<egui::Event>| {
        let input = egui::RawInput { screen_rect: Some(screen), events, ..Default::default() };
        let _ = ctx.run_ui(input, |ui| {
            build_menu_window(ui.ctx(), &mut true, "Default", text, |ui| {
                let mut value = String::new();
                let edit = ui.text_edit_singleline(&mut value);
                if focus_text.replace(false) {
                    edit.request_focus();
                }
                let combo = egui::ComboBox::from_id_salt("escape_probe")
                    .selected_text("A")
                    .show_ui(ui, |ui| {
                        ui.label("B");
                    });
                combo_rect.set(combo.response.rect);
            });
        });
    };
    for _ in 0..3 {
        frame(Vec::new());
    }
    assert_eq!(
        menu_escape_action(true, ctx.egui_wants_keyboard_input(), ctx.any_popup_open()),
        MenuEscape::ClosedMenu
    );
    assert_eq!(menu_escape_action(false, false, false), MenuEscape::NotHandled);

    // 文字入力中の ESC は egui が消費して入力欄を抜けるだけにする。
    focus_text.set(true);
    frame(Vec::new());
    frame(Vec::new());
    assert!(ctx.egui_wants_keyboard_input());
    assert_eq!(
        menu_escape_action(true, ctx.egui_wants_keyboard_input(), ctx.any_popup_open()),
        MenuEscape::NotHandled
    );
    ctx.memory_mut(|memory| memory.stop_text_input());
    frame(Vec::new());

    // プルダウン表示中の ESC はプルダウンだけを閉じる。
    let pos = combo_rect.get().center();
    let click = |pressed| egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    frame(vec![egui::Event::PointerMoved(pos), click(true)]);
    frame(vec![click(false)]);
    frame(Vec::new());
    assert!(ctx.any_popup_open());
    // ComboBox はキーボードフォーカスを取らないため、egui-winit は ESC を未消費と
    // 報告する。それでもメニュー側で止め、選曲のフォルダ移動へ伝播させない。
    assert!(!ctx.egui_wants_keyboard_input());
    let action = menu_escape_action(true, ctx.egui_wants_keyboard_input(), ctx.any_popup_open());
    assert_eq!(action, MenuEscape::KeptForPopup);
    assert!(action.handled());
}

#[test]
fn modal_game_input_blocks_do_not_depend_on_f1_menu_visibility() {
    assert!(!blocks_game_input_for_modal_state(false, false, false));
    assert!(blocks_game_input_for_modal_state(true, false, false));
    assert!(blocks_game_input_for_modal_state(false, true, false));
    assert!(blocks_game_input_for_modal_state(false, false, true));
}

#[test]
fn difficulty_table_source_label_shows_fetched_table_name() {
    let tables = vec![DifficultyTableRecord {
        id: 1,
        source_url: "https://example.com/header.json".to_string(),
        name: "発狂BMS難易度表".to_string(),
        symbol: "★".to_string(),
        level_order: vec!["1".to_string()],
        fetched_at: 1_700_000_000,
    }];

    assert_eq!(
        difficulty_table_source_label("https://example.com/header.json", &tables),
        "発狂BMS難易度表"
    );
}

#[test]
fn difficulty_table_source_label_keeps_url_before_first_fetch() {
    assert_eq!(
        difficulty_table_source_label("https://example.com/header.json", &[]),
        "https://example.com/header.json"
    );
}

#[test]
fn song_folder_display_name_uses_only_the_last_component() {
    assert_eq!(song_folder_display_name("/library/bms"), "bms");
    assert_eq!(song_folder_display_name("/library/bms/"), "bms");
    assert_eq!(song_folder_display_name("/"), "/");
}

#[test]
fn debug_log_filter_keeps_selected_level_and_more_severe_entries() {
    assert!(!DebugLogFilter::Info.allows(TracingLogLevel::Debug));
    assert!(DebugLogFilter::Info.allows(TracingLogLevel::Info));
    assert!(DebugLogFilter::Info.allows(TracingLogLevel::Error));
    assert!(DebugLogFilter::All.allows(TracingLogLevel::Trace));
}

#[test]
fn debug_log_copy_text_includes_level_target_and_message() {
    let entry = LogEntry {
        level: TracingLogLevel::Warn,
        target: "bmz_player::test".to_string(),
        message: "slow frame".to_string(),
    };

    let text = Localizer::new(AppLocale::En);
    assert_eq!(format_log_entry(&entry, text), "[WARN] bmz_player::test slow frame");

    let empty = LogEntry { message: String::new(), ..entry };
    assert_eq!(format_log_entry(&empty, text), "[WARN] bmz_player::test (no message)");
}

#[test]
fn restricted_profile_settings_keep_only_realtime_categories() {
    let baseline = ProfileConfig::new_default("default", "Default", 1);
    let mut edited = baseline.clone();
    edited.display_name = "Changed".to_string();
    edited.play.rule_mode = RuleMode::Dx;
    edited.select.difficulty_table_level_display = DifficultyTableLevelDisplay::Chart;
    edited.audio_mix.master_volume = 23;
    edited.ui.show_fps = !baseline.ui.show_fps;
    edited.ui.set_locale(AppLocale::En);
    edited.system_sound.bgm_dir = "custom-bgm".to_string();
    edited.judge.input_offset_us = 4_000;
    edited.lane.hispeed = 3.25;
    edited.input.gamepad1.analog_scratch_threshold = 321;
    edited.input.gamepad2.analog_scratch = false;
    edited.input.keyboard_release_bounce_ms = 4;
    edited.input.controller_release_bounce_ms = 7;

    restore_restricted_profile_settings(&mut edited, baseline.clone());

    assert_eq!(edited.display_name, baseline.display_name);
    assert_eq!(edited.play.rule_mode, baseline.play.rule_mode);
    assert_eq!(
        edited.select.difficulty_table_level_display,
        baseline.select.difficulty_table_level_display
    );
    assert_eq!(edited.audio_mix.master_volume, 23);
    assert_eq!(edited.ui.show_fps, !baseline.ui.show_fps);
    assert_eq!(edited.ui.locale(), AppLocale::En);
    assert_eq!(edited.system_sound.bgm_dir, "custom-bgm");
    assert_eq!(edited.judge.input_offset_us, 4_000);
    assert_eq!(edited.lane.hispeed, 3.25);
    assert_eq!(edited.input.gamepad1.analog_scratch_threshold, 321);
    assert!(!edited.input.gamepad2.analog_scratch);
    assert_eq!(edited.input.keyboard_release_bounce_ms, 4);
    assert_eq!(edited.input.controller_release_bounce_ms, 7);
}

#[test]
fn sanitize_profile_id_input_keeps_portable_path_chars_only() {
    let mut value = "abc_日本語-_.012/\\: xyz".to_string();

    sanitize_profile_id_input(&mut value);

    assert_eq!(value, "abc_-_012xyz");
}

#[test]
fn sanitize_profile_id_input_truncates_to_profile_id_limit() {
    let mut value = "a".repeat(80);

    sanitize_profile_id_input(&mut value);

    assert_eq!(value.len(), 64);
}

#[test]
fn skin_candidate_display_hides_bundled_origin_label_when_requested() {
    let candidate = SkinCandidate {
        name: "Default".to_string(),
        path: "resource:skins/default/select.json".to_string(),
        origin: SkinCandidateOrigin::Bundled,
    };

    assert_eq!(
        skin_candidate_display(&candidate, true, Localizer::new(crate::i18n::AppLocale::Ja),),
        "[アプリ側] Default (resource:skins/default/select.json)"
    );
    assert_eq!(
        skin_candidate_display(&candidate, false, Localizer::new(crate::i18n::AppLocale::Ja),),
        "Default (resource:skins/default/select.json)"
    );
}

#[test]
fn skin_candidate_display_keeps_user_origin_label() {
    let candidate = SkinCandidate {
        name: "Custom".to_string(),
        path: "data:skins/custom/play7.luaskin".to_string(),
        origin: SkinCandidateOrigin::User,
    };

    assert_eq!(
        skin_candidate_display(&candidate, false, Localizer::new(crate::i18n::AppLocale::Ja),),
        "[ユーザー] Custom (data:skins/custom/play7.luaskin)"
    );
}

#[test]
fn bundled_skin_origin_is_hidden_for_development_or_portable_layout() {
    let app_paths = AppPaths::from_dirs(
        PathBuf::from("data"),
        PathBuf::from("data"),
        PathBuf::from("data/cache"),
        PathBuf::from("data/logs"),
    );
    let mut catalog = SkinCatalog::default();
    catalog.select.push(SkinCandidate {
        name: "Default".to_string(),
        path: "resource:skins/default/select.json".to_string(),
        origin: SkinCandidateOrigin::Bundled,
    });
    catalog.select.push(SkinCandidate {
        name: "Custom".to_string(),
        path: "data:skins/custom/select.luaskin".to_string(),
        origin: SkinCandidateOrigin::User,
    });

    assert!(!show_bundled_skin_origin(&app_paths, &catalog));
}

#[test]
fn bundled_skin_origin_is_shown_when_user_candidates_share_a_regular_layout() {
    let app_paths = AppPaths::from_dirs(
        PathBuf::from("resources"),
        PathBuf::from("profile-data"),
        PathBuf::from("profile-data/cache"),
        PathBuf::from("profile-data/logs"),
    );
    let mut catalog = SkinCatalog::default();
    catalog.select.push(SkinCandidate {
        name: "Default".to_string(),
        path: "resource:skins/default/select.json".to_string(),
        origin: SkinCandidateOrigin::Bundled,
    });
    catalog.select.push(SkinCandidate {
        name: "Custom".to_string(),
        path: "data:skins/custom/select.luaskin".to_string(),
        origin: SkinCandidateOrigin::User,
    });

    assert!(show_bundled_skin_origin(&app_paths, &catalog));
}

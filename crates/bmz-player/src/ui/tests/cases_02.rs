use super::*;

#[test]
fn bundled_skin_origin_is_hidden_when_catalog_has_no_user_candidates() {
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

    assert!(!show_bundled_skin_origin(&app_paths, &catalog));
}

#[test]
fn sync_ir_provider_roles_keeps_only_primary_role() {
    let mut ir_config = IrConfig {
        primary_provider: "bmz-dev".to_string(),
        providers: vec![
            IrProviderConfig {
                provider: "bmz".to_string(),
                provider_key: "bmz".to_string(),
                base_url: "https://bmz-player.hyrorre.workers.dev".to_string(),
                enabled: true,
                account_display_name: String::new(),
                account_id: String::new(),
                send_policy: IrSendPolicyConfig::default(),
                role: IrProviderRoleConfig::Primary,
                last_login_at: None,
                last_success_at: None,
            },
            IrProviderConfig {
                provider: "bmz".to_string(),
                provider_key: "bmz-dev".to_string(),
                base_url: "http://localhost:3000".to_string(),
                enabled: true,
                account_display_name: String::new(),
                account_id: String::new(),
                send_policy: IrSendPolicyConfig::default(),
                role: IrProviderRoleConfig::SubmitOnly,
                last_login_at: None,
                last_success_at: None,
            },
        ],
        ..IrConfig::default()
    };

    assert!(sync_ir_provider_roles(&mut ir_config));
    assert_eq!(ir_config.providers[0].role, IrProviderRoleConfig::SubmitOnly);
    assert_eq!(ir_config.providers[1].role, IrProviderRoleConfig::Primary);

    ir_config.primary_provider.clear();
    assert!(sync_ir_provider_roles(&mut ir_config));
    assert_eq!(ir_config.providers[0].role, IrProviderRoleConfig::SubmitOnly);
    assert_eq!(ir_config.providers[1].role, IrProviderRoleConfig::SubmitOnly);
}

#[test]
fn sync_ir_provider_roles_accepts_bms_ir_as_primary() {
    let mut bms_ir = IrProviderConfig::bms_ir();
    bms_ir.provider_key = crate::ir::bms_ir::BMS_IR_PROVIDER.to_string();
    bms_ir.enabled = true;
    bms_ir.role = IrProviderRoleConfig::Primary;
    let mut ir_config = IrConfig {
        primary_provider: crate::ir::bms_ir::BMS_IR_PROVIDER.to_string(),
        providers: vec![bms_ir],
        ..IrConfig::default()
    };

    assert!(!sync_ir_provider_roles(&mut ir_config));
    assert_eq!(ir_config.primary_provider, crate::ir::bms_ir::BMS_IR_PROVIDER);
    assert_eq!(ir_config.providers[0].role, IrProviderRoleConfig::Primary);
}

#[test]
fn clamp_panel_layout_fits_high_dpi_1920x1080_logical_viewport() {
    // 1920x1080 物理ウィンドウ @ 2x → egui 論理 960x540 相当。
    let constrain = egui::Rect::from_min_size(egui::pos2(16.0, 16.0), egui::vec2(928.0, 508.0));
    // egui 0.34 既定 style 付近の chrome 高さ (frame + title bar)。
    let chrome = egui::vec2(12.0, 58.0);
    let (default_inner, max_inner, pos) =
        clamp_panel_layout(constrain, chrome, 440.0, 560.0, egui::pos2(16.0, 480.0));

    let outer = default_inner + chrome;
    assert!(outer.x <= constrain.width() + 0.01);
    assert!(outer.y <= constrain.height() + 0.01);
    assert!(pos.x + outer.x <= constrain.max.x + 0.01);
    assert!(pos.y + outer.y <= constrain.max.y + 0.01);
    assert_eq!(pos, egui::pos2(16.0, 16.0));
    assert!(default_inner.y < 560.0);
    assert_eq!(max_inner, egui::vec2(916.0, 450.0));
}

#[test]
fn clamp_panel_layout_keeps_preferred_size_on_large_viewport() {
    let constrain = egui::Rect::from_min_size(egui::pos2(16.0, 16.0), egui::vec2(1888.0, 1048.0));
    let chrome = egui::vec2(12.0, 58.0);
    let (default_inner, max_inner, pos) =
        clamp_panel_layout(constrain, chrome, 440.0, 560.0, egui::pos2(16.0, 480.0));

    assert_eq!(default_inner, egui::vec2(440.0, 560.0));
    assert_eq!(max_inner, egui::vec2(1876.0, 990.0));
    // outer 高さ 618 のため y=480 では下端がはみ出す → 446 へクランプ。
    assert_eq!(pos, egui::pos2(16.0, 446.0));
}

#[test]
fn apply_settings_list_action_moves_and_removes_entries() {
    let mut items = vec!["a", "b", "c"];

    apply_settings_list_action(&mut items, SettingsListAction::MoveDown(0));
    assert_eq!(items, vec!["b", "a", "c"]);

    apply_settings_list_action(&mut items, SettingsListAction::MoveUp(2));
    assert_eq!(items, vec!["b", "c", "a"]);

    apply_settings_list_action(&mut items, SettingsListAction::Remove(1));
    assert_eq!(items, vec!["b", "a"]);
}

#[test]
fn apply_settings_list_action_moves_entry_to_index() {
    let mut items = vec!["a", "b", "c", "d"];

    apply_settings_list_action(&mut items, SettingsListAction::MoveTo { from: 0, to: 2 });
    assert_eq!(items, vec!["b", "c", "a", "d"]);

    apply_settings_list_action(&mut items, SettingsListAction::MoveTo { from: 3, to: 1 });
    assert_eq!(items, vec!["b", "d", "c", "a"]);
}

#[test]
fn apply_settings_list_action_ignores_invalid_moves() {
    let mut items = vec!["a", "b"];

    apply_settings_list_action(&mut items, SettingsListAction::MoveUp(0));
    apply_settings_list_action(&mut items, SettingsListAction::MoveDown(1));
    apply_settings_list_action(&mut items, SettingsListAction::MoveTo { from: 0, to: 2 });
    apply_settings_list_action(&mut items, SettingsListAction::MoveTo { from: 2, to: 0 });
    apply_settings_list_action(&mut items, SettingsListAction::Remove(2));

    assert_eq!(items, vec!["a", "b"]);
}

#[test]
fn directory_open_targets_expose_only_app_path_roots() {
    let root = unique_test_dir("bmz-ui-directory-targets");
    let app_paths = AppPaths::from_dirs(
        root.join("resources"),
        root.join("data"),
        root.join("cache"),
        root.join("logs"),
    );

    let targets = directory_open_targets(&app_paths);
    let labels = targets.iter().map(|target| target.label).collect::<Vec<_>>();
    let paths = targets.iter().map(|target| target.path).collect::<Vec<_>>();

    assert_eq!(labels, vec!["resource_dir", "data_dir", "cache_dir", "logs_dir"]);
    assert_eq!(
        paths,
        vec![
            app_paths.resource_dir.as_path(),
            app_paths.data_dir.as_path(),
            app_paths.cache_dir.as_path(),
            app_paths.logs_dir.as_path(),
        ]
    );
}

#[test]
fn manual_url_uses_ir_locale_prefix_except_japanese() {
    assert_eq!(manual_url(AppLocale::Ja), "https://bmz-player.hyrorre.workers.dev/manual");
    assert_eq!(manual_url(AppLocale::En), "https://bmz-player.hyrorre.workers.dev/en/manual");
    assert_eq!(manual_url(AppLocale::ZhTw), "https://bmz-player.hyrorre.workers.dev/zh-TW/manual");
}

#[test]
fn combined_license_notice_uses_packaged_notice_files() {
    let root = unique_test_dir("bmz-ui-license-packaged");
    let resource_dir = root.join("resources");
    let license_dir = resource_dir.join("licenses");
    fs::create_dir_all(&license_dir).unwrap();
    fs::write(license_dir.join("third-party-notices.txt"), "packaged third party").unwrap();
    fs::write(license_dir.join("rust-dependency-licenses.txt"), "packaged rust report").unwrap();
    let app_paths =
        AppPaths::from_dirs(resource_dir, root.join("data"), root.join("cache"), root.join("logs"));

    let notice = combined_license_notice_text_with_repo_root(&app_paths, &root);

    assert!(notice.contains("packaged third party"));
    assert!(notice.contains("packaged rust report"));
    assert!(!notice.contains("The generated Rust dependency license report was not found."));
}

#[test]
fn combined_license_notice_uses_local_rust_report_for_development() {
    let root = unique_test_dir("bmz-ui-license-local");
    let resource_dir = root.join("resources");
    let license_dir = resource_dir.join("licenses");
    fs::create_dir_all(&license_dir).unwrap();
    fs::write(license_dir.join("third-party-notices.txt"), "packaged third party").unwrap();
    fs::write(root.join("rust-dependency-licenses.txt"), "local rust report").unwrap();
    let app_paths =
        AppPaths::from_dirs(resource_dir, root.join("data"), root.join("cache"), root.join("logs"));

    let notice = combined_license_notice_text_with_repo_root(&app_paths, &root);

    assert!(notice.contains("packaged third party"));
    assert!(notice.contains("local rust report"));
    assert!(!notice.contains("The generated Rust dependency license report was not found."));
}

#[test]
fn combined_license_notice_explains_missing_rust_report() {
    let root = unique_test_dir("bmz-ui-license-missing");
    let app_paths = AppPaths::from_dirs(
        root.join("resources"),
        root.join("data"),
        root.join("cache"),
        root.join("logs"),
    );

    let notice = combined_license_notice_text_with_repo_root(&app_paths, &root);

    assert!(notice.contains("BMZ Player Third-Party Notices"));
    assert!(notice.contains("The generated Rust dependency license report was not found."));
    assert!(notice.contains("cargo-about generate --workspace --locked --fail"));
}

#[test]
fn glob_candidates_lists_files_matching_simple_pattern() {
    let root = unique_test_dir("bmz-ui-glob");
    fs::create_dir_all(root.join("parts")).unwrap();
    fs::write(root.join("parts/a.png"), []).unwrap();
    fs::write(root.join("parts/b.png"), []).unwrap();
    fs::write(root.join("parts/c.txt"), []).unwrap();

    let candidates = glob_candidates(&root, "parts/*.png");

    assert_eq!(candidates.len(), 2);
    assert!(candidates.contains(&"parts/a.png".to_string()));
    assert!(candidates.contains(&"parts/b.png".to_string()));
}

#[test]
fn glob_candidates_strips_beatoraja_filter_suffix() {
    let root = unique_test_dir("bmz-ui-glob");
    fs::create_dir_all(root.join("parts/lanecover_lift")).unwrap();
    fs::write(root.join("parts/lanecover_lift/default.png"), []).unwrap();
    fs::write(root.join("parts/lanecover_lift/TYPE-M.png"), []).unwrap();

    let candidates = glob_candidates(&root, "parts/lanecover_lift/*.png|lanecover|");

    assert_eq!(candidates.len(), 2);
    assert!(candidates.contains(&"parts/lanecover_lift/TYPE-M.png".to_string()));
    assert!(candidates.contains(&"parts/lanecover_lift/default.png".to_string()));
}

#[test]
fn glob_candidates_use_lua_skin_library_context_for_sibling_packages() {
    let library_root_dir = unique_test_dir("bmz-ui-package-glob");
    let library_root = library_root_dir.join("skins");
    let entry_dir = library_root.join("GenericTheme-master/play");
    let extension = library_root.join("Hub/extension/sample");
    fs::create_dir_all(&entry_dir).unwrap();
    fs::create_dir_all(&extension).unwrap();
    let entry = entry_dir.join("Hub_play7.luaskin");
    fs::write(&entry, "return { type = 0 }").unwrap();
    fs::write(extension.join("parts.png"), []).unwrap();
    let package = bmz_skin::SkinPathContext::new(&entry, [library_root]).unwrap();
    let context = SkinUiPathContext::package(&entry_dir, package);

    assert_eq!(
        glob_candidates_for_skin(&context, "../../Hub/extension/*|1|"),
        vec!["../../Hub/extension/sample".to_string()]
    );
    assert_eq!(
        glob_candidates_for_skin(&context, "skin/Hub/extension/*/parts.png"),
        vec!["skin/Hub/extension/sample/parts.png".to_string()]
    );
}

#[test]
fn normalize_filepath_selection_maps_legacy_basename_to_relative_candidate() {
    let candidates =
        vec!["parts/gauge/default.png".to_string(), "parts/gauge/blue.png".to_string()];

    assert_eq!(
        normalize_filepath_selection("blue.png", &candidates).as_deref(),
        Some("parts/gauge/blue.png")
    );
    assert_eq!(normalize_filepath_selection("old/blue.png", &candidates), None);
}

#[test]
fn filepath_selection_neighbors_include_random_before_glob_candidates() {
    let candidates = vec!["parts/light.png".to_string(), "parts/dark.png".to_string()];

    assert_eq!(
        next_filepath_selection(RANDOM_FILE_SELECTION, &candidates).as_deref(),
        Some("parts/light.png")
    );
    assert_eq!(
        previous_filepath_selection("parts/light.png", &candidates).as_deref(),
        Some(RANDOM_FILE_SELECTION)
    );
    assert_eq!(
        next_filepath_selection("parts/light.png", &candidates).as_deref(),
        Some("parts/dark.png")
    );
    assert_eq!(previous_filepath_selection(RANDOM_FILE_SELECTION, &candidates), None);
    assert_eq!(next_filepath_selection("parts/dark.png", &candidates), None);
}

#[test]
fn filepath_selection_neighbors_accept_legacy_basename() {
    let candidates = vec!["parts/light.png".to_string(), "parts/dark.png".to_string()];

    assert_eq!(
        previous_filepath_selection("dark.png", &candidates).as_deref(),
        Some("parts/light.png")
    );
    assert_eq!(next_filepath_selection("unknown.png", &candidates), None);
}

#[test]
fn property_default_uses_matching_def_name_or_first_item() {
    let prop = SkinPropertyDef {
        category: String::new(),
        name: "Notes".to_string(),
        item: vec![
            bmz_render::skin::SkinPropertyItemDef { name: "Light".to_string(), op: 1 },
            bmz_render::skin::SkinPropertyItemDef { name: "Dark".to_string(), op: 2 },
        ],
        def: "Dark".to_string(),
    };
    assert_eq!(property_default(&prop), "Dark");

    let prop = SkinPropertyDef { def: "Missing".to_string(), ..prop };
    assert_eq!(property_default(&prop), "Light");
}

#[test]
fn property_selection_neighbors_follow_item_order_and_stop_at_ends() {
    let prop = SkinPropertyDef {
        category: String::new(),
        name: "Notes".to_string(),
        item: vec![
            bmz_render::skin::SkinPropertyItemDef { name: "Light".to_string(), op: 1 },
            bmz_render::skin::SkinPropertyItemDef { name: "Dark".to_string(), op: 2 },
            bmz_render::skin::SkinPropertyItemDef { name: "Color".to_string(), op: 3 },
        ],
        def: "Dark".to_string(),
    };

    assert_eq!(previous_property_selection(&prop, "Dark"), Some("Light"));
    assert_eq!(next_property_selection(&prop, "Dark"), Some("Color"));
    assert_eq!(previous_property_selection(&prop, "Light"), None);
    assert_eq!(next_property_selection(&prop, "Color"), None);
}

#[test]
fn property_selection_neighbors_accept_legacy_numeric_selection() {
    let prop = SkinPropertyDef {
        category: String::new(),
        name: "Notes".to_string(),
        item: vec![
            bmz_render::skin::SkinPropertyItemDef { name: "Light".to_string(), op: 1 },
            bmz_render::skin::SkinPropertyItemDef { name: "Dark".to_string(), op: 2 },
            bmz_render::skin::SkinPropertyItemDef { name: "Color".to_string(), op: 3 },
        ],
        def: "Dark".to_string(),
    };

    assert_eq!(previous_property_selection(&prop, "2"), Some("Light"));
    assert_eq!(next_property_selection(&prop, "2"), Some("Color"));
    assert_eq!(next_property_selection(&prop, "unknown"), None);
}

#[test]
fn filepath_default_matches_def_with_or_without_extension_case_insensitive() {
    let filepath = SkinFilepathDef {
        category: String::new(),
        name: "Notes".to_string(),
        path: "notes/*.png".to_string(),
        def: "default".to_string(),
    };
    let candidates = vec!["aaa.png".to_string(), "Default.PNG".to_string()];

    assert_eq!(filepath_default(&filepath, &candidates).as_deref(), Some("Default.PNG"));

    let filepath = SkinFilepathDef { def: "missing".to_string(), ..filepath };
    assert_eq!(filepath_default(&filepath, &candidates).as_deref(), Some("aaa.png"));
}

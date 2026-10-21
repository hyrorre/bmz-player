use super::*;

/// ModernChic picks the difficulty color at load and indexes it without a nil
/// check; mz-select builds its difficulty label from the level and table text.
/// Like those skins, the header pass (no `skin_config`) returns only a header.
const DECIDE_DIFFICULTY_SKIN: &str = r#"
if skin_config == nil then return {type=6, name='header'} end
local s = require('main_state')
local rgb
if s.option(151) then rgb = {6, 255, 0}
elseif s.option(152) then rgb = {18, 210, 215}
elseif s.option(153) then rgb = {255, 192, 0}
elseif s.option(154) then rgb = {255, 0, 0}
elseif s.option(155) then rgb = {148, 44, 150}
elseif s.option(150) then rgb = {195, 195, 195}
end
local label = s.number(96)
if s.option(150) then label = 'UNKNOWN' .. label .. ' ' .. s.text(1003)
elseif s.option(153) then label = 'HYPER' .. label .. ' ' .. s.text(1003)
end
return {type=6, name=string.format('%d,%d,%d|%s', rgb[1], rgb[2], rgb[3], label)}
"#;

fn decide_snapshot(difficulty: &str, level: &str, table: &str) -> RenderSnapshot {
    RenderSnapshot {
        difficulty_name: difficulty.to_string(),
        play_level: level.to_string(),
        table_text_primary: "Table".to_string(),
        table_text_secondary: "Lv".to_string(),
        table_text_fallback: table.to_string(),
        ..RenderSnapshot::default()
    }
}

#[test]
fn decide_chart_load_state_matches_decide_rendering() {
    let frontend = lua_runtime_state_for_frontend("Player", None);
    assert!((150..=155).all(|option| !frontend.option_values.contains_key(&option)));
    for (label, expected) in [
        ("", 150),
        ("unknown", 150),
        ("6", 150),
        ("1", 151),
        ("BEGINNER", 151),
        ("normal", 152),
        ("Hyper", 153),
        ("3", 153),
        (" ANOTHER ", 154),
        ("insane", 155),
    ] {
        let mut state = lua_runtime_state_for_frontend("Player", None);
        apply_decide_chart_lua_load_state(&mut state, &decide_snapshot(label, "☆12", "★12"));
        for option in 150..=155 {
            assert_eq!(state.option_values.get(&option), Some(&(option == expected)), "{label:?}");
        }
        assert_eq!(state.number_values[&96], 12);
        assert_eq!(state.text_values[&1001], "Table");
        assert_eq!(state.text_values[&1002], "Lv");
        assert_eq!(state.text_values[&1003], "★12");
        assert_eq!(state.text_values[&2], "Player", "frontend values stay");
    }
}

/// Drives the same reload function as Decide scene entry and receives the
/// decoded document from the skin pipeline.
#[test]
fn decide_scene_entry_reload_passes_the_selected_chart_to_lua_skins() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("decide.luaskin");
    std::fs::write(&path, DECIDE_DIFFICULTY_SKIN).unwrap();
    let paths = crate::paths::AppPaths::from_dirs(
        temp.path().join("resources"),
        temp.path().join("data"),
        temp.path().join("cache"),
        temp.path().join("logs"),
    );
    let skin = SkinConfig { decide: path.to_string_lossy().into_owned(), ..Default::default() };
    let mut pipeline = SkinPipelineRuntime::new();
    let decoded = pipeline.decode_rx.take().unwrap();
    let decide = |pipeline: &mut SkinPipelineRuntime, chart: Option<&RenderSnapshot>| {
        let (_, pending_decide, _) = reload_skin_textures(
            &paths,
            pipeline,
            SkinReloadRequest { decide: true, ..Default::default() },
            "Player",
            None,
            &skin,
            bmz_skin::LuaSkinRuntimeMode::Auto,
            false,
            None,
            chart,
        );
        assert!(pending_decide);
        let request = pipeline.screen_request(SkinKind::Decide).unwrap();
        let result = decoded.recv_timeout(Duration::from_secs(30)).unwrap();
        assert_eq!(result.generation, request.generation);
        (request.runtime_state, result.result)
    };

    // Startup and skin-panel decodes have no selected chart, as before.
    let (state, result) = decide(&mut pipeline, None);
    assert!((150..=155).all(|option| !state.option_values.contains_key(&option)));
    let error = format!("{:#}", result.err().expect("no difficulty: the color stays nil"));
    assert!(error.contains("attempt to index a nil value"), "{error}");

    let hyper = decide_snapshot("HYPER", "12", "★12");
    let (state, result) = decide(&mut pipeline, Some(&hyper));
    assert_eq!(state.option_values.get(&153), Some(&true));
    assert_eq!(result.unwrap().document.name, "255,192,0|HYPER12 ★12");

    // The difficulty is a document dependency, so a different chart reloads.
    let unknown = decide_snapshot("", "", "");
    let (_, result) = decide(&mut pipeline, Some(&unknown));
    assert_eq!(result.unwrap().document.name, "195,195,195|UNKNOWN0 ");
}

fn installed_decide(relative: &str) -> Option<(PathBuf, PathBuf)> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data/skins");
    let path = root.join(relative);
    if path.is_file() {
        Some((root, path))
    } else {
        eprintln!("SKIP missing {}", path.display());
        None
    }
}

fn load_installed_decide(
    root: &Path,
    path: &Path,
    state: &bmz_skin::LuaLoadRuntimeState,
) -> Result<bmz_skin::LoadedSkinDocument> {
    let context = bmz_skin::SkinPathContext::new(path, [root.to_path_buf()]).unwrap();
    bmz_skin::load_lua_skin_with_path_context(
        &context,
        &BTreeMap::new(),
        &BTreeMap::new(),
        state,
        &BTreeMap::new(),
    )
}

#[test]
fn installed_decide_skins_load_with_the_selected_chart_when_available() {
    let mut state = lua_runtime_state_for_frontend("Player", None);
    apply_decide_chart_lua_load_state(&mut state, &decide_snapshot("HYPER", "12", "★12"));
    if let Some((root, path)) = installed_decide("mz-select/decide/decide.luaskin") {
        let loaded = load_installed_decide(&root, &path, &state).unwrap();
        let label = loaded.document.text.iter().find(|text| text.id == "difficulty").unwrap();
        assert_eq!(label.constant_text, "HYPER12 ★12");
        eprintln!("mz-select decide label: {}", label.constant_text);
    }
    if let Some((root, path)) = installed_decide("ModernChic/decide.luaskin") {
        let frontend = lua_runtime_state_for_frontend("Player", None);
        assert!(load_installed_decide(&root, &path, &frontend).is_err());
        load_installed_decide(&root, &path, &state).unwrap();
        eprintln!("ModernChic decide loads with the selected chart");
    }
}

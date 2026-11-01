use super::*;
use crate::bootstrap::profile_tests::ProfileTestDir;

#[test]
fn initial_select_is_queued_before_surface_and_not_installed_synchronously() {
    let data = ProfileTestDir::new();
    std::fs::create_dir_all(&data.paths.data_dir).unwrap();
    let path = data.paths.data_dir.join("initial-select.json");
    std::fs::write(&path, r#"{"type":5,"name":"initial asynchronous select"}"#).unwrap();
    let mut renderer = Renderer::default();
    let mut pipeline = SkinPipelineRuntime::new();
    let skin = SkinConfig { select: path.to_string_lossy().into_owned(), ..Default::default() };
    let (_, _, pending_select, _, _) = load_initial_skin_textures(
        &mut renderer,
        &data.paths,
        &pipeline,
        0,
        FrontendLuaLoadInputs {
            player_name: "test",
            ir_name: None,
            player_stats: &PlayerStatsSnapshot::default(),
        },
        &skin,
        true,
        bmz_skin::LuaSkinRuntimeMode::default(),
        false,
    );
    assert!(pending_select);
    assert!(renderer.select_skin_document().is_none());
    assert_eq!(pipeline.screen_request(SkinKind::Select).unwrap().path, path);
    loop {
        let result =
            pipeline.decode_rx.as_ref().unwrap().recv_timeout(Duration::from_secs(10)).unwrap();
        if result.kind == SkinKind::Select {
            assert_eq!(result.result.unwrap().document.name, "initial asynchronous select");
            break;
        }
    }
    pipeline.shutdown_workers();
}

#[test]
fn frontend_reload_reuses_installed_fonts_after_cache_eviction_and_reloads_changes() {
    let data = ProfileTestDir::new();
    std::fs::create_dir_all(&data.paths.data_dir).unwrap();
    let path = data.paths.data_dir.join("decide.json");
    let font = data.paths.data_dir.join("font.ttf");
    std::fs::write(&font, b"font payload").unwrap();
    std::fs::write(&path, r#"{"type":6,"font":[{"id":"shared","path":"font.ttf"}]}"#).unwrap();
    let skin = SkinConfig { decide: path.to_string_lossy().into_owned(), ..Default::default() };
    let mut pipeline = SkinPipelineRuntime::new();
    for attempt in 0..3 {
        if attempt == 1 {
            // The renderer still owns this font even after its CPU payload is evicted.
            *pipeline.font_cache.lock().unwrap() = Default::default();
        } else if attempt == 2 {
            std::fs::write(&font, b"changed font payload").unwrap();
        }
        let (_, pending_decide, _) = reload_skin_textures(
            &data.paths,
            &mut pipeline,
            SkinReloadRequest { decide: true, ..Default::default() },
            FrontendLuaLoadInputs {
                player_name: "test",
                ir_name: None,
                player_stats: &PlayerStatsSnapshot::default(),
            },
            &skin,
            bmz_skin::LuaSkinRuntimeMode::default(),
            false,
            None,
            None,
        );
        assert!(pending_decide);
        let decoded = pipeline
            .decode_rx
            .as_ref()
            .unwrap()
            .recv_timeout(Duration::from_secs(10))
            .unwrap()
            .result
            .unwrap();
        assert_eq!(decoded.stats.font_payload_skipped, usize::from(attempt == 1));
        let font = &decoded.fonts[0];
        assert_eq!(font.data.is_none(), attempt == 1);
        pipeline
            .installed_font_cache
            .insert(font.stored_id.clone(), font.cache_key.clone().unwrap());
    }
    pipeline.shutdown_workers();
}

#[test]
fn select_lua_load_reads_profile_player_stats_and_reloads_when_they_change() {
    let data = ProfileTestDir::new();
    let root = data.paths.resource_dir.join("skins/stats");
    std::fs::create_dir_all(&root).unwrap();
    // mz-select's player rank panel computes exp while loading.
    std::fs::write(
        root.join("select.luaskin"),
        r#"
local state = require("main_state")
local skin = { type = 5 }
if skin_config == nil then return skin end
local exp = state.number(33) * 2 + state.number(34)
skin.name = table.concat({
    exp, state.number(17), state.number(18), state.number(19),
    state.number(30), state.number(31), state.number(333),
}, ":")
return skin
"#,
    )
    .unwrap();
    let skin =
        SkinConfig { select: "resource:skins/stats/select.luaskin".into(), ..Default::default() };
    let mut pipeline = SkinPipelineRuntime::new();
    let mut load = |stats: &PlayerStatsSnapshot| {
        let (pending_select, _, _) = reload_skin_textures(
            &data.paths,
            &mut pipeline,
            SkinReloadRequest { select: true, ..Default::default() },
            FrontendLuaLoadInputs { player_name: "test", ir_name: None, player_stats: stats },
            &skin,
            bmz_skin::LuaSkinRuntimeMode::default(),
            false,
            None,
            None,
        );
        assert!(pending_select);
        let result =
            pipeline.decode_rx.as_ref().unwrap().recv_timeout(Duration::from_secs(10)).unwrap();
        result.result.unwrap().document.name
    };
    let mut stats = PlayerStatsSnapshot {
        play_count: 12,
        clear_count: 7,
        playtime_seconds: 3 * 3_600 + 4 * 60 + 5,
        fast_pgreat: 600,
        slow_pgreat: 400,
        fast_great: 150,
        slow_great: 50,
        fast_good: 20,
        slow_bad: 5,
        fast_poor: 9,
        ..Default::default()
    };
    assert_eq!(load(&stats), "2200:3:4:5:12:7:1225");
    // Returning to Select after a play must not reuse the cached document.
    stats.slow_pgreat += 10;
    stats.play_count += 1;
    assert_eq!(load(&stats), "2220:3:4:5:13:7:1235");
    pipeline.shutdown_workers();
}

#[test]
fn mz_select_player_rank_panel_uses_profile_player_stats() {
    let skin_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../data/skins/mz-select/music_select.luaskin");
    if !skin_path.is_file() {
        return;
    }
    let data = ProfileTestDir::new();
    let skin =
        SkinConfig { select: skin_path.to_string_lossy().into_owned(), ..Default::default() };
    // exp = 1000 * 2 + 200 = 2200: rank 3 (500 + 1000 < 2200 < +1500), 700 / 1500 exp.
    let stats = PlayerStatsSnapshot {
        play_count: 12,
        clear_count: 7,
        playtime_seconds: 3 * 3_600 + 4 * 60,
        fast_pgreat: 1_000,
        fast_great: 200,
        ..Default::default()
    };
    let mut pipeline = SkinPipelineRuntime::new();
    let (pending_select, _, _) = reload_skin_textures(
        &data.paths,
        &mut pipeline,
        SkinReloadRequest { select: true, ..Default::default() },
        FrontendLuaLoadInputs { player_name: "test", ir_name: None, player_stats: &stats },
        &skin,
        bmz_skin::LuaSkinRuntimeMode::default(),
        false,
        None,
        None,
    );
    assert!(pending_select);
    let document = pipeline
        .decode_rx
        .as_ref()
        .unwrap()
        .recv_timeout(Duration::from_secs(60))
        .unwrap()
        .result
        .unwrap()
        .document;
    pipeline.shutdown_workers();

    let text =
        |id: &str| document.text.iter().find(|text| text.id == id).unwrap().constant_text.clone();
    assert_eq!(text("default_playerrank2_profile_player_rank"), "3");
    assert_eq!(text("default_playerrank2_profile_play_cp"), "7 / 12");
    assert_eq!(text("default_playerrank2_profile_play_time"), "3:04:00");
    let graph_width = document
        .destination
        .iter()
        .find_map(|entry| match entry {
            DestinationListEntry::Single(destination)
                if destination.id == "default_playerrank2_graph" =>
            {
                destination.dst.first()
            }
            _ => None,
        })
        .and_then(|entry| match entry {
            SkinDstEntry::Frame(frame) => frame.w,
            _ => None,
        });
    assert_eq!(graph_width, Some(84));
}

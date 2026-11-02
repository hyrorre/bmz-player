use super::*;
use bmz_core::lane::KeyMode;
use bmz_render::scene::{SelectBattleSnapshot, SelectRivalSnapshot, SelectRowKind};

#[test]
fn mz_battle_info_switches_all_three_bodies_and_keeps_player_options_separate() {
    let path = test_app_paths().resource_dir.join("skins/mz-select/music_select.luaskin");
    assert!(path.exists(), "initialize mz-select to verify the actual assets");
    let decoded = decode_beatoraja_skin(&path, SkinKind::Select).unwrap();
    let font_path = |id: &str| {
        let text = decoded.document.text.iter().find(|text| text.id == id).unwrap();
        let font_id = text.font.rsplit(':').next().unwrap();
        &decoded.document.font.iter().find(|font| font.id == font_id).unwrap().path
    };
    assert_eq!(font_path("mz_battle_title"), font_path("default_network_network"));
    assert!(font_path("mz_battle_title").ends_with("m_select_system.fnt"));
    let option_texture = bmz_render::plan::TextureId(
        decoded
            .sources
            .iter()
            .find(|source| source.source_id == "mz_battle_options")
            .unwrap()
            .texture
            .0,
    );
    let panel_option_images = |renderer: &Renderer| {
        renderer.last_plan().unwrap().commands.iter().filter(|command| matches!(command,
            DrawCommand::Image { texture, rect, .. } if *texture == option_texture
                && rect.x >= 350.0 / 1920.0 && rect.x < 660.0 / 1920.0 && rect.y > 615.0 / 1080.0
        )).count()
    };
    assert!(decoded.document.text.iter().filter(|text| text.id.starts_with("mz_battle_")).all(
        |text| {
            !["HARD", "RIVAL OPTION", "LANE ORDER", "REPLICATION"]
                .contains(&text.constant_text.as_str())
                && text.ref_id != 20_204
        }
    ));
    for entry in &decoded.document.destination {
        if let DestinationListEntry::Single(destination) = entry {
            let id = destination.id.as_str();
            if id.starts_with("default_ranking_")
                || id.starts_with("default_irdata_")
                || id == "default_rivaldata_rivaldata_bg"
            {
                assert!(destination.op.contains(&-20_200), "{id}");
            }
        }
    }
    let mut renderer = Renderer::default();
    install_decoded_skin(&mut renderer, decoded, bmz_render::skin::default_skin_manifest())
        .unwrap();
    let mut snapshot = SelectSnapshot {
        time: TimeUs(3_000_000),
        selection_time: TimeUs(3_000_000),
        assist: "G-BATTLE".into(),
        arrange: "S-RANDOM".into(),
        chart_replication_mode: "RIVALCHART".into(),
        rival_selected: true,
        rival: Some(SelectRivalSnapshot {
            display_name: "rival".into(),
            ex_score: 2000,
            clear_index: 7,
            max_combo: 800,
            bp: 2,
            judge_counts: None,
        }),
        ir: bmz_render::scene::ResultIrSnapshot { online: true, ..Default::default() },
        rows: vec![SelectRowSnapshot {
            kind: SelectRowKind::Song,
            in_library: true,
            chart_key_mode: Some(KeyMode::K7),
            ..Default::default()
        }],
        ..Default::default()
    };
    let labels = |renderer: &mut Renderer, snapshot: &SelectSnapshot| {
        renderer.prepare_scene(AppSceneSnapshot::Select(snapshot.clone()));
        renderer
            .last_plan()
            .unwrap()
            .commands
            .iter()
            .filter_map(|command| match command {
                DrawCommand::Text { text, .. } => Some(text.clone()),
                _ => None,
            })
            .collect::<Vec<_>>()
    };
    let muted_statuses = |renderer: &Renderer| {
        let colors = renderer
            .last_plan()
            .unwrap()
            .commands
            .iter()
            .filter_map(|command| match command {
                DrawCommand::Text { text, style, .. }
                    if ["NO DATA", "LOADING", "NOT FIXED"].contains(&text.as_str()) =>
                {
                    Some(style.color)
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        assert!(!colors.is_empty());
        assert!(colors.iter().all(|color| color.r == color.g
            && color.g == color.b
            && color.r > 0.0
            && color.r < 0.6
            && color.a == 1.0));
    };
    let original = labels(&mut renderer, &snapshot);
    assert!(!original.iter().any(|text| text == "g-battle data"));
    assert!(original.iter().any(|text| text == "rival data"));
    let mut pattern: Vec<u8> = (0..16).collect();
    pattern[1..8].copy_from_slice(&[2, 5, 3, 7, 1, 4, 6]);
    snapshot.battle = Some(Arc::new(SelectBattleSnapshot {
        name: "実データの相手".into(),
        key_mode: KeyMode::K7,
        arrange: Some("RANDOM".into()),
        lane_shuffle_pattern: pattern,
        ..Default::default()
    }));
    let battle = labels(&mut renderer, &snapshot);
    for expected in ["g-battle data", "RIVALCHART"] {
        assert!(battle.iter().any(|text| text == expected), "missing {expected}: {battle:?}");
    }
    assert!(!battle.iter().any(|text| text == "実データの相手"));
    assert!(
        !battle.iter().any(|text| ["rival data", "ir data", "ランキング"].contains(&text.as_str()))
    );
    for key in 1..=7 {
        assert!(battle.iter().any(|text| text == &key.to_string()));
    }
    assert_eq!(panel_option_images(&renderer), 1);
    snapshot.battle = Some(Arc::new(SelectBattleSnapshot::default()));
    let missing = labels(&mut renderer, &snapshot);
    assert!(missing.iter().any(|text| text == "NO DATA"));
    assert!(!missing.iter().any(|text| text == "NOT FIXED"));
    muted_statuses(&renderer);
    assert_eq!(panel_option_images(&renderer), 0);
    snapshot.battle = Some(Arc::new(SelectBattleSnapshot {
        arrange: Some("RANDOM".into()),
        ..Default::default()
    }));
    assert!(labels(&mut renderer, &snapshot).iter().any(|text| text == "NO DATA"));
    snapshot.battle = Some(Arc::new(SelectBattleSnapshot {
        arrange: Some("S-RANDOM".into()),
        ..Default::default()
    }));
    let variable = labels(&mut renderer, &snapshot);
    assert!(variable.iter().any(|text| text == "NOT FIXED"));
    assert!(!variable.iter().any(|text| text == "NO DATA"));
    muted_statuses(&renderer);
    snapshot.battle = Some(Arc::new(SelectBattleSnapshot { loading: true, ..Default::default() }));
    let loading = labels(&mut renderer, &snapshot);
    assert!(loading.iter().any(|text| text == "LOADING"));
    assert!(!loading.iter().any(|text| text == "NO DATA" || text == "NOT FIXED"));
    muted_statuses(&renderer);
    snapshot.battle = None;
    assert_eq!(labels(&mut renderer, &snapshot), original);
}

#[test]
fn mz_battle_info_colors_keys_by_the_parity_of_their_original_lane() {
    let path = test_app_paths().resource_dir.join("skins/mz-select/music_select.luaskin");
    assert!(path.exists(), "initialize mz-select to verify the actual assets");
    let decoded = decode_beatoraja_skin(&path, SkinKind::Select).unwrap();
    let keys = decoded.sources.iter().find(|source| source.source_id == "mz_battle_keys").unwrap();
    let (texture, width) = (bmz_render::plan::TextureId(keys.texture.0), keys.size.width);
    let mut renderer = Renderer::default();
    install_decoded_skin(&mut renderer, decoded, bmz_render::skin::default_skin_manifest())
        .unwrap();
    // White and black key frames of result/keys1p.png, left to right.
    let mut shades = |key_mode, pattern: Vec<u8>| {
        let snapshot = SelectSnapshot {
            time: TimeUs(3_000_000),
            battle: Some(Arc::new(SelectBattleSnapshot {
                key_mode,
                arrange: Some("RANDOM".into()),
                lane_shuffle_pattern: pattern,
                ..Default::default()
            })),
            ..Default::default()
        };
        renderer.prepare_scene(AppSceneSnapshot::Select(snapshot));
        let mut tiles: Vec<(f32, char)> = renderer
            .last_plan()
            .unwrap()
            .commands
            .iter()
            .filter_map(|command| match command {
                DrawCommand::Image { texture: drawn, rect, uv, .. } if *drawn == texture => {
                    let source_x = (uv.x * width).round();
                    Some((rect.x, if source_x == 243.0 { 'B' } else { 'W' }))
                }
                _ => None,
            })
            .collect();
        tiles.sort_by(|a, b| a.0.total_cmp(&b.0));
        tiles.into_iter().map(|tile| tile.1).collect::<String>()
    };
    // 9K: lane 8 is even like 2, 4 and 6; lane 9 is odd.
    assert_eq!(
        shades(KeyMode::K9, vec![0, 9, 5, 3, 7, 1, 8, 4, 2, 6, 10, 11, 12, 13, 14, 15]),
        "WWWWWBBBB"
    );
    // A scratch moved onto a key stays white, as before.
    let mut scratch: Vec<u8> = (0..16).collect();
    scratch[1..8].copy_from_slice(&[2, 5, 3, 7, 1, 4, 6]);
    scratch.swap(0, 3);
    assert_eq!(shades(KeyMode::K7, scratch), "BWWWWBB");
}

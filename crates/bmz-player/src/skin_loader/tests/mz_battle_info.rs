use super::*;
use bmz_core::lane::KeyMode;
use bmz_render::scene::{SelectBattleSnapshot, SelectRivalSnapshot, SelectRowKind};

#[test]
fn mz_battle_info_switches_all_three_bodies_and_keeps_player_options_separate() {
    let path = test_app_paths().resource_dir.join("skins/mz-select/music_select.luaskin");
    assert!(path.exists(), "initialize mz-select to verify the actual assets");
    let decoded = decode_beatoraja_skin(&path, SkinKind::Select).unwrap();
    let font =
        decoded.document.font.iter().find(|font| font.id.ends_with("mz_battle_font")).unwrap();
    assert!(font.path.ends_with("mgenplus-1c-bold.ttf"));
    assert!(
        decoded
            .document
            .text
            .iter()
            .filter(|text| text.id.starts_with("mz_battle_"))
            .all(|text| text.constant_text != "HARD")
    );
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
    let original = labels(&mut renderer, &snapshot);
    assert!(!original.iter().any(|text| text == "G-BATTLE"));
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
    for expected in ["G-BATTLE", "実データの相手", "RANDOM", "LANE ORDER", "RIVALCHART"] {
        assert!(battle.iter().any(|text| text == expected), "missing {expected}: {battle:?}");
    }
    assert!(
        !battle.iter().any(|text| ["rival data", "ir data", "ランキング"].contains(&text.as_str()))
    );
    for key in 1..=7 {
        assert!(battle.iter().any(|text| text == &key.to_string()));
    }
    snapshot.battle = Some(Arc::new(SelectBattleSnapshot::default()));
    let missing = labels(&mut renderer, &snapshot);
    assert!(missing.iter().any(|text| text == "オプション情報なし"));
    assert!(!missing.iter().any(|text| text == "RANDOM"));
    snapshot.battle = None;
    assert_eq!(labels(&mut renderer, &snapshot), original);
}

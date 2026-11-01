use super::*;
use bmz_core::lane::KeyMode;
use bmz_render::plan::TextureId;
use bmz_render::scene::{SelectBattleSnapshot, SelectRivalSnapshot, SelectRowKind};

fn labels(renderer: &mut Renderer, snapshot: &SelectSnapshot) -> Vec<String> {
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
        .collect()
}

fn native_frame(renderer: &Renderer, x: f32, top: f32, w: f32, h: f32) -> usize {
    renderer
        .last_plan()
        .unwrap()
        .commands
        .iter()
        .filter(|command| {
            matches!(command,
                DrawCommand::Image { rect, .. } if (rect.x * 1920.0 - x).abs() < 0.01
                    && (rect.y * 1080.0 - top).abs() < 0.01 && (rect.width * 1920.0 - w).abs() < 0.01
                    && (rect.height * 1080.0 - h).abs() < 0.01
            )
        })
        .count()
}

fn key_rows(renderer: &Renderer, texture_id: TextureId, expected: &[&[u8]]) {
    let mut rows: BTreeMap<i32, Vec<(f32, u8)>> = BTreeMap::new();
    for command in &renderer.last_plan().unwrap().commands {
        if let DrawCommand::Image { texture, rect, uv, .. } = command
            && *texture == texture_id
            && (rect.width * 1920.0 - 25.0).abs() < 0.01
        {
            assert!((rect.height * 1080.0 - 35.0).abs() < 0.01);
            assert_eq!(uv.y, 0.0);
            rows.entry((rect.y * 1080.0).round() as i32)
                .or_default()
                .push((rect.x * 1920.0, (uv.x * 300.0 / 25.0).round() as u8 + 1));
        }
    }
    assert_eq!(rows.len(), expected.len());
    for (mut row, expected) in rows.into_values().zip(expected) {
        row.sort_by(|a, b| a.0.total_cmp(&b.0));
        assert_eq!(row.iter().map(|key| key.1).collect::<Vec<_>>(), *expected);
        for pair in row.windows(2) {
            assert!((pair[1].0 - pair[0].0 - 27.0).abs() < 0.01, "Result uses a 27px pitch");
        }
    }
}

#[test]
fn luxe_battle_info_switches_sp_dp_and_restores_native_panels_in_both_lua_modes() {
    let path = test_app_paths().resource_dir.join("skins/Luxez-Flat/music_select.luaskin");
    assert!(path.exists(), "initialize Luxez-Flat to verify its actual assets");
    for mode in [bmz_skin::LuaSkinRuntimeMode::Auto, bmz_skin::LuaSkinRuntimeMode::Compat] {
        let decoded = decode_beatoraja_skin_with_options_and_runtime_state(
            &path,
            SkinKind::Select,
            &BTreeMap::new(),
            &BTreeMap::new(),
            &bmz_skin::LuaLoadRuntimeState { runtime_mode: mode, ..Default::default() },
        )
        .unwrap();
        assert!(decoded.document.text.iter().any(|text| text.id == "luxe_battle_dp_title"));
        assert!(
            decoded.document.text.iter().filter(|text| text.id.starts_with("luxe_battle_")).all(
                |text| !["HARD", "RIVAL OPTION", "LANE ORDER", "REPLICATION"]
                    .contains(&text.constant_text.as_str())
                    && text.ref_id != 20_204
            )
        );
        let keys_texture = TextureId(
            decoded
                .sources
                .iter()
                .find(|source| source.source_id == "luxe_battle_keys")
                .unwrap()
                .texture
                .0,
        );
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
        let original = labels(&mut renderer, &snapshot);
        assert_eq!(native_frame(&renderer, 709.0, 664.0, 413.0, 150.0), 1);
        assert_eq!(native_frame(&renderer, 302.0, 663.0, 412.0, 284.0), 1);
        let mut pattern: Vec<u8> = (0..16).collect();
        pattern[1..8].copy_from_slice(&[2, 5, 3, 7, 1, 4, 6]);
        let single = SelectBattleSnapshot {
            name: "opponent name must stay hidden".into(),
            key_mode: KeyMode::K7,
            arrange: Some("RANDOM".into()),
            lane_shuffle_pattern: pattern,
            ..Default::default()
        };
        snapshot.battle = Some(Arc::new(single.clone()));
        let sp_labels = labels(&mut renderer, &snapshot);
        assert!(
            !sp_labels
                .iter()
                .any(|text| text == "opponent name must stay hidden" || text == "G-BATTLE INFO")
        );
        assert!(sp_labels.iter().any(|text| text == "RIVALCHART"));
        assert_eq!(native_frame(&renderer, 709.0, 664.0, 413.0, 150.0), 0);
        assert_eq!(native_frame(&renderer, 302.0, 663.0, 412.0, 284.0), 1);
        key_rows(&renderer, keys_texture, &[&[2, 5, 3, 7, 1, 4, 6]]);
        let mut pattern: Vec<u8> = (0..16).collect();
        pattern[0] = 15;
        pattern[15] = 0;
        pattern[1..8].copy_from_slice(&[9, 12, 10, 14, 8, 11, 13]);
        pattern[8..15].copy_from_slice(&[7, 6, 5, 4, 3, 2, 1]);
        let double = SelectBattleSnapshot {
            key_mode: KeyMode::K14,
            arrange: Some("RANDOM".into()),
            arrange_2p: Some("MIRROR".into()),
            double_option: Some("FLIP".into()),
            lane_shuffle_pattern: pattern,
            ..single.clone()
        };
        snapshot.battle = Some(Arc::new(double.clone()));
        let dp_labels = labels(&mut renderer, &snapshot);
        for expected in ["G-BATTLE INFO", "1P", "2P", "DP", "FLIP", "RIVALCHART"] {
            assert!(dp_labels.iter().any(|text| text == expected), "{mode:?}: missing {expected}");
        }
        assert_eq!(native_frame(&renderer, 709.0, 664.0, 413.0, 150.0), 1);
        assert_eq!(native_frame(&renderer, 302.0, 663.0, 412.0, 284.0), 0);
        assert_eq!(native_frame(&renderer, 707.0, 810.0, 412.0, 198.0), 1);
        key_rows(&renderer, keys_texture, &[&[2, 5, 3, 7, 1, 4, 6], &[7, 6, 5, 4, 3, 2, 1]]);
        let mut ten_pattern: Vec<u8> = (0..16).collect();
        ten_pattern[0] = 15;
        ten_pattern[15] = 0;
        ten_pattern[1..6].copy_from_slice(&[9, 12, 10, 8, 11]);
        ten_pattern[8..13].copy_from_slice(&[5, 4, 3, 2, 1]);
        snapshot.battle = Some(Arc::new(SelectBattleSnapshot {
            key_mode: KeyMode::K10,
            lane_shuffle_pattern: ten_pattern,
            ..double.clone()
        }));
        labels(&mut renderer, &snapshot);
        key_rows(&renderer, keys_texture, &[&[2, 5, 3, 1, 4], &[5, 4, 3, 2, 1]]);
        for (arrange, loading, expected) in [
            (None, false, "NO DATA"),
            (None, true, "LOADING"),
            (Some("S-RANDOM"), false, "NOT FIXED"),
        ] {
            snapshot.battle = Some(Arc::new(SelectBattleSnapshot {
                arrange: arrange.map(str::to_owned),
                arrange_2p: arrange.map(str::to_owned),
                loading,
                lane_shuffle_pattern: vec![],
                ..double.clone()
            }));
            let current = labels(&mut renderer, &snapshot);
            assert!(current.iter().any(|text| text == expected));
            assert_eq!(native_frame(&renderer, 302.0, 663.0, 412.0, 284.0), 0);
            key_rows(&renderer, keys_texture, &[]);
            for command in &renderer.last_plan().unwrap().commands {
                if let DrawCommand::Text { text, style, .. } = command
                    && text == expected
                {
                    assert!((style.color.r - 127.0 / 255.0).abs() < 0.001);
                    assert!((style.color.g - 128.0 / 255.0).abs() < 0.001);
                    assert!((style.color.b - 128.0 / 255.0).abs() < 0.001);
                }
            }
        }
        snapshot.battle = Some(Arc::new(SelectBattleSnapshot {
            key_mode: KeyMode::K9,
            lane_shuffle_pattern: vec![0, 9, 5, 3, 7, 1, 8, 4, 2, 6, 10, 11, 12, 13, 14, 15],
            ..single.clone()
        }));
        let pms_labels = labels(&mut renderer, &snapshot);
        assert!(pms_labels.iter().any(|text| text == "8"));
        assert!(pms_labels.iter().any(|text| text == "9"));
        assert!(!pms_labels.iter().any(|text| text == "S"));
        let mut scratch_pattern: Vec<u8> = (0..16).collect();
        scratch_pattern.swap(0, 1);
        snapshot.battle = Some(Arc::new(SelectBattleSnapshot {
            arrange: Some("RANDOM-EX".into()),
            lane_shuffle_pattern: scratch_pattern,
            ..single.clone()
        }));
        assert!(labels(&mut renderer, &snapshot).iter().any(|text| text == "S"));
        snapshot.battle = Some(Arc::new(single));
        assert_eq!(
            labels(&mut renderer, &snapshot),
            sp_labels,
            "DP -> SP must restore the left panel"
        );
        snapshot.battle = None;
        assert_eq!(
            labels(&mut renderer, &snapshot),
            original,
            "closing must restore both native panels"
        );
        assert_eq!(native_frame(&renderer, 709.0, 664.0, 413.0, 150.0), 1);
        assert_eq!(native_frame(&renderer, 302.0, 663.0, 412.0, 284.0), 1);
    }
}

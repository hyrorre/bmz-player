use super::*;
use bmz_core::lane::KeyMode;
use bmz_render::assets::RgbaImageAsset;
use bmz_render::plan::TextureId;
use bmz_render::scene::{SelectBattleSnapshot, SelectRivalSnapshot, SelectRowKind};

fn select_snapshot() -> SelectSnapshot {
    SelectSnapshot {
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
    }
}

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

/// The native pixels each panel pixel receives from the frame texture, nearest-sampled.
fn frame_pixels(
    renderer: &Renderer,
    texture_id: TextureId,
    native: &RgbaImageAsset,
    (left, top, w, h): (f32, f32, usize, usize),
) -> Vec<Vec<Vec<[u8; 4]>>> {
    let mut pixels = vec![vec![Vec::new(); w]; h];
    for command in &renderer.last_plan().unwrap().commands {
        let DrawCommand::Image { texture, rect, uv, .. } = command else { continue };
        if *texture != texture_id {
            continue;
        }
        let (x0, y0) = (rect.x * 1920.0 - left, rect.y * 1080.0 - top);
        let (dw, dh) = (rect.width * 1920.0, rect.height * 1080.0);
        for y in y0.round() as i32..(y0 + dh).round() as i32 {
            for x in x0.round() as i32..(x0 + dw).round() as i32 {
                let (Ok(px), Ok(py)) = (usize::try_from(x), usize::try_from(y)) else { continue };
                if px >= w || py >= h {
                    continue;
                }
                let u = uv.x + (x as f32 + 0.5 - x0) / dw * uv.width;
                let v = uv.y + (y as f32 + 0.5 - y0) / dh * uv.height;
                let sx = (u * native.width as f32).floor() as usize;
                let sy = (v * native.height as f32).floor() as usize;
                let offset = (sy * native.width as usize + sx) * 4;
                pixels[py][px].push(native.pixels[offset..offset + 4].try_into().unwrap());
            }
        }
    }
    pixels
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
        let mut snapshot = select_snapshot();
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

#[test]
fn luxe_battle_info_frames_keep_the_native_edge_glow_continuous() {
    let path = test_app_paths().resource_dir.join("skins/Luxez-Flat/music_select.luaskin");
    assert!(path.exists(), "initialize Luxez-Flat to verify its actual assets");
    let decoded = decode_beatoraja_skin_with_options_and_runtime_state(
        &path,
        SkinKind::Select,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &bmz_skin::LuaLoadRuntimeState::default(),
    )
    .unwrap();
    let native = |id: &str| {
        let source = decoded.sources.iter().find(|source| source.source_id == id).unwrap();
        (TextureId(source.texture.0), source.asset.clone().unwrap())
    };
    let panels = [
        (KeyMode::K7, native("luxe_battle_rival"), (709.0, 664.0, 413, 150)),
        (KeyMode::K14, native("luxe_battle_detail"), (302.0, 663.0, 412, 284)),
    ];
    let mut renderer = Renderer::default();
    install_decoded_skin(&mut renderer, decoded, bmz_render::skin::default_skin_manifest())
        .unwrap();
    for (key_mode, (texture, native), panel) in panels {
        let snapshot = SelectSnapshot {
            battle: Some(Arc::new(SelectBattleSnapshot {
                key_mode,
                arrange: Some("RANDOM".into()),
                ..Default::default()
            })),
            ..select_snapshot()
        };
        labels(&mut renderer, &snapshot);
        let pixels = frame_pixels(&renderer, texture, &native, panel);
        let (w, h) = (panel.2, panel.3);
        let at = |x: usize, y: usize| {
            let drawn = &pixels[y][x];
            // Overlapping translucent strips would darken; gaps would show through.
            assert_eq!(drawn.len(), 1, "{key_mode:?} ({x}, {y}) must be drawn exactly once");
            drawn[0]
        };
        let native_at = |x: usize, y: usize| {
            let offset = (y * native.width as usize + x) * 4;
            <[u8; 4]>::try_from(&native.pixels[offset..offset + 4]).unwrap()
        };
        // The DP panel is 1px narrower than its artwork; its right side keeps the artwork's.
        let shift = native.width as usize - w;
        for y in 0..h {
            let header = y < 38;
            let right = if header { w - 32 } else { w - 36 };
            for x in 0..w {
                let native_x = if x >= right { x + shift } else { x };
                // The rims, the dot's glow, the right glow and the bottom corners stay
                // unscaled, so the glow runs into the corners without a step.
                if x < 11 || x >= right || ((header || y >= h - 19) && x < 36) {
                    assert_eq!(at(x, y), native_at(native_x, y), "{key_mode:?} ({x}, {y})");
                } else if x < 36 {
                    // The labels tint the left glow, so it mirrors the clean right glow.
                    assert_eq!(at(x, y), at(w - 1 - x, y), "{key_mode:?} ({x}, {y})");
                } else {
                    at(x, y);
                }
            }
        }
    }
}

#[test]
fn luxe_battle_info_draws_lanes_8_and_9_like_the_numbered_tiles() {
    let path = test_app_paths().resource_dir.join("skins/Luxez-Flat/music_select.luaskin");
    assert!(path.exists(), "initialize Luxez-Flat to verify its actual assets");
    let decoded = decode_beatoraja_skin_with_options_and_runtime_state(
        &path,
        SkinKind::Select,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &bmz_skin::LuaLoadRuntimeState::default(),
    )
    .unwrap();
    let keys = TextureId(
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
    let snapshot = SelectSnapshot {
        battle: Some(Arc::new(SelectBattleSnapshot {
            key_mode: KeyMode::K9,
            arrange: Some("RANDOM".into()),
            lane_shuffle_pattern: vec![0, 9, 5, 3, 7, 1, 8, 4, 2, 6, 10, 11, 12, 13, 14, 15],
            ..Default::default()
        })),
        ..select_snapshot()
    };
    labels(&mut renderer, &snapshot);
    let commands = &renderer.last_plan().unwrap().commands;
    // The stretched plain face of each blank tile: tile 1 (light) at x=5, tile 2 (dark) at x=30.
    let mut faces: Vec<(f32, f32)> = commands
        .iter()
        .filter_map(|command| match command {
            DrawCommand::Image { texture, rect, uv, .. }
                if *texture == keys && (rect.width * 1920.0 - 14.0).abs() < 0.01 =>
            {
                Some((rect.x, (uv.x * 300.0).round()))
            }
            _ => None,
        })
        .collect();
    faces.sort_by(|a, b| a.0.total_cmp(&b.0));
    // Lane 9 comes first and lane 8 sixth: odd is light, even is dark.
    assert_eq!(faces.iter().map(|face| face.1).collect::<Vec<_>>(), [5.0, 30.0]);
    for command in commands {
        if let DrawCommand::Text { text, style, .. } = command
            && (text == "8" || text == "9")
        {
            assert!(style.font_id.as_deref().is_some_and(|id| id.ends_with("luxe_battle_digits")));
            assert_eq!(
                (style.color.r, (style.color.g * 255.0).round(), style.color.b),
                (1.0, 168.0, 0.0),
                "{text}"
            );
            // A thin 1 px edge, like the tiles' digits.
            let edge = style.outline.map(|outline| (outline.width * 1080.0).round());
            assert_eq!(edge, Some(1.0), "{text}");
        }
    }
}

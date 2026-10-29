use super::*;

#[test]
fn mz_select_result_uses_runtime_decisions_and_draws_note_graphs() {
    let skin_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../data/skins/mz-select/result/result.luaskin");
    if !skin_path.is_file() {
        return;
    }
    let runtime_state = LuaLoadRuntimeState {
        number_values: BTreeMap::from([
            (74, 100),
            (153, 354),
            (370, 7),
            (371, 5),
            (374, -12),
            (375, -50),
            (410, 20),
            (411, 10),
            (412, 8),
            (413, 4),
            (414, 3),
            (415, 2),
            (416, 1),
            (417, 1),
            (418, 1),
            (419, 1),
            (421, 1),
            (422, 1),
        ]),
        option_values: BTreeMap::from([(51, true)]),
        ..LuaLoadRuntimeState::default()
    };
    let decoded = decode_beatoraja_skin_with_options_and_runtime_state(
        &skin_path,
        SkinKind::Result,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &runtime_state,
    )
    .expect("decode mz-select result skin with runtime result values");

    assert!(decoded.document.slider.iter().any(|slider| slider.slider_type == 8));

    let timing = decoded
        .document
        .text
        .iter()
        .find(|text| text.id == "timing")
        .expect("mz-select timing text");
    assert_eq!(timing.constant_text, "平均12.5ms遅い");
    for (id, label, draw) in [
        ("arrange_f_random", "F-RANDOM", "event_index(344) == 10"),
        ("arrange_mf_random", "MF-RANDOM", "event_index(344) == 11"),
        ("arrange_f_random_2p", "2P F-RANDOM", "event_index(345) == 10"),
        ("arrange_mf_random_2p", "2P MF-RANDOM", "event_index(345) == 11"),
    ] {
        assert!(
            decoded.document.text.iter().any(|text| text.id == id && text.constant_text == label),
            "mz-select result should decode {id} text"
        );
        assert!(decoded.document.destination.iter().any(|entry| matches!(
            entry,
            DestinationListEntry::Single(destination)
                if destination.id == id && destination.draw == draw
        )));
    }
    let clear_state = decoded
        .document
        .image
        .iter()
        .find(|image| image.id == "clear_state")
        .expect("mz-select clear update image");
    assert_eq!(clear_state.x, 0, "current clear above previous should use UP image");
    assert!(decoded.document.destination.iter().any(|entry| matches!(
        entry,
        DestinationListEntry::Single(destination) if destination.id == "win"
    )));
    assert!(!decoded.document.destination.iter().any(|entry| matches!(
        entry,
        DestinationListEntry::Single(destination) if destination.id == "draw"
    )));

    let document_textures = decoded.sources.iter().map(|source| SkinDocumentTexture {
        source_id: source.source_id.clone(),
        texture: source.texture,
        source_size: source.size,
    });
    let context = SkinContext::from_manifest_and_document(
        bmz_render::skin::default_skin_manifest(),
        decoded.document,
        document_textures,
    );
    let graph = std::sync::Arc::new(bmz_render::snapshot::ResultGraphSnapshot {
        judge_graph_buckets: vec![
            bmz_render::snapshot::ResultJudgeGraphBucket { values: [0, 10, 5, 2, 1, 1] },
            bmz_render::snapshot::ResultJudgeGraphBucket { values: [0, 8, 4, 2, 1, 0] },
        ],
        early_late_graph_buckets: vec![
            bmz_render::snapshot::ResultEarlyLateGraphBucket {
                values: [0, 10, 4, 2, 1, 0, 3, 2, 1, 0],
            },
            bmz_render::snapshot::ResultEarlyLateGraphBucket {
                values: [0, 8, 3, 2, 1, 0, 4, 2, 1, 0],
            },
        ],
        judge_graph_density: vec![12, 18],
        ..bmz_render::snapshot::ResultGraphSnapshot::default()
    });
    let state = bmz_render::skin::SkinDrawState {
        elapsed_ms: 500,
        result_failed: Some(false),
        total_notes: 100,
        key_mode: KeyMode::K7,
        ..bmz_render::skin::SkinDrawState::default()
    };
    let items = context.static_document_items_for_result_state_and_text(
        &graph,
        &state,
        &bmz_render::skin::SkinTextState::default(),
    );
    let populated_batches = items
        .iter()
        .filter(|item| {
            matches!(
                item,
                bmz_render::skin::SkinRenderItem::RectBatch { rects, .. } if !rects.is_empty()
            )
        })
        .count();
    assert_eq!(populated_batches, 2, "JUDGE and FAST/SLOW graph batches should render");
    assert!(
        !items.iter().any(|item| matches!(
            item,
            bmz_render::skin::SkinRenderItem::Rect {
                color,
                blend: bmz_render::skin::BlendMode::Add,
                ..
            } if color.r == 0.0 && color.g == 0.0 && color.b == 0.0
        )),
        "additive black gauge backgrounds must not cover the two note graphs"
    );
}

#[test]
fn ecfn_play7_judge_combo_x_matches_beatoraja_layout_when_available() {
    use std::collections::HashMap;

    use bmz_render::skin::{SkinDocumentTexture, SkinImageSize, SkinRenderItem, SkinTextureId};

    let skin_path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data/skins/ECFN/play/play7.luaskin");
    if !skin_path.is_file() {
        return;
    }
    let decoded = decode_beatoraja_skin(&skin_path, SkinKind::Play).unwrap();
    let mock_texture = SkinDocumentTexture {
        source_id: "mock".to_string(),
        texture: SkinTextureId(1),
        source_size: SkinImageSize { width: 1920.0, height: 1080.0 },
    };
    let sources: HashMap<String, SkinDocumentTexture> = decoded
        .document
        .source
        .iter()
        .map(|source| (source.id.clone(), mock_texture.clone()))
        .chain(decoded.document.value.iter().map(|value| (value.src.clone(), mock_texture.clone())))
        .chain(decoded.document.image.iter().map(|image| (image.src.clone(), mock_texture.clone())))
        .collect();
    let items =
        decoded.document.judge_render_items("PGREAT", 42, 100, &sources).expect("judge items");
    let digit_xs: Vec<f32> = items
        .iter()
        .skip(1)
        .filter_map(|item| match item {
            SkinRenderItem::Image { rect, .. } => Some(rect.x),
            _ => None,
        })
        .collect();
    assert_eq!(digit_xs.len(), 2);
    let expected_first = 334.0 / 1920.0;
    let expected_second = 392.0 / 1920.0;
    assert!(
        (digit_xs[0] - expected_first).abs() < 0.001,
        "first digit x={} expected {expected_first}",
        digit_xs[0]
    );
    assert!(
        (digit_xs[1] - expected_second).abs() < 0.001,
        "second digit x={} expected {expected_second}",
        digit_xs[1]
    );
}

#[test]
fn ecfn_play7_pre_notes_judge_line_renders_in_front_when_available() {
    use std::collections::HashMap;

    use bmz_render::skin::{
        SkinDocumentTexture, SkinDrawState, SkinImageSize, SkinRenderItem, SkinTextState,
    };

    let skin_path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data/skins/ECFN/play/play7.luaskin");
    if !skin_path.is_file() {
        return;
    }
    let decoded = decode_beatoraja_skin(&skin_path, SkinKind::Play).unwrap();
    let image_15 = decoded
        .document
        .image
        .iter()
        .find(|image| image.id == "15")
        .expect("ECFN id=15 image should decode");
    assert_eq!((image_15.src.as_str(), image_15.x, image_15.y), ("0", 16, 0));
    let image_15_map = decoded.document.image_map();
    let mapped_15 = image_15_map.get("15").expect("ECFN id=15 image should map");
    assert_eq!((mapped_15.src.as_str(), mapped_15.x, mapped_15.y), ("0", 16, 0));
    let system_texture = decoded
        .sources
        .iter()
        .find(|source| source.source_id == "0")
        .map(|source| source.texture)
        .expect("ECFN source 0 should decode");
    let system_size = decoded
        .sources
        .iter()
        .find(|source| source.source_id == "0")
        .map(|source| SkinImageSize { width: source.size.width, height: source.size.height })
        .expect("ECFN source 0 should decode");
    let sources: HashMap<String, SkinDocumentTexture> = decoded
        .sources
        .iter()
        .map(|source| {
            (
                source.source_id.clone(),
                SkinDocumentTexture {
                    source_id: source.source_id.clone(),
                    texture: source.texture,
                    source_size: SkinImageSize {
                        width: source.size.width,
                        height: source.size.height,
                    },
                },
            )
        })
        .collect();

    let (behind, front, _) = decoded.document.static_render_items_split(
        &sources,
        &SkinDrawState::default(),
        &SkinTextState::default(),
    );

    assert!(
        behind.iter().all(|item| !matches!(
            item,
            SkinRenderItem::Image {
                texture,
                rect,
                ..
            } if *texture == system_texture
                && (rect.y - 715.0 / 1080.0).abs() < 0.001
                && (rect.height - 8.0 / 1080.0).abs() < 0.001
        )),
        "ECFN judge line should not remain behind notes"
    );
    assert!(
        front.iter().any(|item| matches!(
            item,
            SkinRenderItem::Image {
                texture,
                rect,
                uv,
                ..
            } if *texture == system_texture
                && (rect.y - 715.0 / 1080.0).abs() < 0.001
                && (rect.height - 8.0 / 1080.0).abs() < 0.001
                && (uv.x - 16.0 / system_size.width).abs() < 0.001
                && uv.y.abs() < 0.001
        )),
        "expected ECFN id=15 judge line in front items; got {front:?}"
    );
}

#[test]
fn ecfn_play14_judge1_combo_is_right_of_judge_with_score_graph_when_available() {
    use std::collections::HashMap;

    use bmz_core::lane::Lane;
    use bmz_render::skin::{
        MAX_JUDGE_REGIONS, SkinDocumentTexture, SkinDrawState, SkinImageSize, SkinRenderItem,
        SkinTextureId,
    };

    let skin_path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data/skins/ECFN/play/play14.luaskin");
    if !skin_path.is_file() {
        return;
    }
    let options = BTreeMap::from([("スコアグラフ".to_string(), "On".to_string())]);
    let decoded =
        decode_beatoraja_skin_with_options(&skin_path, SkinKind::Play, &options, &BTreeMap::new())
            .expect("ECFN play14 should decode when its score graph layout is enabled");
    let judge0 = decoded.document.judge.iter().find(|judge| judge.id == "judge").expect("judge");
    let judge1 = decoded.document.judge.iter().find(|judge| judge.id == "judge1").expect("judge1");
    assert_eq!(judge0.index, 0);
    assert_eq!(judge1.index, 1);

    let mock_texture = SkinDocumentTexture {
        source_id: "mock".to_string(),
        texture: SkinTextureId(1),
        source_size: SkinImageSize { width: 1920.0, height: 1080.0 },
    };
    let sources: HashMap<String, SkinDocumentTexture> = decoded
        .document
        .source
        .iter()
        .map(|source| (source.id.clone(), mock_texture.clone()))
        .chain(decoded.document.value.iter().map(|value| (value.src.clone(), mock_texture.clone())))
        .chain(decoded.document.image.iter().map(|image| (image.src.clone(), mock_texture.clone())))
        .collect();

    let mut judge_ms = [None; MAX_JUDGE_REGIONS];
    let mut judge_index = [None; MAX_JUDGE_REGIONS];
    judge_ms[0] = Some(100);
    judge_ms[1] = Some(100);
    judge_index[0] = Some(0);
    judge_index[1] = Some(0);
    let state = SkinDrawState { judge_ms, judge_index, combo: 42, ..SkinDrawState::default() };

    let left_items = decoded
        .document
        .judge_render_items_for_def(judge0, 0, 42, 100, &sources, &state)
        .expect("left judge");
    let right_items = decoded
        .document
        .judge_render_items_for_def(judge1, 0, 42, 100, &sources, &state)
        .expect("right judge");
    let left_digit = left_items
        .iter()
        .skip(1)
        .find_map(|item| match item {
            SkinRenderItem::Image { rect, .. } => Some(rect.x),
            _ => None,
        })
        .expect("left combo digit");
    let right_digit = right_items
        .iter()
        .skip(1)
        .find_map(|item| match item {
            SkinRenderItem::Image { rect, .. } => Some(rect.x),
            _ => None,
        })
        .expect("right combo digit");
    assert!(
        right_digit > left_digit,
        "judge1 digit x={right_digit} should be right of judge x={left_digit}"
    );

    let region = bmz_render::skin::lane_judge_region(
        Lane::Key8,
        bmz_core::lane::KeyMode::K14,
        decoded.document.judge_region_count(),
    );
    assert_eq!(region, 1);
}

#[test]
fn starseeker_play_lua_skin_can_be_decoded_when_available() {
    let skin_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../data/skins/Starseeker/play/play7.luaskin");
    if !skin_path.is_file() {
        return;
    }

    let decoded = decode_beatoraja_skin(&skin_path, SkinKind::Play).unwrap();

    assert!(!decoded.document.destination.is_empty());
}

#[test]
fn starseeker_metallic_blue_judge_parts_are_loaded_when_available() {
    let skin_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../data/skins/Starseeker/play/play7.luaskin");
    if !skin_path.is_file() {
        return;
    }

    let files =
        BTreeMap::from([("判定文字".to_string(), "custom/judge/metallic_blue".to_string())]);
    let decoded =
        decode_beatoraja_skin_with_options(&skin_path, SkinKind::Play, &BTreeMap::new(), &files)
            .expect("decode Starseeker metallic_blue judge skin");

    assert!(decoded.document.source.iter().any(|source| source.id == "judge_main"));
    assert!(decoded.sources.iter().any(|source| source.source_id == "judge_main"));
    assert!(decoded.document.image.iter().any(|image| image.id == "judgef-pg"));
    assert!(decoded.document.value.iter().any(|value| value.id == "judgen-pg"));
    assert!(decoded.document.judge.iter().any(|judge| judge.id == "judge"));
}

#[test]
fn starseeker_result_lua_skin_renders_stat_details_when_available() {
    let skin_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../data/skins/Starseeker/result/result.luaskin");
    if !skin_path.is_file() {
        return;
    }

    let options = BTreeMap::from([
        ("F/Sリスト".to_string(), "Default".to_string()),
        ("逆サイド詳細フレーム".to_string(), "ON".to_string()),
        ("プレーサイド".to_string(), "1P".to_string()),
    ]);
    let files = BTreeMap::from([
        ("使用テーマ".to_string(), "Theme/starseeker".to_string()),
        ("フォント".to_string(), "_font/TYPE-M".to_string()),
        ("シャッター".to_string(), "Shutter/TYPE-M".to_string()),
    ]);
    let decoded =
        decode_beatoraja_skin_with_options(&skin_path, SkinKind::Result, &options, &files)
            .expect("decode starseeker result skin");
    let destinations = decoded.document.all_destinations(&[]);
    let slow_judgement_timing = destinations
        .iter()
        .find(|destination| destination.id == "judge_adv_s")
        .expect("starseeker result should keep SLOW timing label destination");
    let fast_judgement_timing = destinations
        .iter()
        .find(|destination| destination.id == "judge_adv_f")
        .expect("starseeker result should keep FAST timing label destination");
    assert_eq!(slow_judgement_timing.draw, "number(374) < 0 or number(375) < 0");
    assert_eq!(fast_judgement_timing.draw, "number(374) > 0 or number(375) > 0");
    assert!(
        decoded.document.all_destinations(&[]).iter().any(|destination| {
            matches!(
                destination.id.as_str(),
                "judge_detail" | "judgegraph" | "fsgraph" | "timingGraph"
            )
        }),
        "starseeker result stat destinations should survive lua conversion"
    );
    assert!(
        decoded.document.source.iter().any(|source| source.id == "jud_detail_main"),
        "starseeker result document should keep jud_detail_main source; sources: {:?}",
        decoded.document.source.iter().map(|source| source.id.as_str()).collect::<Vec<_>>()
    );
    let stat_texture = decoded
        .sources
        .iter()
        .find(|source| source.source_id == "jud_detail_main")
        .map(|source| source.texture)
        .expect("starseeker result should load jud_detail_main source");
    let document_textures =
        decoded.sources.iter().map(|source| bmz_render::skin::SkinDocumentTexture {
            source_id: source.source_id.clone(),
            texture: source.texture,
            source_size: bmz_render::skin::SkinImageSize {
                width: source.size.width,
                height: source.size.height,
            },
        });
    let context = bmz_render::skin::SkinContext::from_manifest_and_document(
        bmz_render::skin::default_skin_manifest(),
        decoded.document,
        document_textures,
    );
    let bmz_render::scene::AppSceneSnapshot::Result(mut snapshot) =
        bmz_render::sample::sample_result_scene()
    else {
        panic!("sample result scene");
    };
    snapshot.elapsed_time = bmz_core::time::TimeUs(1_000_000);
    snapshot.judge_counts = bmz_render::snapshot::DisplayJudgeCounts {
        pgreat: 120,
        great: 40,
        good: 12,
        bad: 4,
        poor: 3,
        empty_poor: 2,
    };
    snapshot.fast_slow_counts = bmz_render::snapshot::FastSlowJudgeCounts {
        fast_pgreat: 80,
        slow_pgreat: 40,
        fast_great: 12,
        slow_great: 28,
        fast_good: 4,
        slow_good: 8,
        fast_bad: 1,
        slow_bad: 3,
        fast_poor: 1,
        slow_poor: 2,
        fast_empty_poor: 1,
        slow_empty_poor: 1,
    };
    let graph = std::sync::Arc::make_mut(&mut snapshot.graph);
    graph.judge_graph_density = vec![1, 3, 2, 4];
    graph.timing_points = vec![
        bmz_render::snapshot::ResultTimingPoint {
            time_ms: 100,
            delta_us: -12_000,
            judge: bmz_core::judge::Judge::Great,
        },
        bmz_render::snapshot::ResultTimingPoint {
            time_ms: 200,
            delta_us: 8_000,
            judge: bmz_core::judge::Judge::PGreat,
        },
    ];

    let plan = bmz_render::plan::DrawPlan::from_scene_with_skin(
        &bmz_render::scene::AppSceneSnapshot::Result(snapshot),
        &context,
        &mut bmz_render::skin::DynamicTimerRuntime::default(),
    );

    assert!(plan.commands.iter().any(|command| matches!(
        command,
        bmz_render::plan::DrawCommand::Image { texture, .. }
            if *texture == bmz_render::plan::TextureId(stat_texture.0)
    )));
    assert!(plan.commands.iter().any(|command| matches!(
        command,
        bmz_render::plan::DrawCommand::Rect { rect, .. }
            if rect.x > 0.70 && rect.y > 0.20 && rect.y < 0.55
    )));
}

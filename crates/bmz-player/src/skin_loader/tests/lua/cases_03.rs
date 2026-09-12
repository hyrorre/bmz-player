use super::*;

#[test]
fn milliondollar_result_runtime_events_toggle_observe_timers_when_available() {
    let skin_path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data/skins/MILLIONDOLLAR/result.luaskin");
    if !skin_path.is_file() {
        return;
    }

    let decoded = decode_beatoraja_skin_with_options(
        &skin_path,
        SkinKind::Result,
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .expect("decode MILLIONDOLLAR result skin");
    let cim_sources = decoded
        .sources
        .iter()
        .filter(|source| {
            source
                .path
                .extension()
                .and_then(|extension| extension.to_str())
                .is_some_and(|extension| extension.eq_ignore_ascii_case("cim"))
        })
        .collect::<Vec<_>>();
    assert_eq!(cim_sources.len(), 11, "all MILLIONDOLLAR CIM atlases must decode");
    assert!(
        cim_sources.iter().all(|source| source.asset.is_some()),
        "MILLIONDOLLAR CIM atlases must provide RGBA assets before GPU upload"
    );
    let document = &decoded.document;
    let circle_destinations = document
        .destination
        .iter()
        .filter_map(|entry| match entry {
            DestinationListEntry::Single(destination)
                if destination.id == "Graph_Circle_Meter"
                    || destination.id == "Graph_Circle_Frame" =>
            {
                Some(destination)
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(circle_destinations.len(), 724);
    let circle_timers = circle_destinations
        .iter()
        .filter_map(|destination| destination.timer)
        .collect::<BTreeSet<_>>();
    assert_eq!(circle_timers.len(), 1, "the shared circle visibility edge needs one timer");
    assert!(circle_timers.iter().all(|timer| {
        ((*timer - bmz_render::skin::SKIN_DYNAMIC_TIMER_BASE) as usize)
            < bmz_render::skin::SKIN_DYNAMIC_TIMER_COUNT
    }));

    let source_12 = decoded
        .sources
        .iter()
        .find(|source| source.source_id == "12")
        .expect("MILLIONDOLLAR parts atlas");
    let sources = decoded
        .sources
        .iter()
        .map(|source| {
            (
                source.source_id.clone(),
                SkinDocumentTexture {
                    source_id: source.source_id.clone(),
                    texture: source.texture,
                    source_size: source.size,
                },
            )
        })
        .collect::<HashMap<_, _>>();
    let mut circle_runtime = DynamicTimerRuntime::default();
    circle_runtime.reset_for_document(Some(document));
    let mut circle_state = SkinDrawState::default();
    circle_runtime.advance(document, &mut circle_state, 100);
    let circle_items =
        document.static_render_items(&sources, &circle_state, &SkinTextState::default());
    let rendered_segments = circle_items
        .iter()
        .filter(|item| {
            matches!(
                item,
                SkinRenderItem::RotatedImage { texture, .. }
                    if *texture == source_12.texture
            )
        })
        .count();
    assert!(
        rendered_segments >= 700,
        "MILLIONDOLLAR circle graph segments must render, got {rendered_segments}"
    );
    let circle_angles = circle_items
        .iter()
        .filter_map(|item| match item {
            SkinRenderItem::RotatedImage { texture, angle_deg, center, .. }
                if *texture == source_12.texture =>
            {
                assert!((center.x - 0.5).abs() < f32::EPSILON);
                assert!((center.y - 0.5).abs() < f32::EPSILON);
                Some(*angle_deg)
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    assert!(circle_angles.iter().all(|angle| *angle >= 0.0));
    assert!(circle_angles.iter().any(|angle| *angle >= 359.0));

    let event = document.runtime_events.first().expect("runtime toggle event");
    let initial_true = event
        .toggle_flags
        .iter()
        .find(|flag_id| {
            document.runtime_flags.iter().any(|flag| flag.id == **flag_id && flag.initial)
        })
        .copied()
        .expect("initially visible flag");
    let initial_false = event
        .toggle_flags
        .iter()
        .find(|flag_id| {
            document.runtime_flags.iter().any(|flag| flag.id == **flag_id && !flag.initial)
        })
        .copied()
        .expect("initially hidden flag");
    let timer_index = |flag_id: i32| {
        let observe = format!("runtime_flag({flag_id})");
        let timer = document
            .dynamic_timers
            .iter()
            .find(|timer| timer.observe == observe)
            .expect("timer observing runtime flag");
        usize::try_from(timer.id - bmz_render::skin::SKIN_DYNAMIC_TIMER_BASE).unwrap()
    };
    let true_timer = timer_index(initial_true);
    let false_timer = timer_index(initial_false);
    let mut runtime = DynamicTimerRuntime::default();
    runtime.reset_for_document(Some(document));
    let mut state = SkinDrawState::default();

    runtime.advance(document, &mut state, 100);
    assert_eq!(state.dynamic_timer_ms[true_timer], Some(0));
    assert_eq!(state.dynamic_timer_ms[false_timer], None);

    assert!(runtime.dispatch_runtime_event(document, event.id));
    runtime.advance(document, &mut state, 150);
    assert_eq!(state.dynamic_timer_ms[true_timer], None);
    assert_eq!(state.dynamic_timer_ms[false_timer], Some(0));

    assert!(runtime.dispatch_runtime_event(document, event.id));
    runtime.advance(document, &mut state, 200);
    assert_eq!(state.dynamic_timer_ms[true_timer], Some(0));
    assert_eq!(state.dynamic_timer_ms[false_timer], None);
}

#[test]
fn milliondollar_result_song_info_uses_runtime_judge_rank_and_ln_mode_when_available() {
    let skin_path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data/skins/MILLIONDOLLAR/result.luaskin");
    if !skin_path.is_file() {
        return;
    }

    let decoded = decode_beatoraja_skin_with_options_and_runtime_state(
        &skin_path,
        SkinKind::Result,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &LuaLoadRuntimeState {
            text_values: BTreeMap::from([(1, "RANK AAA".to_string())]),
            option_values: BTreeMap::from([
                (180, false),
                (181, true),
                (182, false),
                (183, false),
                (184, false),
            ]),
            event_index_values: BTreeMap::from([(42, 2), (43, 0), (54, 0), (308, 2)]),
            ..LuaLoadRuntimeState::default()
        },
    )
    .expect("decode MILLIONDOLLAR result skin with chart metadata");

    let judge_rank = decoded
        .document
        .image
        .iter()
        .find(|image| image.id == "Parts_Text_Info_Judgerank")
        .expect("MILLIONDOLLAR judge-rank label");
    let ln_type = decoded
        .document
        .image
        .iter()
        .find(|image| image.id == "Parts_Text_Info_Lntype")
        .expect("MILLIONDOLLAR LN-type label");
    let arrange = decoded
        .document
        .image
        .iter()
        .find(|image| image.id == "Parts_Texts_Useoption_SP")
        .expect("MILLIONDOLLAR SP arrange label");
    let target_rank = decoded
        .document
        .image
        .iter()
        .find(|image| image.id == "Parts_Texts_Target_Rank")
        .expect("MILLIONDOLLAR fixed target rank label");
    assert_eq!(judge_rank.y, 310, "HARD must select atlas row 3");
    assert_eq!(ln_type.y, 291, "HCN must select atlas row 2");
    assert_eq!(arrange.y, 48, "RANDOM must select atlas row 2");
    assert_eq!(target_rank.y, 16, "RANK AAA must select the AAA target row");
    assert!(decoded.document.destination.iter().any(|entry| matches!(
        entry,
        DestinationListEntry::Single(destination)
            if destination.id == "Parts_Texts_Useoption_SP"
    )));
}

#[test]
fn milliondollar_result_uses_integer_only_gauge_layout_at_one_hundred_percent_when_available() {
    let skin_path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data/skins/MILLIONDOLLAR/result.luaskin");
    if !skin_path.is_file() {
        return;
    }

    let decoded = decode_beatoraja_skin_with_options_and_runtime_state(
        &skin_path,
        SkinKind::Result,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &LuaLoadRuntimeState {
            number_values: BTreeMap::from([(107, 100)]),
            ..LuaLoadRuntimeState::default()
        },
    )
    .expect("decode MILLIONDOLLAR result skin with full gauge");

    let draw_for = |id: &str| {
        decoded.document.destination.iter().find_map(|entry| match entry {
            DestinationListEntry::Single(destination) if destination.id == id => {
                Some(destination.draw.as_str())
            }
            _ => None,
        })
    };
    assert_eq!(draw_for("Number_Remaingauge_Max_1"), Some("number(107) == 100"));
    assert_eq!(draw_for("Number_Remaingauge_Max_00"), Some("number(107) == 100"));
    assert_eq!(draw_for("Number_Remaingauge_Normal"), Some("number(107) < 100"));
    assert_eq!(draw_for("Parts_Text_Remaingauge_Dot"), Some("number(107) < 100"));
    assert_eq!(draw_for("Number_Remaingauge_Afterdot"), Some("number(107) < 100"));
}

#[test]
fn milliondollar_result_rank_diff_uses_load_time_result_scores_when_available() {
    let skin_path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data/skins/MILLIONDOLLAR/result.luaskin");
    if !skin_path.is_file() {
        return;
    }

    let decoded = decode_beatoraja_skin_with_options_and_runtime_state(
        &skin_path,
        SkinKind::Result,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &LuaLoadRuntimeState {
            number_values: BTreeMap::from([
                (71, 2_877),
                (74, 1_550),
                (151, 2_756),
                (170, 3_021),
                (171, 2_877),
            ]),
            ..LuaLoadRuntimeState::default()
        },
    )
    .expect("decode MILLIONDOLLAR result skin with result scores");

    let best_rank = decoded
        .document
        .image
        .iter()
        .find(|image| image.id == "Parts_Rank_Middle_Best")
        .expect("MILLIONDOLLAR best DJ level");
    let next_rank = decoded
        .document
        .image
        .iter()
        .find(|image| image.id == "Parts_Rank_Nextrank")
        .expect("MILLIONDOLLAR next-rank label");
    let next_rank_diff = decoded
        .document
        .value
        .iter()
        .find(|value| value.id == "Number_Nextrank_Diff")
        .expect("MILLIONDOLLAR next-rank difference");

    assert_eq!(best_rank.y, 0, "3021/3100 must select the AAA row");
    assert_eq!(next_rank.x, 951, "positive rank difference must select the plus label");
    assert_eq!(next_rank.y, 18, "AAA+ must select the plus row");
    let callback_id = next_rank_diff
        .value_expr
        .strip_prefix("bmz:lua_value_callback:")
        .expect("next-rank difference should retain a Lua callback")
        .parse::<usize>()
        .expect("valid Lua callback ID");
    let mut lua_runtime = decoded.lua_runtime.expect("MILLIONDOLLAR value callback runtime");
    let state = SkinDrawState::default();
    let text_values = BTreeMap::new();
    let provider =
        RenderLuaMainState { state: &state, enabled_options: &[], text_values: &text_values };
    lua_runtime.begin_frame();
    assert_eq!(lua_runtime.evaluate_number(callback_id, &provider), Some(121.0));
}

#[test]
fn starseeker_result_misscount_diff_uses_runtime_number_color_block() {
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
    let runtime_state = LuaLoadRuntimeState {
        number_values: BTreeMap::from([(178, -1)]),
        text_values: BTreeMap::new(),
        option_values: BTreeMap::new(),
        ..LuaLoadRuntimeState::default()
    };
    let decoded = decode_beatoraja_skin_with_options_and_runtime_state(
        &skin_path,
        SkinKind::Result,
        &options,
        &files,
        &runtime_state,
    )
    .expect("decode starseeker result skin with misscount diff");

    let diff_misscount = decoded
        .document
        .value
        .iter()
        .find(|value| value.id == "Diff_Misscount")
        .expect("starseeker result should define Diff_Misscount");

    assert_eq!(diff_misscount.ref_id, 178);
    assert_eq!(diff_misscount.y, 345);
}

#[test]
fn rmz_play8_lua_skin_decodes_when_available() {
    let skin_path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data/skins/Rmz-skin/play8main.luaskin");
    if !skin_path.is_file() {
        return;
    }

    let decoded = decode_beatoraja_skin(&skin_path, SkinKind::Play).unwrap();

    assert_eq!(decoded.document.skin_type, 24);
    let note = decoded.document.note.as_ref().expect("play8 skin should define notes");
    assert_eq!(note.note.len(), 8);
    assert_eq!(note.dst.len(), 8);
}

#[test]
fn antique_play_lua_bakes_configured_keybeam_height_offset_when_available() {
    let skin_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../data/skins/mz-select/play/antique/system/play7main.luaskin");
    if !skin_path.is_file() {
        return;
    }

    let load = |height| {
        load_skin_document(
            &skin_path,
            SkinKind::Play,
            &BTreeMap::new(),
            &BTreeMap::new(),
            &LuaLoadRuntimeState {
                offset_values: BTreeMap::from([(
                    "キービームの長さ".to_string(),
                    bmz_skin::LuaSkinOffsetValue { h: height, ..Default::default() },
                )]),
                offset_id_values: BTreeMap::from([(
                    53,
                    bmz_skin::LuaSkinOffsetValue { h: height, ..Default::default() },
                )]),
                ..Default::default()
            },
            None,
        )
        .expect("decode Antique play skin")
        .document
    };
    let keybeam_height = |document: &SkinDocument| {
        document.destination.iter().find_map(|entry| match entry {
            DestinationListEntry::Single(destination)
                if destination.id == "imgset_keybeam1" && destination.timer == Some(101) =>
            {
                destination.dst.first().and_then(|entry| match entry {
                    bmz_render::skin::SkinDstEntry::Frame(frame) => frame.h,
                    bmz_render::skin::SkinDstEntry::Conditional { .. } => None,
                })
            }
            _ => None,
        })
    };

    assert_eq!(keybeam_height(&load(0)), Some(564));
    assert_eq!(keybeam_height(&load(37)), Some(601));
}

#[test]
fn antique_play_lua_places_split_fast_slow_beside_the_key_label_when_available() {
    let skin_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../data/skins/mz-select/play/antique/system/play7main.luaskin");
    if !skin_path.is_file() {
        return;
    }

    let load = |scratch: &str| {
        decode_beatoraja_skin_with_options(
            &skin_path,
            SkinKind::Play,
            &BTreeMap::from([
                ("スクラッチ".to_string(), scratch.to_string()),
                ("FAST/SLOW".to_string(), "SCRATCH/KEYS別 (BMZ)".to_string()),
            ]),
            &BTreeMap::new(),
        )
        .expect("decode Antique split FAST/SLOW skin")
    };
    let assert_destination = |document: &SkinDocument, id: &str, timer, option, x, width, alpha| {
        let destination = document
            .destination
            .iter()
            .find_map(|entry| match entry {
                DestinationListEntry::Single(destination)
                    if destination.id == id && destination.timer == Some(timer) =>
                {
                    Some(destination)
                }
                _ => None,
            })
            .unwrap_or_else(|| panic!("Antique destination {id} timer {timer}"));
        assert_eq!(destination.loop_time, Some(-1));
        assert_eq!(destination.op, vec![909, option]);
        assert!(matches!(
            destination.dst.first(),
            Some(bmz_render::skin::SkinDstEntry::Frame(frame))
                if frame.x == Some(x)
                    && frame.w == Some(width)
                    && frame.h == Some(20)
                    && frame.a == alpha
        ));
        assert!(matches!(
            destination.dst.last(),
            Some(bmz_render::skin::SkinDstEntry::Frame(frame))
                    if frame.time == Some(800)
        ));
    };

    let left = load("左");
    assert!(left.document.enabled_options().contains(&909));
    assert!(!left.document.enabled_options().contains(&908));
    assert!(left.document.property.iter().any(|property| {
        property.name == "FAST/SLOW"
            && property
                .item
                .iter()
                .any(|item| item.name == "SCRATCH/KEYS別 (BMZ)" && item.op == 909)
    }));
    let scratch_source = left
        .sources
        .iter()
        .find(|source| source.source_id == "src_judgedetail_scratch")
        .expect("Antique scratch FAST/SLOW source");
    assert_eq!((scratch_source.size.width, scratch_source.size.height), (108.0, 40.0));
    assert_destination(&left.document, "img_s_fast", 19010, 19030, 307, 108, None);
    assert_destination(&left.document, "img_s_slow", 19010, 19040, 307, 108, None);
    assert_destination(&left.document, "img_fast", 19011, 19031, 423, 72, None);
    assert_destination(&left.document, "img_slow", 19011, 19041, 423, 72, None);

    let right = load("右");
    assert_destination(&right.document, "img_fast", 19011, 19031, 263, 72, None);
    assert_destination(&right.document, "img_slow", 19011, 19041, 263, 72, None);
    assert_destination(&right.document, "img_s_fast", 19010, 19030, 343, 108, None);
    assert_destination(&right.document, "img_s_slow", 19010, 19040, 343, 108, None);
}

#[test]
fn simple_play_lua_bakes_configured_note_height_offset_when_available() {
    let skin_path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data/skins/simple-play/play7.luaskin");
    if !skin_path.is_file() {
        return;
    }

    let load_note_sizes = |height| {
        load_skin_document(
            &skin_path,
            SkinKind::Play,
            &BTreeMap::new(),
            &BTreeMap::new(),
            &LuaLoadRuntimeState {
                offset_values: BTreeMap::from([(
                    "ノーツオフセット Notes Offset".to_string(),
                    bmz_skin::LuaSkinOffsetValue { h: height, ..Default::default() },
                )]),
                ..Default::default()
            },
            None,
        )
        .expect("decode simple-play skin")
        .document
        .note
        .expect("simple-play note definition")
        .size
    };
    let baseline = load_note_sizes(0);
    let configured = load_note_sizes(7);

    assert_eq!(baseline.len(), configured.len());
    assert!(
        baseline.iter().zip(configured).all(|(before, after)| after == before + 7),
        "simple-play note heights did not receive the configured offset"
    );
}

#[test]
fn rmz_play7_keeps_runtime_stagefile_loading_destinations_when_available() {
    let skin_path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data/skins/Rmz-skin/play7main.luaskin");
    if !skin_path.is_file() {
        return;
    }

    let decoded = decode_beatoraja_skin(&skin_path, SkinKind::Play).unwrap();
    let destinations = decoded.document.all_destinations(&decoded.document.enabled_options());
    let stagefile_destinations =
        destinations.iter().filter(|destination| destination.id == "-100").collect::<Vec<_>>();

    assert!(stagefile_destinations.iter().any(|destination| {
        destination.timer.is_none() && destination.op.contains(&80) && destination.op.contains(&191)
    }));
    assert!(stagefile_destinations.iter().any(|destination| {
        destination.timer == Some(40)
            && destination.op.contains(&81)
            && destination.op.contains(&191)
    }));
}

#[test]
fn rmz_play7_lanecover_green_renders_green_number_when_available() {
    let skin_path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data/skins/Rmz-skin/play7main.luaskin");
    if !skin_path.is_file() {
        return;
    }

    let decoded = decode_beatoraja_skin(&skin_path, SkinKind::Play).unwrap();
    let lanecover_green_value = decoded
        .document
        .value
        .iter()
        .find(|value| value.id == "lanecover-green")
        .expect("Rmz lanecover green value should decode");
    assert_eq!(
        lanecover_green_value.value_expr, "0.6*number(312)",
        "decoded value: {lanecover_green_value:?}"
    );
    let source = decoded
        .sources
        .iter()
        .find(|source| source.source_id == "play_system_src")
        .expect("Rmz play system source should decode");
    let sources = decoded
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
        .collect::<std::collections::HashMap<_, _>>();
    let state = bmz_render::skin::SkinDrawState {
        elapsed_ms: 2_000,
        play_timer_ms: Some(2_000),
        total_duration_ms: 500,
        duration_green_ms: Some(300),
        lane_cover_changing: true,
        lanecover_enabled: true,
        ..Default::default()
    };

    let items = decoded.document.static_render_items(
        &sources,
        &state,
        &bmz_render::skin::SkinTextState::default(),
    );
    assert!(
            !items.iter().any(
                |item| matches!(item, bmz_render::skin::SkinRenderItem::Text { text, .. } if text == "FHS")
            ),
            "FHS mark should stay hidden while NHS is active"
        );
    let digit_width = 20.0;
    let source_candidates = items
        .iter()
        .filter_map(|item| {
            if let bmz_render::skin::SkinRenderItem::Image { texture, rect, uv, .. } = item
                && *texture == source.texture
            {
                Some((
                    (rect.x * 1920.0).round() as i32,
                    (rect.y * 1080.0).round() as i32,
                    (uv.x * source.size.width / digit_width).round() as i32,
                    (uv.y * source.size.height).round() as i32,
                ))
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    let mut digits = items
        .iter()
        .filter_map(|item| {
            if let bmz_render::skin::SkinRenderItem::Image { texture, rect, uv, .. } = item
                && *texture == source.texture
                && (rect.y * 1080.0 - 10.0).abs() < 2.0
                && (rect.x * 1920.0 - 849.0).abs() < 80.0
            {
                let digit = (uv.x * source.size.width / digit_width).round() as i32;
                Some(((rect.x * 1920.0).round() as i32, digit))
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    digits.sort_by_key(|(x, _)| *x);
    let digits = digits.into_iter().map(|(_, digit)| digit).collect::<Vec<_>>();

    assert_eq!(digits, vec![3, 0, 0], "source candidates: {source_candidates:?}");

    let fhs_state = bmz_render::skin::SkinDrawState { hispeed_mode_index: 1, ..state.clone() };
    let fhs_items = decoded.document.static_render_items(
        &sources,
        &fhs_state,
        &bmz_render::skin::SkinTextState::default(),
    );
    assert!(
            fhs_items.iter().any(
                |item| matches!(item, bmz_render::skin::SkinRenderItem::Text { text, .. } if text == "FHS")
            ),
            "FHS mark should render while FHS is active"
        );
}

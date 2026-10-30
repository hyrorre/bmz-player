use super::*;

#[test]
fn lr2_folder_distribution_uses_source_clock_and_row_major_animation_frames() {
    let mut document: SkinDocument = serde_json::from_value(serde_json::json!({
        "type":5,"w":100,"h":100,
        "graph":[{"id":"graph","src":"atlas","x":0,"y":0,"w":220,"h":20,"divx":22,"divy":2,"timer":11,"cycle":200,"lr2Distribution":true}],
        "songlist":{"id":"bars","liston":[{"id":"body","dst":[{"x":0,"y":40,"w":100,"h":20}]}],"graph":[{"id":"graph","blend":2,"filter":1,"dst":[{"x":0,"y":0,"w":80,"h":10}]}]},
        "destination":[{"id":"bars"}]
    })).unwrap();
    let sources = HashMap::from([(
        "atlas".into(),
        SkinDocumentTexture {
            source_id: "atlas".into(),
            texture: SkinTextureId(1),
            source_size: SkinImageSize { width: 220.0, height: 20.0 },
        },
    )]);
    let mut counts = [0; 11];
    counts[0] = 1;
    counts[10] = 3;
    let mut snapshot = SelectSnapshot {
        time: TimeUs(175_000),
        selection_time: TimeUs(75_000),
        rows: vec![SelectRowSnapshot {
            is_folder: true,
            kind: SelectRowKind::Folder,
            folder_lamp_counts: counts,
            ..Default::default()
        }],
        ..Default::default()
    };
    for (time, selection_time, x, y) in
        [(175_000, 75_000, 21.0 / 22.0, 0.0), (225_000, 125_000, 10.0 / 22.0, 0.5)]
    {
        snapshot.time = TimeUs(time);
        snapshot.selection_time = TimeUs(selection_time);
        let items = document.select_render_items(&sources, &snapshot);
        assert_eq!(items.len(), 2, "{items:?}");
        let SkinRenderItem::Image { uv, rect, linear_filter, .. } = &items[0] else {
            panic!("{:?}", items[0])
        };
        assert!(approx_eq(uv.x, x) && approx_eq(uv.y, y), "{uv:?}");
        assert!(approx_eq(rect.width, 0.6));
        assert!(*linear_filter);
        let SkinRenderItem::Image { rect, .. } = &items[1] else { panic!() };
        assert!(approx_eq(rect.width, 0.2));
    }
    document.graph[0].timer = Some(999999);
    let items = document.select_render_items(&sources, &snapshot);
    let SkinRenderItem::Image { uv, .. } = &items[0] else { panic!() };
    assert!(approx_eq(uv.x, 10.0 / 22.0) && approx_eq(uv.y, 0.0));
    document.graph[0].timer = Some(-1);
    snapshot.time = TimeUs(275_000);
    let items = document.select_render_items(&sources, &snapshot);
    let SkinRenderItem::Image { uv, .. } = &items[0] else { panic!() };
    assert!(approx_eq(uv.x, 21.0 / 22.0) && approx_eq(uv.y, 0.0));
    document.graph[0].timer = Some(11);
    snapshot.selection_time = TimeUs(-1_000);
    let items = document.select_render_items(&sources, &snapshot);
    let SkinRenderItem::Image { uv, .. } = &items[0] else { panic!() };
    assert!(approx_eq(uv.x, 10.0 / 22.0) && approx_eq(uv.y, 0.0));
    snapshot.rows[0].folder_lamp_counts = [0; 11];
    assert!(document.select_render_items(&sources, &snapshot).is_empty());
    snapshot.rows[0].is_folder = false;
    snapshot.rows[0].kind = SelectRowKind::Song;
    snapshot.rows[0].download_progress =
        Some(crate::scene::SelectRowDownloadProgress { fraction: 0.5, lamp: 1 });
    assert!(document.select_render_items(&sources, &snapshot).is_empty());
}

#[test]
fn select_trophy_follows_course_definition_priority() {
    let mut row = SelectRowSnapshot {
        course_trophy_names: vec!["goldmedal".into(), "bronzemedal".into()],
        achieved_trophy_names: vec!["bronzemedal".into(), "goldmedal".into()],
        ..Default::default()
    };
    assert_eq!(select_row_trophy_index(&row), Some(0));
    row.course_trophy_names.push("custom-trophy".into());
    row.achieved_trophy_names.push("custom-trophy".into());
    assert!(select_row_trophy_index(&row).is_none());
    row.achieved_trophy_names.clear();
    assert!(select_row_trophy_index(&row).is_none());
}

fn label_document(cn_image: bool, cn_hidden: bool) -> SkinDocument {
    let mut images = (0..5).filter(|&index| index != 3 || cn_image).map(|index| {
        serde_json::json!({"id":format!("label{index}"),"src":"atlas","x":index * 10,"y":0,"w":10,"h":10})
    }).collect::<Vec<_>>();
    images.push(serde_json::json!({"id":"body","src":"atlas","x":0,"y":10,"w":10,"h":10}));
    let labels = (0..5).map(|index| {
        serde_json::json!({"id":format!("label{index}"),"op":if index == 3 && cn_hidden {vec![999999]} else {vec![]},"dst":[{"x":index*10,"y":0,"w":10,"h":10}]})
    }).collect::<Vec<_>>();
    serde_json::from_value(serde_json::json!({
        "type":5,"w":100,"h":100,"image":images,
        "songlist":{"id":"bars","liston":[{"id":"body","dst":[{"x":0,"y":40,"w":90,"h":10}]}],"label":labels},
        "destination":[{"id":"bars"}]
    })).unwrap()
}

fn rendered_labels(document: &SkinDocument, row: SelectRowSnapshot, policy: usize) -> Vec<i32> {
    let sources = HashMap::from([(
        "atlas".into(),
        SkinDocumentTexture {
            source_id: "atlas".into(),
            texture: SkinTextureId(1),
            source_size: SkinImageSize { width: 50.0, height: 20.0 },
        },
    )]);
    document
        .select_render_items(
            &sources,
            &SelectSnapshot {
                rows: vec![row],
                ln_policy_setting_index: policy,
                ..Default::default()
            },
        )
        .iter()
        .filter_map(|item| match item {
            SkinRenderItem::Image { uv, .. } if uv.y == 0.0 => Some((uv.x * 5.0).round() as i32),
            _ => None,
        })
        .collect()
}

#[test]
fn select_labels_choose_declared_and_undefined_ln_modes() {
    for (bits, fallback, expected) in [
        (0, 2, 0),
        (SKIN_SOURCE_LN_UNDEFINED_BIT, 0, 0),
        (SKIN_SOURCE_LN_UNDEFINED_BIT, 1, 3),
        (SKIN_SOURCE_LN_UNDEFINED_BIT, 2, 4),
        (SKIN_SOURCE_LN_DEFINED_LN_BIT, 2, 0),
        (SKIN_SOURCE_LN_DEFINED_CN_BIT, 0, 3),
        (SKIN_SOURCE_LN_DEFINED_HCN_BIT, 0, 4),
        (SKIN_SOURCE_LN_DEFINED_CN_BIT | SKIN_SOURCE_LN_UNDEFINED_BIT, 2, 4),
    ] {
        let row = SelectRowSnapshot {
            has_long_notes: true,
            has_mines: true,
            has_random: true,
            source_ln_profile_bits: Some(bits),
            ..Default::default()
        };
        assert_eq!(select_row_label_indices(&row, fallback), [expected, 2, 1]);
    }
}

#[test]
fn select_labels_fall_back_only_when_the_cn_image_is_absent() {
    let row = SelectRowSnapshot {
        kind: SelectRowKind::Song,
        in_library: true,
        has_long_notes: true,
        source_ln_profile_bits: Some(SKIN_SOURCE_LN_DEFINED_CN_BIT),
        ..Default::default()
    };
    assert_eq!(rendered_labels(&label_document(true, false), row.clone(), 0), [3]);
    assert_eq!(rendered_labels(&label_document(false, false), row.clone(), 0), [0]);
    let mut missing_source = label_document(true, false);
    missing_source.image.iter_mut().find(|image| image.id == "label3").unwrap().src =
        "missing".into();
    assert_eq!(rendered_labels(&missing_source, row.clone(), 0), [0]);
    assert!(rendered_labels(&label_document(true, true), row.clone(), 0).is_empty());
    assert!(
        rendered_labels(
            &label_document(true, false),
            SelectRowSnapshot { in_library: false, ..row },
            0
        )
        .is_empty()
    );
}

use super::*;

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

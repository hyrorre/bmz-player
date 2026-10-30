use super::*;
use bmz_render::scene::{SelectChartDistributionSecond, SelectRowKind};
use bmz_render::skin::SkinTextureId;

fn bar_document(parts: &str) -> SkinDocument {
    let root = unique_test_dir("bmz-lr2-select-decorations");
    fs::create_dir_all(&root).unwrap();
    let path = root.join("select.lr2skin");
    fs::write(&path, format!(
        "#INFORMATION,5,test,test\n#RESOLUTION,640,480\n#IMAGE,a.png\n#SRC_BAR_BODY,0,0,0,0,100,10,1,1,0,0\n#DST_BAR_BODY_OFF,0,0,0,100,100,10,0,255,255,255,255,1,0,0,0\n#DST_BAR_BODY_ON,0,0,0,100,100,10,0,255,255,255,255,1,0,0,0\n{parts}"
    )).unwrap();
    load_skin_document_uncached(
        &path,
        SkinKind::Select,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &LuaLoadRuntimeState::default(),
    )
    .unwrap()
    .document
}

fn bar_sources() -> HashMap<String, SkinDocumentTexture> {
    HashMap::from([(
        "0".into(),
        SkinDocumentTexture {
            source_id: "0".into(),
            texture: SkinTextureId(1),
            source_size: SkinImageSize { width: 100.0, height: 100.0 },
        },
    )])
}

#[test]
fn lr2_select_trophies_use_course_achievements_and_hide_unowned_rows() {
    let mut parts = String::new();
    for index in 0..3 {
        parts.push_str(&format!("#SRC_BAR_TROPHY,{index},0,{},40,10,10,1,1,0,0\n#DST_BAR_TROPHY,{index},0,0,0,10,10,0,255,255,255,255,1,0,0,0\n",index*10));
    }
    let document = bar_document(&parts);
    for (kind, owned, names, expected) in [
        (SelectRowKind::Course, true, vec!["bronzemedal"], Some(0)),
        (SelectRowKind::Course, true, vec!["silvermedal"], Some(1)),
        (SelectRowKind::Course, true, vec!["bronzemedal", "goldmedal"], Some(2)),
        (SelectRowKind::Course, true, vec!["custom-trophy"], None),
        (SelectRowKind::Course, false, vec!["goldmedal"], None),
        (SelectRowKind::Song, true, vec!["goldmedal"], None),
    ] {
        let snapshot = SelectSnapshot {
            time: TimeUs(1_000_000),
            rows: vec![SelectRowSnapshot {
                kind,
                in_library: owned,
                achieved_trophy_names: names.into_iter().map(str::to_string).collect(),
                ..Default::default()
            }],
            ..Default::default()
        };
        let trophies = document
            .select_render_items(&bar_sources(), &snapshot)
            .into_iter()
            .filter_map(|item| match item {
                SkinRenderItem::Image { uv, .. } if (uv.y - 0.4).abs() < 1e-6 => {
                    Some((uv.x * 10.0).round() as i32)
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(trophies, expected.into_iter().collect::<Vec<_>>(), "{kind:?}, owned={owned}");
    }
}

#[test]
fn lr2_select_label_commands_reach_the_cn_image() {
    let mut parts = String::new();
    for index in 0..5 {
        parts.push_str(&format!("#SRC_BAR_LABEL,{index},0,{},20,10,10,1,1,0,0\n#DST_BAR_LABEL,{index},0,0,0,10,10,0,255,255,255,255,1,0,0,0\n",index*10));
    }
    let document = bar_document(&parts);
    let snapshot = SelectSnapshot {
        time: TimeUs(1_000_000),
        rows: vec![SelectRowSnapshot {
            kind: SelectRowKind::Song,
            in_library: true,
            has_long_notes: true,
            source_ln_profile_bits: Some(bmz_render::snapshot::SKIN_SOURCE_LN_DEFINED_CN_BIT),
            ..Default::default()
        }],
        ..Default::default()
    };
    let labels = document
        .select_render_items(&bar_sources(), &snapshot)
        .into_iter()
        .filter_map(|item| match item {
            SkinRenderItem::Image { uv, .. } if (uv.y - 0.2).abs() < 1e-6 => {
                Some((uv.x * 10.0).round() as i32)
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(labels, [3]);
}

#[test]
fn lr2_select_notechart_renders_density_and_clears_on_selection_change() {
    let root = unique_test_dir("bmz-lr2-select-notechart");
    fs::create_dir_all(&root).unwrap();
    let path = root.join("select.lr2skin");
    fs::write(
        &path,
        concat!(
            "#INFORMATION,5,test,test\n#RESOLUTION,640,480\n",
            "#SRC_NOTECHART,0,111,0,0,0,0,0,0,0,0,300,120,0,0,300,1,0,1,1\n",
            "#DST_NOTECHART,0,0,50,200,0,0,0,255,255,255,255,1,0,0,0\n",
        ),
    )
    .unwrap();
    let document = load_skin_document_uncached(
        &path,
        SkinKind::Select,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &LuaLoadRuntimeState::default(),
    )
    .unwrap()
    .document;
    let mut snapshot = SelectSnapshot {
        time: TimeUs(1_000_000),
        rows: vec![SelectRowSnapshot {
            kind: SelectRowKind::Song,
            in_library: true,
            chart_distribution: vec![
                SelectChartDistributionSecond { key_taps: 4, mines: 1, ..Default::default() },
                SelectChartDistributionSecond {
                    scratch_taps: 2,
                    key_long_bodies: 3,
                    ..Default::default()
                },
            ],
            ..Default::default()
        }],
        ..Default::default()
    };
    let items = document.select_render_items(&HashMap::new(), &snapshot);
    assert_eq!(items.len(), 4, "{items:?}");
    for item in &items {
        let SkinRenderItem::Rect { rect, .. } = item else { panic!("{item:?}") };
        assert!(rect.x >= 50.0 / 640.0 && rect.x + rect.width <= 350.0 / 640.0 + 1e-6);
        assert!(rect.y >= 80.0 / 480.0 - 1e-6 && rect.y + rect.height <= 200.0 / 480.0 + 1e-6);
    }
    snapshot.time = TimeUs(0);
    assert!(document.select_render_items(&HashMap::new(), &snapshot).is_empty());
    snapshot.time = TimeUs(1_000_000);
    snapshot.selected_index = 1;
    snapshot.rows.push(SelectRowSnapshot {
        index: 1,
        kind: SelectRowKind::Song,
        in_library: true,
        ..Default::default()
    });
    assert!(document.select_render_items(&HashMap::new(), &snapshot).is_empty());
}

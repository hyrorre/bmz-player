use super::*;
use bmz_render::scene::{SelectChartDistributionSecond, SelectRowKind};

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

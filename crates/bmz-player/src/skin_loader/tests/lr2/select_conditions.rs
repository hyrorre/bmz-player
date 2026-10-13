use super::*;
use bmz_render::skin::{LR2_BUTTON_BASE, SkinClickTarget, SkinTextureId};

fn sources(document: &SkinDocument) -> HashMap<String, SkinDocumentTexture> {
    document
        .source
        .iter()
        .enumerate()
        .map(|(index, source)| {
            (
                source.id.clone(),
                SkinDocumentTexture {
                    source_id: source.id.clone(),
                    texture: SkinTextureId(index as u32 + 1),
                    source_size: SkinImageSize { width: 4000.0, height: 4000.0 },
                },
            )
        })
        .collect()
}

fn snapshot() -> SelectSnapshot {
    SelectSnapshot {
        time: TimeUs(1_500_000),
        option_panel_time: TimeUs(1_500_000),
        rows: vec![SelectRowSnapshot {
            kind: bmz_render::scene::SelectRowKind::Song,
            total_notes: 1700,
            ex_score: Some(3115),
            ..Default::default()
        }],
        ..Default::default()
    }
}

fn check_select_conditions(document: &SkinDocument) {
    let sources = sources(document);
    let settings = Default::default();
    let mut snapshot = snapshot();
    snapshot.option_panel = 1;
    for mode in ["14K", "7K", "10K", "ALL", "14K"] {
        snapshot.select_mode = mode.into();
        let dp = matches!(mode, "14K" | "10K");
        for (x, y, button) in [(481.0, 155.5, 43), (627.0, 582.5, 54)] {
            let hit =
                document.select_click_hit(&sources, &snapshot, &settings, x / 1280.0, y / 720.0);
            assert_eq!(hit.is_some_and(|hit| matches!(hit.target, SkinClickTarget::Event { event_id, .. } if event_id == LR2_BUTTON_BASE + button)), dp, "{mode}: button {button}");
        }
    }
    snapshot.select_mode = "14K".into();
    snapshot.mouse_position = Some((473.0 / 1280.0, 518.5 / 720.0));
    for (panel, score_hover) in [(1, false), (3, true), (0, false), (1, false)] {
        snapshot.option_panel = panel;
        let items = document.select_render_items(&sources, &snapshot);
        let has_score_hover = items.iter().any(|item| matches!(item,
            SkinRenderItem::Image { rect, .. } if (rect.x * 1280.0 - 373.0).abs() < 0.01 && (rect.y * 720.0 - 504.0).abs() < 0.01 && (rect.width * 1280.0 - 147.0).abs() < 0.01));
        assert_eq!(has_score_hover, score_hover, "panel={panel}");
    }
    snapshot.mouse_position = None;
    snapshot.option_panel = 0;
    for score in [0, 1700, 3115, 3400] {
        snapshot.rows[0].ex_score = Some(score);
        let items = document.select_render_items(&sources, &snapshot);
        let bar = items.iter().find_map(|item| match item {
            SkinRenderItem::Image { rect, uv, .. }
                if (uv.x * 4000.0 - 1116.0).abs() < 0.01
                    && (uv.y * 4000.0 - 301.0).abs() < 0.01 =>
            {
                Some(rect)
            }
            _ => None,
        });
        if score == 0 {
            assert!(bar.is_none());
        } else {
            let bar = bar.expect("score bar");
            assert!((bar.width * 1280.0 - 378.0 * score as f32 / 3400.0).abs() < 0.01);
        }
    }
    snapshot.rows[0].kind = bmz_render::scene::SelectRowKind::Folder;
    snapshot.rows[0].is_folder = true;
    assert!(!document.select_render_items(&sources, &snapshot).iter().any(|item| matches!(item,
        SkinRenderItem::Image { uv, .. } if (uv.x * 4000.0 - 1116.0).abs() < 0.01 && (uv.y * 4000.0 - 301.0).abs() < 0.01)));
}

#[test]
fn lr2_select_conditions_decode_and_render_without_external_assets() {
    let root = unique_test_dir("bmz-lr2-select-conditions");
    fs::create_dir_all(&root).unwrap();
    let path = root.join("select.lr2skin");
    fs::write(
        &path,
        "#INFORMATION,5,test,test\n#RESOLUTION,1280,720\n#IMAGE,a.png\n\
        #SRC_BUTTON,0,0,0,0,116,126,1,6,0,21,43,1,1\n\
        #DST_BUTTON,0,0,423,145,116,21,0,255,255,255,255,1,0,0,0,0,21,10,-11,0\n\
        #SRC_BUTTON,0,0,0,0,116,42,1,2,0,21,54,1,1\n\
        #DST_BUTTON,0,0,569,572,116,21,0,255,255,255,255,1,0,0,0,0,21,10,-11,0\n\
        #SRC_ONMOUSE,0,0,608,107,147,22,1,1,0,0,3,3,3,147,22\n\
        #DST_ONMOUSE,0,0,373,504,147,22,0,255,255,255,255,2,0,0,0,0,0,0,0,0\n\
        #SRC_BARGRAPH,0,0,1116,301,378,10,1,1,0,0,47,0,0\n\
        #DST_BARGRAPH,0,0,134,626,378,10,0,130,255,255,255,2,0,0,0,0,0,5,0,572\n",
    )
    .unwrap();
    let loaded = load_skin_document_uncached(
        &path,
        SkinKind::Select,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &LuaLoadRuntimeState::default(),
    )
    .unwrap();
    check_select_conditions(&loaded.document);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn lr2_wmix_select_conditions_render_when_available() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../data/skins/WMIX_HD/select/select.lr2skin");
    if !path.is_file() {
        eprintln!("SKIP missing WMIX_HD select");
        return;
    }
    let loaded = load_skin_document_uncached(
        &path,
        SkinKind::Select,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &LuaLoadRuntimeState::default(),
    )
    .unwrap();
    assert_eq!((loaded.document.w, loaded.document.h), (1280, 720));
    check_select_conditions(&loaded.document);
}

use super::*;
use crate::scene::SelectRowBannerSnapshot;
use serde_json::json;

fn banner(texture: u32) -> SelectRowBannerSnapshot {
    SelectRowBannerSnapshot {
        texture: SkinTextureId(texture),
        size: SkinImageSize { width: 200.0, height: 50.0 },
        lease: Arc::new(()),
    }
}

fn document() -> SkinDocument {
    let bars: Vec<_> =
        (0..3).map(|i| json!({"id":"bars","dst":[{"x":100,"y":i*100,"w":300,"h":50}]})).collect();
    serde_json::from_value(json!({"type":5,"w":1000,"h":1000,
        "image":[{"id":"body","src":"src","w":10,"h":10}],
        "imageset":[{"id":"bars","images":["body"]}],
        "songlist":{"id":"list","center":1,"liston":bars,"listoff":bars,
            "banner":[{"filter":0,"dst":[{"x":5,"y":7,"w":200,"h":50}]},
                      {"filter":1,"dst":[{"x":8,"y":9,"w":100,"h":25}]}],
            "flash":[{"id":"body","dst":[{"w":300,"h":50}]}]},
        "destination":[{"id":"list"}]}))
    .unwrap()
}

#[test]
fn row_specific_textures_follow_the_same_scrolling_origins_and_keep_part_order() {
    let doc = document();
    let sources = HashMap::from([(
        "src".into(),
        SkinDocumentTexture {
            source_id: "src".into(),
            texture: SkinTextureId(1),
            source_size: SkinImageSize { width: 10.0, height: 10.0 },
        },
    )]);
    let rows: Vec<_> = (0..3)
        .map(|i| SelectRowSnapshot {
            index: i,
            banner: Some(banner(100 + i)),
            ..Default::default()
        })
        .collect();
    for progress in [0.0_f32, 0.4, 0.999, 1.4, -0.6] {
        let mut snapshot = SelectSnapshot {
            chart_count: 31,
            selected_index: 1,
            rows: rows.clone(),
            bar_scroll_direction: if progress < 0.0 { -1 } else { 1 },
            bar_scroll_progress: progress.abs(),
            ..Default::default()
        };
        // With a backlog, logical rows differ from the physical draw window.
        if progress > 1.0 {
            snapshot.selected_index = 20;
            snapshot.bar_scroll_rows = Some(rows.clone());
        }
        let items = doc.select_render_items(&sources, &snapshot);
        let images: Vec<_> = items
            .iter()
            .filter_map(|item| {
                if let SkinRenderItem::Image { texture, rect, linear_filter, .. } = item {
                    Some((*texture, *rect, *linear_filter))
                } else {
                    None
                }
            })
            .collect();
        let banner_images: Vec<_> =
            images.iter().filter(|(texture, _, _)| texture.0 >= 100).collect();
        assert_eq!(
            banner_images.iter().map(|(id, _, _)| id.0).collect::<Vec<_>>(),
            [100, 100, 101, 101, 102, 102]
        );
        for image in &banner_images {
            let position =
                images.iter().position(|candidate| std::ptr::eq(candidate, *image)).unwrap();
            let body = images[..position]
                .iter()
                .rev()
                .find(|(id, rect, _)| id.0 == 1 && rect.width > 0.25)
                .unwrap();
            let offset = if image.2 { 8.0 } else { 5.0 };
            assert!((image.1.x - body.1.x - offset / 1000.0).abs() < 0.00001);
            let (y, height) = if image.2 { (9.0, 25.0) } else { (7.0, 50.0) };
            let expected_y = body.1.y + body.1.height - (y + height) / 1000.0;
            assert!((image.1.y - expected_y).abs() < 0.00001);
        }
    }
}

#[test]
fn missing_images_non_chart_rows_and_absent_definitions_draw_nothing() {
    let doc = document();
    let list = doc.songlist.as_ref().unwrap();
    let row = SelectRowSnapshot { banner: Some(banner(10)), ..Default::default() };
    for row in [
        SelectRowSnapshot { banner: None, ..row.clone() },
        SelectRowSnapshot { is_folder: true, ..row.clone() },
        SelectRowSnapshot { in_library: false, ..row.clone() },
        SelectRowSnapshot { kind: SelectRowKind::Command, ..row.clone() },
    ] {
        assert!(render(&doc, list, &row, (0, 0), &[], &Default::default()).is_empty());
    }
    let mut empty = doc.clone();
    empty.songlist.as_mut().unwrap().banner.clear();
    assert!(
        render(&empty, empty.songlist.as_ref().unwrap(), &row, (0, 0), &[], &Default::default())
            .is_empty()
    );
    let snapshot = SelectSnapshot { rows: vec![row], ..Default::default() };
    let mut missing = empty.clone();
    missing.songlist.as_mut().unwrap().banner = doc.songlist.as_ref().unwrap().banner.clone();
    let mut missing_snapshot = snapshot.clone();
    missing_snapshot.rows[0].banner = None;
    assert_eq!(
        empty.select_render_items(&HashMap::new(), &snapshot),
        missing.select_render_items(&HashMap::new(), &missing_snapshot)
    );
}

#[test]
fn banner_destinations_preserve_timer_draw_options_animation_filter_clip_and_stretch() {
    let mut doc = document();
    doc.songlist.as_mut().unwrap().banner = vec![
        serde_json::from_value(json!({
        "timer":1,"op":[1985],"draw":"number(71) >= 100","filter":0,"stretch":1,
        "dst":[{"time":0,"x":10,"y":0,"w":100,"h":100,"a":128,
                "clip_x":0,"clip_y":0,"clip_w":100,"clip_h":30},
               {"time":100,"x":30,"y":0,"w":100,"h":100}]}))
        .unwrap(),
    ];
    let list = doc.songlist.as_ref().unwrap();
    let row = SelectRowSnapshot {
        banner: Some(banner(10)),
        ex_score: Some(150),
        total_notes: 100,
        play_count: 1,
        ..Default::default()
    };
    let mut state = SkinDrawState {
        elapsed_ms: 50,
        start_input_ms: Some(50),
        select_screen: true,
        ..Default::default()
    };
    SkinDocument::apply_select_songlist_render_row_state(&mut state, &row, None);
    let items = render(&doc, list, &row, (100, 200), &[], &state);
    assert!(matches!(items.first(), Some(SkinRenderItem::PushClip { .. })), "{items:?}");
    assert!(matches!(items.last(), Some(SkinRenderItem::PopClip)));
    assert!(items.iter().any(|item| matches!(item,SkinRenderItem::Image { texture, linear_filter:false,.. } if *texture==SkinTextureId(10))));
    state.start_input_ms = None;
    assert!(render(&doc, list, &row, (0, 0), &[], &state).is_empty());
    state.start_input_ms = Some(50);
    state.select_ex_score = Some(50);
    state.ex_score = 50;
    assert!(render(&doc, list, &row, (0, 0), &[], &state).is_empty());
}

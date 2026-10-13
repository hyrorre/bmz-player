use super::*;
use bmz_render::skin::SkinClickTarget;

fn rows(center: u32) -> Vec<SelectRowSnapshot> {
    (0..25)
        .map(|slot| {
            let index = (center + 100 - 12 + slot) % 100;
            SelectRowSnapshot {
                index,
                title: format!("Song {index}"),
                play_level: "1".into(),
                ..Default::default()
            }
        })
        .collect()
}

#[test]
fn lr2_wmix_scroll_draw_and_click_positions_survive_continuous_input_when_available() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../data/skins/WMIX_HD/select/select.lr2skin");
    if !path.is_file() {
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
    let document = &loaded.document;
    let sources: HashMap<_, _> = document
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
        .collect();
    let before = SelectSnapshot {
        time: TimeUs(1_500_000),
        selection_time: TimeUs(1_500_000),
        chart_count: 100,
        selected_index: 40,
        select_mode: "7K".into(),
        rows: rows(40),
        bar_scroll_direction: 1,
        bar_scroll_progress: 0.4,
        ..Default::default()
    };
    // Large batches retain a bounded visual window around the old screen position.
    for steps in [1, 7, 36, 100] {
        let selected = (40 + steps) % 100;
        let after = SelectSnapshot {
            selected_index: selected,
            rows: rows(selected),
            bar_scroll_progress: 0.4,
            bar_scroll_rows: Some(rows(40)),
            ..before.clone()
        };
        let mut compared = std::collections::BTreeSet::new();
        let items = document.select_render_items(&sources, &after);
        for item in document.select_render_items(&sources, &before) {
            let SkinRenderItem::Image { rect, .. } = item else { continue };
            let x = rect.x + rect.width * 0.5;
            let y = rect.y + rect.height * 0.5;
            let Some(hit) = document.select_click_hit(&sources, &before, &Default::default(), x, y)
            else {
                continue;
            };
            let SkinClickTarget::SelectRow { row_index } = hit.target else { continue };
            let next = document
                .select_click_hit(&sources, &after, &Default::default(), x, y)
                .expect("same visible row after retargeting");
            assert!(
                matches!(next.target, SkinClickTarget::SelectRow { row_index: index } if index == row_index)
            );
            assert_eq!(hit.rect, next.rect, "steps={steps}, row={row_index}");
            assert!(
                items.iter().any(|item| matches!(item,
                SkinRenderItem::Image { rect, .. } if rect == &next.rect)),
                "real skin body must match click rectangle"
            );
            compared.insert(row_index);
        }
        assert!(!compared.is_empty(), "WMIX_HD center bar must be exercised");
    }
}

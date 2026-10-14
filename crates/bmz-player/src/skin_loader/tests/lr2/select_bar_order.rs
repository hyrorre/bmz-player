use super::*;
use bmz_render::skin::SkinTextureId;

const HEADER: &str = "#INFORMATION,5,test,test\n#RESOLUTION,640,480\n#IMAGE,a.png\n\
    #SRC_BAR_BODY,0,0,0,0,100,10,1,1,0,0\n\
    #SRC_BAR_FLASH,0,0,0,20,100,14,1,1,0,0\n\
    #BAR_CENTER,1\n";
/// Three bar slots 10px apart; slot 1 is the selected row.
const BODIES: &str = "#DST_BAR_BODY_OFF,0,0,0,100,100,10,0,255,255,255,255,0,0,0,0,0,0,0,0,0\n\
    #DST_BAR_BODY_OFF,1,0,0,110,100,10,0,255,255,255,255,0,0,0,0,0,0,0,0,0\n\
    #DST_BAR_BODY_OFF,2,0,0,120,100,10,0,255,255,255,255,0,0,0,0,0,0,0,0,0\n\
    #DST_BAR_BODY_ON,1,0,0,110,100,10,0,255,255,255,255,0,0,0,0,0,0,0,0,0\n";
/// The selection flash is taller than a bar and reaches into the next slot.
const FLASH: &str = "#DST_BAR_FLASH,0,0,0,-12,100,14,0,255,255,255,255,0,0,0,0,0,0,0,0,0\n";
/// An ordinary image (for example a panel or a mask) over the bar area.
fn mask(y: i32) -> String {
    format!(
        "#SRC_IMAGE,0,0,0,{y},10,10,1,1,0,0\n\
         #DST_IMAGE,0,0,0,100,100,30,0,255,255,255,255,0,0,0,0,0,0,0,0,0\n"
    )
}

/// Draw order of the bar bodies (`B`), the flash (`F`) and ordinary images (`M<y>`),
/// told apart by their source rows in the 100x100 atlas.
fn draw_order(files: &[(&str, String)], scroll: (i32, f32)) -> String {
    draw_order_with_panel(files, scroll, 0)
}

fn draw_order_with_panel(files: &[(&str, String)], scroll: (i32, f32), panel: u8) -> String {
    let root = unique_test_dir("bmz-lr2-select-bar-order");
    fs::create_dir_all(&root).unwrap();
    for (name, body) in files {
        fs::write(root.join(name), body).unwrap();
    }
    let document = load_skin_document_uncached(
        &root.join(files[0].0),
        SkinKind::Select,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &LuaLoadRuntimeState::default(),
    )
    .unwrap()
    .document;
    fs::remove_dir_all(root).unwrap();
    let sources: HashMap<String, SkinDocumentTexture> = document
        .source
        .iter()
        .map(|source| {
            (
                source.id.clone(),
                SkinDocumentTexture {
                    source_id: source.id.clone(),
                    texture: SkinTextureId(1),
                    source_size: SkinImageSize { width: 100.0, height: 100.0 },
                },
            )
        })
        .collect();
    let snapshot = SelectSnapshot {
        time: TimeUs(1_000_000),
        selection_time: TimeUs(1_000_000),
        chart_count: 3,
        selected_index: 1,
        option_panel: panel,
        bar_scroll_direction: scroll.0,
        bar_scroll_progress: scroll.1,
        rows: (0..3)
            .map(|index| SelectRowSnapshot {
                index,
                kind: bmz_render::scene::SelectRowKind::Song,
                in_library: true,
                ..Default::default()
            })
            .collect(),
        ..Default::default()
    };
    document
        .select_render_items(&sources, &snapshot)
        .iter()
        .filter_map(|item| match item {
            SkinRenderItem::Image { uv, .. } => Some(match (uv.y * 100.0).round() as i32 {
                0 => "B".to_string(),
                20 => "F".to_string(),
                y => format!("M{y}"),
            }),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[test]
fn lr2_select_bar_parts_keep_their_declaration_order() {
    let stopped = (0, 0.0);
    // ECBE declares the flash after every bar: the next bar must not cover it.
    let flash_last = format!("{HEADER}{BODIES}{FLASH}");
    assert_eq!(draw_order(&[("select.lr2skin", flash_last)], stopped), "B B B F");
    // A flash declared first is drawn under the bars, as LR2 sorts by DST line.
    let flash_first = format!("{HEADER}{FLASH}{BODIES}");
    assert_eq!(draw_order(&[("select.lr2skin", flash_first)], stopped), "F B B B");
    // While scrolling, the order is kept and the flash stays one item.
    for scroll in [(1, 0.4), (-1, 0.4)] {
        let flash_last = format!("{HEADER}{BODIES}{FLASH}");
        assert_eq!(draw_order(&[("select.lr2skin", flash_last)], scroll), "B B B F", "{scroll:?}");
    }
}

#[test]
fn lr2_select_bar_parts_interleave_with_ordinary_images() {
    // An image between the bars and the flash covers the bars but not the flash.
    let between = format!("{HEADER}{BODIES}{}{FLASH}", mask(40));
    assert_eq!(draw_order(&[("select.lr2skin", between)], (0, 0.0)), "B B B M40 F");
    // An image declared after every bar part covers them all.
    let after = format!("{HEADER}{BODIES}{FLASH}{}", mask(40));
    assert_eq!(draw_order(&[("select.lr2skin", after)], (0, 0.0)), "B B B F M40");
}

#[test]
fn lr2_select_bar_part_order_continues_across_includes() {
    // The bars come from the entry, the flash from an include with an image before
    // it, and another image follows the include in the entry.
    let entry = format!("{HEADER}{BODIES}#INCLUDE,parts.csv\n{}", mask(60));
    let parts = format!("{}{FLASH}", mask(40));
    assert_eq!(
        draw_order(&[("select.lr2skin", entry), ("parts.csv", parts)], (0, 0.0)),
        "B B B M40 F M60"
    );
}

#[test]
fn lr2_select_bar_part_order_follows_the_active_conditional_definition() {
    let source = "#SRC_BAR_FLASH,0,0,0,20,100,14,1,1,0,0\n";
    let entry = format!(
        "{HEADER}{BODIES}#IF,21\n{source}{FLASH}#ENDIF\n{}#IF,-21\n{source}{FLASH}#ENDIF\n",
        mask(40)
    );
    for (panel, expected) in [(0, "B B B M40 F"), (1, "B B B F M40")] {
        assert_eq!(
            draw_order_with_panel(&[("select.lr2skin", entry.clone())], (0, 0.0), panel),
            expected,
            "panel {panel}"
        );
    }
}

#[test]
fn lr2_select_bar_part_keeps_distinct_dst_conditions_for_the_same_source() {
    let entry =
        format!("{HEADER}{BODIES}#IF,21\n{FLASH}#ENDIF\n{}#IF,-21\n{FLASH}#ENDIF\n", mask(40));
    for (panel, expected) in [(0, "B B B M40 F"), (1, "B B B F M40")] {
        assert_eq!(
            draw_order_with_panel(&[("select.lr2skin", entry.clone())], (0, 0.0), panel),
            expected,
            "panel {panel}"
        );
    }
}

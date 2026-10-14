use super::*;
use bmz_render::scene::{ResultIrSnapshot, ResultIrState};
use bmz_render::skin::SkinTextureId;

/// ECBE draws each LR2 IR status (options 600..608) as its own label at the same
/// footer position (115,706,96,7). Exactly one label may appear for every state.
#[test]
fn ecbe_select_shows_one_ir_status_label_when_available() {
    let path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data/skins/ECBE/Select/select.lr2skin");
    if !path.is_file() {
        eprintln!("SKIP missing ECBE select");
        return;
    }
    let document = load_skin_document_uncached(
        &path,
        SkinKind::Select,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &LuaLoadRuntimeState::default(),
    )
    .unwrap()
    .document;
    assert_eq!((document.w, document.h), (1280, 720));
    let sources: HashMap<String, SkinDocumentTexture> = document
        .source
        .iter()
        .enumerate()
        .map(|(index, source)| {
            (
                source.id.clone(),
                SkinDocumentTexture {
                    source_id: source.id.clone(),
                    texture: SkinTextureId(index as u32 + 1),
                    source_size: SkinImageSize { width: 4096.0, height: 4096.0 },
                },
            )
        })
        .collect();
    let mut snapshot = SelectSnapshot {
        time: TimeUs(2_000_000),
        rows: vec![SelectRowSnapshot {
            kind: bmz_render::scene::SelectRowKind::Song,
            in_library: true,
            ..Default::default()
        }],
        chart_count: 1,
        ..Default::default()
    };
    let label_count = |snapshot: &SelectSnapshot| {
        document
            .select_render_items(&sources, snapshot)
            .iter()
            .filter(|item| {
                matches!(item, SkinRenderItem::Image { rect, .. }
                    if (rect.x * 1280.0 - 115.0).abs() < 0.01
                        && (rect.y * 720.0 - 706.0).abs() < 0.01
                        && (rect.width * 1280.0 - 96.0).abs() < 0.01
                        && (rect.height * 720.0 - 7.0).abs() < 0.01)
            })
            .count()
    };
    // Players -> no players -> failure -> players, plus the waiting states and no IR.
    for (state, total_player) in [
        (ResultIrState::Offline, None),
        (ResultIrState::Waiting, None),
        (ResultIrState::Loading, None),
        (ResultIrState::Loaded, Some(3)),
        (ResultIrState::Loaded, Some(0)),
        (ResultIrState::Failed, None),
        (ResultIrState::Loaded, Some(3)),
    ] {
        snapshot.ir = ResultIrSnapshot { online: true, state, total_player, ..Default::default() };
        assert_eq!(label_count(&snapshot), 1, "{state:?} {total_player:?}");
    }
}

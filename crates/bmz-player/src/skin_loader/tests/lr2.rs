use super::*;

fn installed_lr2_roots() -> [PathBuf; 1] {
    [Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data/skins")]
}

fn load_installed_lr2_document(path: &Path, kind: SkinKind) -> SkinDocument {
    let context = SkinPathContext::new(path, installed_lr2_roots()).unwrap();
    load_skin_document_uncached_with_path_context(
        path,
        kind,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &LuaLoadRuntimeState::default(),
        Some(&context),
    )
    .unwrap()
    .document
}

fn decode_installed_lr2_play_skin(path: &Path, files: &BTreeMap<String, String>) -> DecodedSkin {
    decode_beatoraja_skin_request(BeatorajaSkinDecodeRequest {
        skin_path: path,
        kind: SkinKind::Play,
        options: &BTreeMap::new(),
        files,
        runtime_state: &LuaLoadRuntimeState::default(),
        library_roots: &installed_lr2_roots(),
        pinned_sources: None,
        document_cache: None,
        source_cache: None,
        texture_cache: None,
        font_cache: None,
        installed_fonts: None,
    })
    .unwrap()
}

#[path = "lr2/cases_01.rs"]
mod cases_01;
#[path = "lr2/cases_02.rs"]
mod cases_02;
#[path = "lr2/cases_03.rs"]
mod cases_03;
#[path = "lr2/cases_04.rs"]
mod cases_04;
#[path = "lr2/cases_05.rs"]
mod cases_05;
#[path = "lr2/cases_06.rs"]
mod cases_06;
#[path = "lr2/scenes.rs"]
mod scenes;
#[path = "lr2/select_bar_order.rs"]
mod select_bar_order;
#[path = "lr2/select_conditions.rs"]
mod select_conditions;
#[path = "lr2/select_drawing.rs"]
mod select_drawing;
#[path = "lr2/select_ir_status.rs"]
mod select_ir_status;
#[path = "lr2/select_scroll.rs"]
mod select_scroll;
#[path = "lr2/select_skin_settings.rs"]
mod select_skin_settings;

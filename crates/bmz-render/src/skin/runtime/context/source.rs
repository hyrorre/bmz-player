use super::*;

pub(in crate::skin) fn select_runtime_document_sources<'a>(
    base_sources: &HashMap<String, SkinDocumentTexture>,
    runtime_sources: &'a Mutex<HashMap<String, SkinDocumentTexture>>,
    snapshot: &SelectSnapshot,
) -> std::sync::MutexGuard<'a, HashMap<String, SkinDocumentTexture>> {
    let mut sources = runtime_sources.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    update_runtime_document_source(
        &mut sources,
        base_sources,
        "100",
        snapshot
            .stage_background
            .then_some(snapshot.stage_image_size)
            .flatten()
            .map(|size| (SELECT_STAGE_TEXTURE, size)),
    );
    update_runtime_document_source(
        &mut sources,
        base_sources,
        "101",
        snapshot
            .backbmp_image
            .then_some(snapshot.backbmp_image_size)
            .flatten()
            .map(|size| (PLAY_BACKBMP_TEXTURE, size)),
    );
    update_runtime_document_source(
        &mut sources,
        base_sources,
        "102",
        snapshot
            .banner_image
            .then_some(snapshot.banner_image_size)
            .flatten()
            .map(|size| (SELECT_BANNER_TEXTURE, size)),
    );
    sources
}

pub(in crate::skin) fn static_runtime_document_sources<'a>(
    base_sources: &HashMap<String, SkinDocumentTexture>,
    runtime_sources: &'a Mutex<HashMap<String, SkinDocumentTexture>>,
    state: &SkinDrawState,
) -> std::sync::MutexGuard<'a, HashMap<String, SkinDocumentTexture>> {
    let mut sources = runtime_sources.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    update_runtime_document_source(
        &mut sources,
        base_sources,
        "100",
        state
            .stagefile_override
            .filter(|_| state.play_screen)
            .map(|frame| (TextureId(frame.texture.0), frame.source_size))
            .or_else(|| {
                state
                    .has_stagefile
                    .then_some(state.stagefile_image_size)
                    .flatten()
                    .map(|size| (SELECT_STAGE_TEXTURE, size))
            }),
    );
    update_runtime_document_source(
        &mut sources,
        base_sources,
        "101",
        state
            .has_backbmp
            .then_some((PLAY_BACKBMP_TEXTURE, SkinImageSize { width: 1.0, height: 1.0 })),
    );
    sources
}

fn update_runtime_document_source(
    sources: &mut HashMap<String, SkinDocumentTexture>,
    base_sources: &HashMap<String, SkinDocumentTexture>,
    source_id: &str,
    runtime_source: Option<(TextureId, SkinImageSize)>,
) {
    if let Some((texture, source_size)) = runtime_source {
        sources.insert(
            source_id.to_string(),
            SkinDocumentTexture {
                source_id: source_id.to_string(),
                texture: SkinTextureId(texture.0),
                source_size,
            },
        );
    } else if let Some(source) = base_sources.get(source_id) {
        sources.insert(source_id.to_string(), source.clone());
    } else {
        sources.remove(source_id);
    }
}

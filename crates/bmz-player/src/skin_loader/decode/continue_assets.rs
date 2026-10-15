use crate::skin_loader::*;

/// Resolves the assets an LR2 Play skin would hand to a following Result through
/// `#IMAGE,CONTINUE` / `#LR2FONT,CONTINUE`, without decoding images or fonts.
///
/// BMZ uses this to stand in for the Play skin when the attempt was played with
/// another skin (a Lua / JSON skin or an LR2 skin of another theme). The skin's
/// default options are used; `runtime_state` carries load-time options such as
/// the best clear (100..105).
pub fn resolve_lr2_continue_assets(
    skin_path: &Path,
    runtime_state: &LuaLoadRuntimeState,
    document_cache: Option<SharedSkinDocumentCache>,
) -> Result<bmz_skin::Lr2ContinueAssets> {
    let empty = BTreeMap::new();
    let LoadedSkinDocumentForDecode { document, files, .. } = load_skin_document_with_path_context(
        skin_path,
        SkinKind::Play,
        &empty,
        &empty,
        runtime_state,
        document_cache,
        None,
    )?;
    if !document.lr2 {
        return Ok(Default::default());
    }
    let skin_root = skin_path.parent().unwrap_or_else(|| Path::new("."));
    let absolute =
        |path: PathBuf| std::path::absolute(&path).unwrap_or(path).to_string_lossy().into_owned();
    let sources = document
        .source
        .iter()
        .filter_map(|source| {
            let mut source = source.clone();
            if !source.path.starts_with("bmz://") {
                let path = resolve_json_skin_source_path_with_context(
                    skin_root,
                    None,
                    &source.path,
                    &document,
                    &files,
                )?;
                source.path = absolute(path);
            }
            Some((source.id.clone(), source))
        })
        .collect();
    let fonts = document
        .font
        .iter()
        .filter(|font| !font.id.is_empty() && !font.path.is_empty())
        .filter_map(|font| {
            let path = resolve_json_skin_asset_path_with_context(
                skin_root, None, &font.path, &document, &files,
            )
            .filter(|path| is_supported_font_path(path))?;
            let mut font = font.clone();
            font.path = absolute(path);
            Some((font.id.clone(), font))
        })
        .collect();
    Ok(bmz_skin::Lr2ContinueAssets {
        sources,
        fonts,
        system_fonts: document.lr2_system_fonts.clone(),
    })
}

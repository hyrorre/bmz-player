use super::*;

pub(super) struct DecodedFonts {
    pub(super) fonts: Vec<DecodedFont>,
    pub(super) count: usize,
    pub(super) decode_us: u64,
    pub(super) payload_skipped: usize,
    pub(super) cache_hits: usize,
    pub(super) cache_misses: usize,
    pub(super) cache_uncacheable: usize,
    pub(super) cache_disabled: usize,
}

pub(super) fn decode_skin_fonts(
    document: &SkinDocument,
    skin_root: &Path,
    resolved_files: &BTreeMap<String, String>,
    path_context: Option<&SkinPathContext>,
    font_namespace: &str,
    font_cache: Option<&SharedSkinFontCache>,
    installed_fonts: Option<&HashMap<String, SkinFontCacheKey>>,
) -> DecodedFonts {
    let resolve_font_path = |font_id: &str, path: &str| {
        let font_path = resolve_json_skin_asset_path_with_context(
            skin_root,
            path_context,
            path,
            document,
            resolved_files,
        )?;
        if !is_supported_font_path(&font_path) {
            tracing::debug!(
                font_id,
                path = %font_path.display(),
                "skipping unsupported beatoraja skin font"
            );
            return None;
        }
        if is_bitmap_font_path(&font_path)
            && let Some(context) = path_context
            && let Err(error) = validate_bitmap_font_pages(context, &font_path)
        {
            tracing::warn!(path = %font_path.display(), %error, "invalid skin bitmap font pages");
            return None;
        }
        Some(font_path)
    };
    let mut tasks = Vec::new();
    for font in &document.font {
        if font.id.is_empty() || font.path.is_empty() {
            continue;
        }
        let Some(font_path) = resolve_font_path(&font.id, &font.path) else {
            continue;
        };
        // beatoraja `font.fallback` は主フォントと同じ規則でパスを解決し、
        // 別 ID のフォントとして登録してから主フォントへ順番を関連付ける。
        let mut fallbacks = Vec::new();
        for (index, fallback) in font.fallback.iter().enumerate() {
            let Some(path) = resolve_font_path(&font.id, &fallback.path) else {
                continue;
            };
            let stored_id = skin_font_fallback_id(font_namespace, &font.id, index);
            fallbacks.push(stored_id.clone());
            tasks.push((stored_id, path, Vec::new()));
        }
        tasks.push((format!("{font_namespace}:{}", font.id), font_path, fallbacks));
    }

    let count = tasks.len();
    let started_at = Instant::now();
    let decoded: Vec<(DecodedFont, FontCacheStatus)> = tasks
        .into_par_iter()
        .filter_map(|(stored_id, font_path, fallbacks)| {
            decode_skin_font(stored_id, font_path, fallbacks, font_cache, installed_fonts)
        })
        .collect();
    let decode_us = elapsed_us(started_at);
    // 読み込めなかった fallback は優先順から外す。
    let available: std::collections::HashSet<String> =
        decoded.iter().map(|(font, _)| font.stored_id.clone()).collect();
    let decoded: Vec<_> = decoded
        .into_iter()
        .map(|(mut font, status)| {
            font.fallbacks.retain(|id| available.contains(id));
            (font, status)
        })
        .collect();

    let mut outcome = DecodedFonts {
        fonts: Vec::with_capacity(decoded.len()),
        count,
        decode_us,
        payload_skipped: 0,
        cache_hits: 0,
        cache_misses: 0,
        cache_uncacheable: 0,
        cache_disabled: 0,
    };
    for (font, status) in decoded {
        match status {
            FontCacheStatus::Hit => outcome.cache_hits += 1,
            FontCacheStatus::Miss => outcome.cache_misses += 1,
            FontCacheStatus::SkippedInstalled => outcome.payload_skipped += 1,
            FontCacheStatus::Uncacheable => outcome.cache_uncacheable += 1,
            FontCacheStatus::Disabled => outcome.cache_disabled += 1,
        }
        outcome.fonts.push(font);
    }
    outcome
}

fn validate_bitmap_font_pages(context: &SkinPathContext, path: &Path) -> Result<()> {
    for page in bmz_render::bitmap_font::bitmap_font_page_paths(path)? {
        context.resolve_asset(&page.to_string_lossy())?;
    }
    Ok(())
}

/// `font.fallback` の各フォントを登録する ID。主フォントの ID とは衝突しない。
pub(in crate::skin_loader) fn skin_font_fallback_id(
    font_namespace: &str,
    font_id: &str,
    index: usize,
) -> String {
    format!("{font_namespace}:{font_id}#fallback{index}")
}

fn decode_skin_font(
    stored_id: String,
    font_path: PathBuf,
    fallbacks: Vec<String>,
    font_cache: Option<&SharedSkinFontCache>,
    installed_fonts: Option<&HashMap<String, SkinFontCacheKey>>,
) -> Option<(DecodedFont, FontCacheStatus)> {
    let cache_key = skin_font_cache_key(&font_path);
    if let (Some(installed_fonts), Some(cache_key)) = (installed_fonts, cache_key.as_ref())
        && installed_fonts.get(&stored_id) == Some(cache_key)
    {
        return Some((
            DecodedFont {
                stored_id,
                path: font_path,
                data: None,
                cache_key: Some(cache_key.clone()),
                fallbacks,
            },
            FontCacheStatus::SkippedInstalled,
        ));
    }
    tracing::trace!(font_id = %stored_id, path = %font_path.display(), "skin font decode begin");
    match decode_font_with_cache_key(&font_path, font_cache, cache_key) {
        Ok((data, status, cache_key)) => Some((
            DecodedFont { stored_id, path: font_path, data: Some(data), cache_key, fallbacks },
            status,
        )),
        Err(error) => {
            tracing::warn!(
                font_id = %stored_id,
                path = %font_path.display(),
                %error,
                "failed to load beatoraja skin font"
            );
            None
        }
    }
}

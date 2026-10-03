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
    let tasks: Vec<_> = document
        .font
        .iter()
        .filter_map(|font| {
            if font.id.is_empty() || font.path.is_empty() {
                return None;
            }
            let font_path = resolve_json_skin_asset_path_with_context(
                skin_root,
                path_context,
                &font.path,
                document,
                resolved_files,
            )?;
            if !is_supported_font_path(&font_path) {
                tracing::debug!(
                    font_id = %font.id,
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
            Some((format!("{font_namespace}:{}", font.id), font_path))
        })
        .collect();

    let count = tasks.len();
    let started_at = Instant::now();
    let decoded: Vec<(DecodedFont, FontCacheStatus)> = tasks
        .into_par_iter()
        .filter_map(|(stored_id, font_path)| {
            decode_skin_font(stored_id, font_path, font_cache, installed_fonts)
        })
        .collect();
    let decode_us = elapsed_us(started_at);

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

fn decode_skin_font(
    stored_id: String,
    font_path: PathBuf,
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
            },
            FontCacheStatus::SkippedInstalled,
        ));
    }
    match decode_font_with_cache_key(&font_path, font_cache, cache_key) {
        Ok((data, status, cache_key)) => {
            Some((DecodedFont { stored_id, path: font_path, data: Some(data), cache_key }, status))
        }
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

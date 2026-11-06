use crate::skin_loader::*;

fn decode_font(path: &Path, context: &SkinPathContext) -> Result<DecodedFontData> {
    let path = context.resolve_asset(&path.to_string_lossy())?;
    let path = path.as_path();
    if is_bitmap_font_path(path) {
        Ok(DecodedFontData::Bitmap(bmz_render::bitmap_font::load_bitmap_font_with_context(
            path, context,
        )?))
    } else {
        let bytes = bmz_skin_assets::read(path)
            .with_context(|| format!("failed to read font: {}", path.display()))?;
        Ok(DecodedFontData::Vector(bytes))
    }
}

#[cfg(test)]
pub(in crate::skin_loader) fn decode_font_with_cache(
    path: &Path,
    font_cache: Option<&SharedSkinFontCache>,
) -> Result<(DecodedFontData, FontCacheStatus, Option<SkinFontCacheKey>)> {
    let asset = bmz_skin_assets::SkinAsset::resolve(path)?;
    let context = SkinPathContext::for_entry(asset.backing_path())?;
    decode_font_with_cache_key(path, &context, font_cache, skin_font_cache_key(path))
}

pub(in crate::skin_loader) fn decode_font_with_cache_key(
    path: &Path,
    context: &SkinPathContext,
    font_cache: Option<&SharedSkinFontCache>,
    key: Option<SkinFontCacheKey>,
) -> Result<(DecodedFontData, FontCacheStatus, Option<SkinFontCacheKey>)> {
    let Some(font_cache) = font_cache else {
        return decode_font(path, context).map(|data| (data, FontCacheStatus::Disabled, None));
    };
    let Some(key) = key else {
        return decode_font(path, context).map(|data| (data, FontCacheStatus::Uncacheable, None));
    };
    if let Ok(mut cache) = font_cache.lock()
        && let Some(data) = cache.get(&key)
    {
        return Ok((data, FontCacheStatus::Hit, Some(key)));
    }
    let data = decode_font(path, context)?;
    if let Ok(mut cache) = font_cache.lock() {
        cache.insert(key.clone(), data.clone());
    }
    Ok((data, FontCacheStatus::Miss, Some(key)))
}

#[cfg(test)]
pub(in crate::skin_loader) fn skin_font_cache_key(path: &Path) -> Option<SkinFontCacheKey> {
    let asset = bmz_skin_assets::SkinAsset::resolve(path).ok()?;
    let context = SkinPathContext::for_entry(asset.backing_path()).ok()?;
    skin_font_cache_key_with_context(path, &context)
}

pub(in crate::skin_loader) fn skin_font_cache_key_with_context(
    path: &Path,
    context: &SkinPathContext,
) -> Option<SkinFontCacheKey> {
    let path = context.resolve_asset(&path.to_string_lossy()).ok()?;
    let asset = bmz_skin_assets::SkinAsset::resolve(&path).ok()?;
    let metadata = fs::metadata(asset.backing_path()).ok()?;
    let path = asset.logical_path().to_path_buf();
    let is_bitmap = is_bitmap_font_path(&path);
    let mut pages = Vec::new();
    if is_bitmap {
        for page in
            bmz_render::bitmap_font::bitmap_font_page_paths_with_context(&path, context).ok()?
        {
            let asset = bmz_skin_assets::SkinAsset::resolve(&page).ok()?;
            let metadata = fs::metadata(asset.backing_path()).ok()?;
            pages.push((
                asset.logical_path().to_path_buf(),
                metadata.modified().ok(),
                metadata.len(),
            ));
        }
    }
    Some(SkinFontCacheKey {
        is_bitmap,
        pages,
        path,
        modified: metadata.modified().ok(),
        len: metadata.len(),
    })
}

pub(in crate::skin_loader) fn font_data_cache_bytes(data: &DecodedFontData) -> usize {
    match data {
        DecodedFontData::Vector(bytes) => bytes.len(),
        DecodedFontData::Bitmap(font) => font
            .pages
            .values()
            .map(|page| page.image.pixels.len())
            .fold(font.glyphs.len().saturating_mul(64), usize::saturating_add),
    }
}

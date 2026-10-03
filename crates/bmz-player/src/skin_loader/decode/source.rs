use crate::skin_loader::*;

pub(in crate::skin_loader) fn is_skin_video_source_extension(extension: &str) -> bool {
    matches!(extension, "mp4" | "wmv" | "m4v" | "webm" | "mpg" | "mpeg" | "m1v" | "m2v" | "avi")
}

pub(in crate::skin_loader) fn is_skin_static_source_extension(extension: &str) -> bool {
    matches!(extension, "png" | "bmp" | "jpg" | "jpeg" | "gif" | "tga" | "cim")
}

pub(in crate::skin_loader) fn load_source_asset_with_cache<F>(
    path: &Path,
    is_video: bool,
    source_cache: Option<&SharedSkinSourceAssetCache>,
    load: F,
) -> Result<(RgbaImageAsset, SourceCacheStatus)>
where
    F: FnOnce() -> Result<RgbaImageAsset>,
{
    let Some(source_cache) = source_cache else {
        return load().map(|asset| (asset, SourceCacheStatus::Disabled));
    };
    let Some(key) = skin_source_asset_cache_key(path, is_video) else {
        return load().map(|asset| (asset, SourceCacheStatus::Uncacheable));
    };
    if let Ok(cache) = source_cache.lock()
        && let Some(asset) = cache.get(&key)
    {
        return Ok((asset, SourceCacheStatus::Hit));
    }
    let asset = load()?;
    if let Ok(mut cache) = source_cache.lock() {
        cache.insert(key, asset.clone());
    }
    Ok((asset, SourceCacheStatus::Miss))
}

pub(in crate::skin_loader) fn lookup_source_texture_cache(
    texture_cache: Option<&SharedSkinGpuTextureCache>,
    path: &Path,
    is_video: bool,
) -> (Option<CachedSkinGpuTexture>, Option<SkinSourceAssetCacheKey>, TextureCacheStatus) {
    let key = skin_source_asset_cache_key(path, is_video);
    match (texture_cache, key.as_ref()) {
        (Some(texture_cache), Some(key)) => {
            if let Ok(mut cache) = texture_cache.lock()
                && let Some(texture) = cache.get(key)
            {
                return (Some(texture), Some(key.clone()), TextureCacheStatus::Hit);
            }
            (None, Some(key.clone()), TextureCacheStatus::Miss)
        }
        (Some(_), None) => (None, None, TextureCacheStatus::Uncacheable),
        (None, _) => (None, key, TextureCacheStatus::Disabled),
    }
}

pub(in crate::skin_loader) fn elapsed_us(start: Instant) -> u64 {
    start.elapsed().as_micros().min(u64::MAX as u128) as u64
}

pub(in crate::skin_loader) fn skin_source_asset_cache_key(
    path: &Path,
    is_video: bool,
) -> Option<SkinSourceAssetCacheKey> {
    let asset = bmz_skin_assets::SkinAsset::resolve(path).ok()?;
    let metadata = fs::metadata(asset.backing_path()).ok()?;
    let path = asset.logical_path().to_path_buf();
    Some(SkinSourceAssetCacheKey {
        path,
        modified: metadata.modified().ok(),
        len: metadata.len(),
        is_video,
    })
}

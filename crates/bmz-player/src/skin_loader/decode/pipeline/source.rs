use super::*;

pub(super) struct DecodedSources {
    pub(super) pairs: Vec<DecodedSourceResult>,
    pub(super) task_count: usize,
    pub(super) decode_us: u64,
}

pub(super) fn decode_skin_sources(
    document: &SkinDocument,
    skin_root: &Path,
    resolved_files: &BTreeMap<String, String>,
    path_context: Option<&SkinPathContext>,
    required_sources: &HashSet<String>,
    warn_missing_required: bool,
    source_cache: Option<&SharedSkinSourceAssetCache>,
    texture_cache: Option<&SharedSkinGpuTextureCache>,
) -> DecodedSources {
    let tasks = collect_source_tasks(document, skin_root, resolved_files, path_context);
    let task_count = tasks.len();
    let started_at = Instant::now();
    let mut pairs: Vec<_> = tasks
        .into_par_iter()
        .filter_map(|task| {
            decode_source_task(
                task,
                required_sources,
                warn_missing_required,
                source_cache,
                texture_cache,
            )
        })
        .collect();
    let decode_us = elapsed_us(started_at);
    pairs.sort_by_key(|decoded| decoded.index);
    DecodedSources { pairs, task_count, decode_us }
}

fn collect_source_tasks(
    document: &SkinDocument,
    skin_root: &Path,
    resolved_files: &BTreeMap<String, String>,
    path_context: Option<&SkinPathContext>,
) -> Vec<SourceDecodeTask> {
    document
        .source
        .iter()
        .enumerate()
        .filter_map(|(index, source)| {
            if let Some(asset) = lr2_builtin_source_asset(&source.path) {
                return Some(SourceDecodeTask::Builtin {
                    index,
                    source_id: source.id.clone(),
                    path: PathBuf::from(&source.path),
                    asset,
                });
            }
            let path = resolve_json_skin_source_path_with_context(
                skin_root,
                path_context,
                &source.path,
                document,
                resolved_files,
            )?;
            let extension = path
                .extension()
                .and_then(|extension| extension.to_str())
                .map(str::to_ascii_lowercase)
                .unwrap_or_default();
            if is_skin_static_source_extension(&extension) {
                Some(SourceDecodeTask::File {
                    index,
                    source_id: source.id.clone(),
                    path,
                    color_key: source.lr2_color_key,
                })
            } else if is_skin_video_source_extension(&extension) {
                Some(SourceDecodeTask::Video { index, source_id: source.id.clone(), path })
            } else {
                tracing::debug!(
                    source_id = %source.id,
                    path = %path.display(),
                    "skipping unsupported beatoraja skin source"
                );
                None
            }
        })
        .collect()
}

fn decode_source_task(
    task: SourceDecodeTask,
    required_sources: &HashSet<String>,
    warn_missing_required: bool,
    source_cache: Option<&SharedSkinSourceAssetCache>,
    texture_cache: Option<&SharedSkinGpuTextureCache>,
) -> Option<DecodedSourceResult> {
    match task {
        SourceDecodeTask::Builtin { index, source_id, path, asset } => {
            let size = SkinImageSize { width: asset.width as f32, height: asset.height as f32 };
            Some(DecodedSourceResult {
                texture_lease: None,
                index,
                source_id,
                path,
                asset: Some(asset),
                size,
                is_video: false,
                cached_texture: None,
                cache_key: None,
                source_status: None,
                texture_status: None,
            })
        }
        SourceDecodeTask::File { index, source_id, path, color_key } => decode_file_source(
            index,
            source_id,
            path,
            color_key,
            required_sources,
            warn_missing_required,
            source_cache,
            texture_cache,
        ),
        SourceDecodeTask::Video { index, source_id, path } => {
            decode_video_source(index, source_id, path, source_cache, texture_cache)
        }
    }
}

fn decode_file_source(
    index: usize,
    source_id: String,
    path: PathBuf,
    color_key: Option<[u8; 3]>,
    required_sources: &HashSet<String>,
    warn_missing_required: bool,
    source_cache: Option<&SharedSkinSourceAssetCache>,
    texture_cache: Option<&SharedSkinGpuTextureCache>,
) -> Option<DecodedSourceResult> {
    let key = skin_source_asset_cache_key(&path, false).map(|mut key| {
        key.color_key = color_key;
        key
    });
    let (cached_texture, cache_key, texture_status) =
        lookup_source_texture_cache_key(texture_cache, key);
    if let Some(cached_texture) = cached_texture {
        return Some(cached_source_result(
            index,
            source_id,
            path,
            cached_texture,
            cache_key,
            false,
            texture_status,
        ));
    }
    match load_source_asset_with_cache_key(cache_key.clone(), source_cache, || {
        bmz_render::assets::load_static_rgba_image_with_color_key(&path, color_key)
    }) {
        Ok((asset, source_status)) => Some(asset_source_result(
            index,
            source_id,
            path,
            asset,
            cache_key,
            false,
            source_status,
            texture_status,
        )),
        Err(error) => {
            if warn_missing_required && required_sources.contains(&source_id) {
                tracing::warn!(
                    source_id = %source_id,
                    path = %path.display(),
                    %error,
                    "failed to load beatoraja skin source"
                );
            } else {
                tracing::debug!(
                    source_id = %source_id,
                    path = %path.display(),
                    %error,
                    "skipping unused missing beatoraja skin source"
                );
            }
            None
        }
    }
}

fn decode_video_source(
    index: usize,
    source_id: String,
    path: PathBuf,
    source_cache: Option<&SharedSkinSourceAssetCache>,
    texture_cache: Option<&SharedSkinGpuTextureCache>,
) -> Option<DecodedSourceResult> {
    let (cached_texture, cache_key, texture_status) =
        lookup_source_texture_cache(texture_cache, &path, true);
    if let Some(cached_texture) = cached_texture {
        return Some(cached_source_result(
            index,
            source_id,
            path,
            cached_texture,
            cache_key,
            true,
            texture_status,
        ));
    }
    match load_source_asset_with_cache(&path, true, source_cache, || {
        load_skin_video_first_frame_rgba(&path)
    }) {
        Ok((asset, source_status)) => Some(asset_source_result(
            index,
            source_id,
            path,
            asset,
            cache_key,
            true,
            source_status,
            texture_status,
        )),
        Err(error) => {
            tracing::warn!(
                source_id = %source_id,
                path = %path.display(),
                %error,
                "failed to load beatoraja skin video source"
            );
            None
        }
    }
}

fn cached_source_result(
    index: usize,
    source_id: String,
    path: PathBuf,
    cached: CachedSkinGpuTexture,
    cache_key: Option<SkinSourceAssetCacheKey>,
    is_video: bool,
    texture_status: TextureCacheStatus,
) -> DecodedSourceResult {
    DecodedSourceResult {
        index,
        source_id,
        path,
        asset: None,
        size: cached.size,
        is_video,
        cached_texture: Some(cached.texture),
        texture_lease: Some(cached.lease),
        cache_key,
        source_status: None,
        texture_status: Some(texture_status),
    }
}

fn asset_source_result(
    index: usize,
    source_id: String,
    path: PathBuf,
    asset: RgbaImageAsset,
    cache_key: Option<SkinSourceAssetCacheKey>,
    is_video: bool,
    source_status: SourceCacheStatus,
    texture_status: TextureCacheStatus,
) -> DecodedSourceResult {
    let size = SkinImageSize { width: asset.width as f32, height: asset.height as f32 };
    DecodedSourceResult {
        index,
        source_id,
        path,
        asset: Some(asset),
        size,
        is_video,
        cached_texture: None,
        texture_lease: None,
        cache_key,
        source_status: Some(source_status),
        texture_status: Some(texture_status),
    }
}

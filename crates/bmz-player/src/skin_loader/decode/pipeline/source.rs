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
    let deferred_bytes = std::sync::atomic::AtomicUsize::new(0);
    let mut pairs: Vec<_> = tasks
        .into_par_iter()
        .filter_map(|task| {
            let decoded = decode_source_task(
                task,
                required_sources,
                warn_missing_required,
                source_cache,
                texture_cache,
            )?;
            if let Some(image) = &decoded.deferred
                && deferred_bytes
                    .try_update(
                        std::sync::atomic::Ordering::Relaxed,
                        std::sync::atomic::Ordering::Relaxed,
                        |bytes| {
                            bytes
                                .checked_add(image.stored_bytes())
                                .filter(|bytes| *bytes <= 512 * 1024 * 1024)
                        },
                    )
                    .is_err()
            {
                tracing::warn!(
                    source_id = %decoded.source_id,
                    "deferred skin images exceed 512 MiB"
                );
                return None;
            }
            Some(decoded)
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
            if let Some(frames) = document.source_frames.get(&source.id) {
                let context = path_context?;
                if frames.paths.is_empty() || frames.paths.len() > 4096 || frames.columns == 0 {
                    tracing::warn!(source_id = %source.id, "invalid skin frame sequence");
                    return None;
                }
                let paths = frames
                    .paths
                    .iter()
                    .map(|path| {
                        if path == "bmz://lr2/empty" {
                            Ok(PathBuf::from(path))
                        } else {
                            context.resolve_asset(path)
                        }
                    })
                    .collect::<Result<Vec<_>>>();
                return match paths {
                    Ok(paths) => Some(SourceDecodeTask::Frames {
                        index,
                        source_id: source.id.clone(),
                        paths,
                        columns: frames.columns,
                    }),
                    Err(error) => {
                        tracing::warn!(source_id = %source.id, %error, "invalid skin frame asset");
                        None
                    }
                };
            }
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
                if document.deferred_sources.contains(&source.id) {
                    return Some(SourceDecodeTask::Deferred {
                        index,
                        source_id: source.id.clone(),
                        path,
                    });
                }
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
        SourceDecodeTask::Deferred { index, source_id, path } => {
            let asset = bmz_render::assets::load_static_rgba_image_with_color_key(&path, None)
                .and_then(|image| bmz_render::assets::deferred::DeferredImage::encode(&image));
            match asset {
                Ok(image) => Some(DecodedSourceResult {
                    size: SkinImageSize { width: image.width as f32, height: image.height as f32 },
                    deferred: Some(Arc::new(image)),
                    index,
                    source_id,
                    path,
                    asset: None,
                    animation: None,
                    texture_lease: None,
                    is_video: false,
                    cached_texture: None,
                    cache_key: None,
                    source_status: Some(SourceCacheStatus::Uncacheable),
                    texture_status: Some(TextureCacheStatus::Uncacheable),
                }),
                Err(error) => {
                    tracing::warn!(%source_id, %error, "failed to compress skin frame");
                    None
                }
            }
        }
        SourceDecodeTask::Frames { index, source_id, paths, columns } => {
            let path = paths.first()?.clone();
            match load_frame_sequence(&paths, columns) {
                Ok(asset) => Some(asset_source_result(
                    index,
                    source_id,
                    path,
                    asset,
                    None,
                    false,
                    SourceCacheStatus::Uncacheable,
                    TextureCacheStatus::Uncacheable,
                )),
                Err(error) => {
                    tracing::warn!(%source_id, %error, "failed to assemble skin frames");
                    None
                }
            }
        }
        SourceDecodeTask::Builtin { index, source_id, path, asset } => {
            let size = SkinImageSize { width: asset.width as f32, height: asset.height as f32 };
            Some(DecodedSourceResult {
                deferred: None,
                texture_lease: None,
                index,
                source_id,
                path,
                asset: Some(asset),
                animation: None,
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

/// Frame sequences keep their specified grid, so numbers and animation indices
/// refer to the same cells on every GPU. Inputs and output have explicit limits.
fn load_frame_sequence(paths: &[PathBuf], columns: u32) -> Result<RgbaImageAsset> {
    anyhow::ensure!(
        !paths.is_empty() && paths.len() <= 4096 && columns > 0 && columns as usize <= paths.len(),
        "invalid skin frame grid"
    );
    let mut images = Vec::new();
    let mut bytes = 0usize;
    for path in paths {
        let image = match lr2_builtin_source_asset(&path.to_string_lossy()) {
            Some(image) => image,
            None => bmz_render::assets::load_static_rgba_image_with_color_key(path, None)?,
        };
        bytes = bytes.saturating_add(image.pixels.len());
        anyhow::ensure!(bytes <= 128 * 1024 * 1024, "skin frame input exceeds memory budget");
        images.push(image);
    }
    let cell_w = images.iter().map(|image| image.width).max().unwrap();
    let cell_h = images.iter().map(|image| image.height).max().unwrap();
    let rows = (images.len() as u32).div_ceil(columns);
    let width = cell_w.checked_mul(columns).context("skin frame width overflow")?;
    let height = cell_h.checked_mul(rows).context("skin frame height overflow")?;
    let byte_count = u64::from(width) * u64::from(height) * 4;
    anyhow::ensure!(
        width <= 16384 && height <= 16384 && byte_count <= 128 * 1024 * 1024,
        "skin frame atlas exceeds size limit"
    );
    let mut pixels = vec![0; byte_count as usize];
    for (i, image) in images.into_iter().enumerate() {
        // Qwilight stretches each complete frame into the same destination.
        // Uniform atlas UVs require smaller frames to fill their cell too.
        let image = if image.width != cell_w || image.height != cell_h {
            let rgba = image::RgbaImage::from_raw(image.width, image.height, image.pixels)
                .context("invalid skin frame pixels")?;
            let resized = image::imageops::resize(
                &rgba,
                cell_w,
                cell_h,
                image::imageops::FilterType::Triangle,
            );
            RgbaImageAsset { width: cell_w, height: cell_h, pixels: resized.into_raw() }
        } else {
            image
        };
        let x = (i as u32 % columns) * cell_w;
        let y = (i as u32 / columns) * cell_h;
        for row in 0..image.height {
            let from = (row * image.width * 4) as usize;
            let to = ((y + row) * width * 4 + x * 4) as usize;
            let len = (image.width * 4) as usize;
            pixels[to..to + len].copy_from_slice(&image.pixels[from..from + len]);
        }
    }
    Ok(RgbaImageAsset { width, height, pixels })
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
    tracing::trace!(
        source_id = %source_id,
        path = %path.display(),
        "skin source file decode begin"
    );
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
    match load_skin_file_source(&path, color_key, cache_key.clone(), source_cache) {
        Ok(LoadedSkinFileSource::Still(asset, source_status)) => Some(asset_source_result(
            index,
            source_id,
            path,
            asset,
            cache_key,
            false,
            source_status,
            texture_status,
        )),
        Ok(LoadedSkinFileSource::Animated(animation)) => {
            let (width, height) =
                bmz_render::assets::sprite_sheet::provisional_sprite_sheet_size(&animation);
            Some(DecodedSourceResult {
                deferred: None,
                texture_lease: None,
                index,
                source_id,
                path,
                asset: None,
                animation: Some(animation),
                size: SkinImageSize { width: width as f32, height: height as f32 },
                is_video: false,
                cached_texture: None,
                cache_key,
                source_status: Some(SourceCacheStatus::Uncacheable),
                texture_status: Some(texture_status),
            })
        }
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

enum LoadedSkinFileSource {
    Still(RgbaImageAsset, SourceCacheStatus),
    Animated(Arc<bmz_render::assets::AnimatedImage>),
}

/// Loads a still image through the CPU source cache. Animated frames skip that cache:
/// their sprite sheet depends on the GPU that receives it.
fn load_skin_file_source(
    path: &Path,
    color_key: Option<[u8; 3]>,
    cache_key: Option<SkinSourceAssetCacheKey>,
    source_cache: Option<&SharedSkinSourceAssetCache>,
) -> Result<LoadedSkinFileSource> {
    if !bmz_render::assets::may_animate(path) {
        return load_source_asset_with_cache_key(cache_key, source_cache, || {
            bmz_render::assets::load_static_rgba_image_with_color_key(path, color_key)
        })
        .map(|(asset, status)| LoadedSkinFileSource::Still(asset, status));
    }
    if let (Some(cache), Some(key)) = (source_cache, cache_key.as_ref())
        && let Ok(cache) = cache.lock()
        && let Some(asset) = cache.get(key)
    {
        return Ok(LoadedSkinFileSource::Still(asset, SourceCacheStatus::Hit));
    }
    match bmz_render::assets::load_skin_source_image_with_color_key(path, color_key)? {
        bmz_render::assets::DecodedImage::Animated(animation) => {
            Ok(LoadedSkinFileSource::Animated(animation))
        }
        bmz_render::assets::DecodedImage::Static(asset) => {
            let status = match (source_cache, cache_key) {
                (None, _) => SourceCacheStatus::Disabled,
                (Some(_), None) => SourceCacheStatus::Uncacheable,
                (Some(cache), Some(key)) => {
                    if let Ok(mut cache) = cache.lock() {
                        cache.insert(key, asset.clone());
                    }
                    SourceCacheStatus::Miss
                }
            };
            Ok(LoadedSkinFileSource::Still(asset, status))
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
        deferred: None,
        index,
        source_id,
        path,
        asset: None,
        animation: None,
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
        deferred: None,
        index,
        source_id,
        path,
        asset: Some(asset),
        animation: None,
        size,
        is_video,
        cached_texture: None,
        texture_lease: None,
        cache_key,
        source_status: Some(source_status),
        texture_status: Some(texture_status),
    }
}

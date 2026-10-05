use crate::skin_loader::*;

pub(in crate::skin_loader) enum SourceDecodeTask {
    File { index: usize, source_id: String, path: PathBuf, color_key: Option<[u8; 3]> },
    Video { index: usize, source_id: String, path: PathBuf },
    Builtin { index: usize, source_id: String, path: PathBuf, asset: RgbaImageAsset },
}

pub(in crate::skin_loader) struct DecodedSourceResult {
    pub(in crate::skin_loader) texture_lease: Option<Arc<()>>,
    pub(in crate::skin_loader) index: usize,
    pub(in crate::skin_loader) source_id: String,
    pub(in crate::skin_loader) path: PathBuf,
    pub(in crate::skin_loader) asset: Option<RgbaImageAsset>,
    pub(in crate::skin_loader) size: SkinImageSize,
    pub(in crate::skin_loader) is_video: bool,
    pub(in crate::skin_loader) cached_texture: Option<SkinTextureId>,
    pub(in crate::skin_loader) cache_key: Option<SkinSourceAssetCacheKey>,
    pub(in crate::skin_loader) source_status: Option<SourceCacheStatus>,
    pub(in crate::skin_loader) texture_status: Option<TextureCacheStatus>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::skin_loader) enum SourceCacheStatus {
    Hit,
    Miss,
    Uncacheable,
    Disabled,
}

pub(in crate::skin_loader) const MAX_SKIN_AUDIO_ASSETS: usize = 64;

pub(in crate::skin_loader) fn decode_skin_audio_assets(
    kind: SkinKind,
    skin_root: &Path,
    path_context: Option<&SkinPathContext>,
    document: &SkinDocument,
) -> Vec<DecodedSkinAudio> {
    if kind != SkinKind::Result {
        return Vec::new();
    }
    let mut paths = document
        .scene_audio
        .iter()
        .chain(document.custom_events.iter().flat_map(|event| &event.audio_actions))
        .map(|action| action.path.clone())
        .filter(|path| !path.is_empty())
        .collect::<HashSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    paths.sort();
    if paths.len() > MAX_SKIN_AUDIO_ASSETS {
        tracing::warn!(
            count = paths.len(),
            limit = MAX_SKIN_AUDIO_ASSETS,
            "truncating excessive skin audio asset list"
        );
        paths.truncate(MAX_SKIN_AUDIO_ASSETS);
    }
    paths
        .into_par_iter()
        .filter_map(|path| {
            let Some(resolved) =
                resolve_skin_audio_path_with_context(skin_root, path_context, &path)
            else {
                tracing::warn!(path, "skipping invalid or missing skin audio asset");
                return None;
            };
            let mut loader = FfmpegSampleLoader::default();
            match loader.load(&resolved) {
                Ok(sample) => Some(DecodedSkinAudio { path, sample }),
                Err(error) => {
                    tracing::warn!(
                        path = %resolved.display(),
                        %error,
                        "failed to decode skin audio asset"
                    );
                    None
                }
            }
        })
        .collect()
}

#[cfg(test)]
pub(in crate::skin_loader) fn resolve_skin_audio_path(
    skin_root: &Path,
    path: &str,
) -> Option<PathBuf> {
    resolve_skin_audio_path_with_context(skin_root, None, path)
}

pub(in crate::skin_loader) fn resolve_skin_audio_path_with_context(
    skin_root: &Path,
    path_context: Option<&SkinPathContext>,
    path: &str,
) -> Option<PathBuf> {
    if let Some(path_context) = path_context {
        return path_context.resolve_file(path).ok();
    }
    let normalized = path.replace('\\', "/");
    let relative = Path::new(&normalized);
    if relative.is_absolute()
        || relative.components().any(|component| {
            matches!(
                component,
                std::path::Component::ParentDir
                    | std::path::Component::RootDir
                    | std::path::Component::Prefix(_)
            )
        })
    {
        return None;
    }
    let canonical_root = fs::canonicalize(skin_root).ok()?;
    let candidate = resolve_case_insensitive_path(&skin_root.join(relative));
    let canonical = fs::canonicalize(candidate).ok()?;
    canonical.is_file().then_some(())?;
    canonical.starts_with(canonical_root).then_some(canonical)
}

use std::path::{Path, PathBuf};

/// Resolve only the container of an archive locator, never the virtual entry.
pub fn canonical_chart_path(path: &Path) -> anyhow::Result<PathBuf> {
    let locator = crate::chart_source::ChartLocator::parse(path)?;
    if !locator.is_archive() && crate::chart_source::is_archive_file(locator.container_path()) {
        anyhow::bail!(
            "An archive contains multiple charts. Add its folder with `bmz-player songs load`, then select a chart, or specify archive.zip!/folder/chart.bms: {}",
            path.display()
        );
    }
    let locator = locator.canonicalize()?;
    let path = locator.to_path_buf();
    anyhow::ensure!(
        crate::storage::scan::is_chart_file(&path),
        "unsupported chart extension: {}",
        path.display()
    );
    Ok(path)
}

/// File information is cheap enough for Select; hashing and extraction stay in workers.
pub(crate) fn archive_asset_stamp(folder: &str) -> Option<String> {
    let locator = crate::chart_source::ChartLocator::parse(Path::new(folder)).ok()?;
    locator
        .is_archive()
        .then(|| format!("{:?}", crate::song_archive::metadata_stamp(locator.container_path())))
}

pub(crate) fn asset_cache_key(folder: &str, file: &str) -> String {
    match archive_asset_stamp(folder) {
        Some(stamp) => format!(
            "archive-asset|{}",
            serde_json::to_string(&(folder, file, stamp)).expect("string tuple")
        ),
        None => format!("{folder}|{file}"),
    }
}

/// Called exclusively by asset workers. A changed source never fills an older cache key.
pub(crate) fn resolve_cached_asset(
    key: &str,
    cache_root: &Path,
    preview: bool,
) -> anyhow::Result<Option<PathBuf>> {
    let (folder, file) = if let Some(encoded) = key.strip_prefix("archive-asset|") {
        let (folder, file, _): (String, String, String) = serde_json::from_str(encoded)?;
        anyhow::ensure!(
            asset_cache_key(&folder, &file) == key,
            "archive changed before asset load"
        );
        (folder, file)
    } else {
        let (folder, file) = key.split_once('|').unwrap_or(("", ""));
        (folder.to_owned(), file.to_owned())
    };
    let locator = crate::chart_source::ChartLocator::parse(Path::new(&folder))?;
    if let crate::chart_source::ChartLocator::Archive { container, entry } = &locator {
        let path = resolve_archive_asset(container, entry, &file, cache_root, preview)?;
        anyhow::ensure!(
            asset_cache_key(&folder, &file) == key,
            "archive changed during asset load"
        );
        return Ok(path);
    }
    let resolved = locator.materialize(cache_root)?;
    // Validate the requested name before any file probing, then validate extension fallback too.
    let requested = resolved.resolve_asset(file.trim())?;
    let asset_name = if resolved.archive_root.is_some() && !file.trim().is_empty() {
        requested.to_string_lossy().into_owned()
    } else {
        file.clone()
    };
    let path = if preview {
        resolve_preview_file(&resolved.path, &asset_name)
    } else {
        resolve_chart_asset_path(&resolved.path.to_string_lossy(), &asset_name)
    };
    if let Some(path) = &path {
        resolved.validate_asset_path(path)?;
    }
    anyhow::ensure!(asset_cache_key(&folder, &file) == key, "archive changed during asset load");
    Ok(path)
}

/// Concurrent Select asset extractions; each may decode a solid archive prefix.
const ARCHIVE_ASSET_WORKERS: usize = 2;
static ARCHIVE_ASSET_SLOTS: (std::sync::Mutex<usize>, std::sync::Condvar) =
    (std::sync::Mutex::new(0), std::sync::Condvar::new());

struct ArchiveAssetSlot;
impl ArchiveAssetSlot {
    fn acquire() -> Self {
        let (count, ready) = &ARCHIVE_ASSET_SLOTS;
        let mut count = ready
            .wait_while(count.lock().unwrap_or_else(|e| e.into_inner()), |count| {
                *count >= ARCHIVE_ASSET_WORKERS
            })
            .unwrap_or_else(|e| e.into_inner());
        *count += 1;
        Self
    }
}
impl Drop for ArchiveAssetSlot {
    fn drop(&mut self) {
        let (count, ready) = &ARCHIVE_ASSET_SLOTS;
        *count.lock().unwrap_or_else(|e| e.into_inner()) -= 1;
        ready.notify_one();
    }
}

/// Select images and previews need one entry; extracting the whole archive here would
/// let cursor movement fill the disk with every pack scrolled past.
fn resolve_archive_asset(
    container: &Path,
    folder: &str,
    file: &str,
    cache_root: &Path,
    preview: bool,
) -> anyhow::Result<Option<PathBuf>> {
    let _slot = ArchiveAssetSlot::acquire();
    let control = crate::song_archive::ArchiveControl::default();
    let index = crate::song_archive::inspect(container, &control)?;
    let Some(entry) = archive_asset_entry(&index, folder, file, preview)? else {
        return Ok(None);
    };
    crate::song_archive::materialize_entry(
        container,
        &index.generation,
        &entry,
        cache_root,
        &control,
    )
    .map(Some)
}

/// Mirror resolve_chart_asset_path/resolve_preview_file over the archive index.
fn archive_asset_entry(
    index: &crate::song_archive::ArchiveIndex,
    folder: &str,
    relative: &str,
    preview: bool,
) -> anyhow::Result<Option<String>> {
    let exists = |name: &str| {
        index.entries.iter().any(|candidate| !candidate.is_directory && candidate.name == name)
    };
    // A chart entry stands for its folder, like the parent of a chart file.
    let folder = if exists(folder) {
        folder.rsplit_once('/').map_or("", |(parent, _)| parent)
    } else {
        folder
    };
    let relative = relative.trim();
    if !relative.is_empty() {
        anyhow::ensure!(
            !relative.contains(':')
                && !relative.contains('\0')
                && !relative.starts_with(['/', '\\']),
            "unsafe archive asset: {relative}"
        );
        let name = crate::chart_source::normalize_entry(&format!("{folder}/{relative}"))?;
        if exists(&name) {
            return Ok(Some(name));
        }
        let (parent, file) = name.rsplit_once('/').map_or(("", name.as_str()), |split| split);
        if let Some(stem) = Path::new(file).file_stem().and_then(|stem| stem.to_str()) {
            let extensions =
                if preview { PREVIEW_AUDIO_EXTENSIONS } else { CHART_IMAGE_EXTENSIONS };
            for extension in extensions {
                for extension in [extension.to_string(), extension.to_ascii_uppercase()] {
                    let candidate = if parent.is_empty() {
                        format!("{stem}.{extension}")
                    } else {
                        format!("{parent}/{stem}.{extension}")
                    };
                    if exists(&candidate) {
                        return Ok(Some(candidate));
                    }
                }
            }
        }
    }
    if !preview {
        return Ok(None);
    }
    let prefix = if folder.is_empty() { String::new() } else { format!("{folder}/") };
    Ok(index
        .entries
        .iter()
        .filter(|candidate| !candidate.is_directory)
        .filter_map(|candidate| candidate.name.strip_prefix(&prefix).map(|name| (candidate, name)))
        .filter(|(_, name)| !name.contains('/') && is_preview_audio_file(Path::new(name)))
        .min_by_key(|(_, name)| preview_sort_key(Path::new(name)))
        .map(|(candidate, _)| candidate.name.clone()))
}

const CHART_IMAGE_EXTENSIONS: &[&str] = &["jpg", "jpeg", "gif", "bmp", "png", "tga"];
const PREVIEW_AUDIO_EXTENSIONS: &[&str] = &["wav", "ogg", "mp3", "flac"];

#[cfg(test)]
pub(crate) mod archive_tests;

/// BMS ヘッダで指定された相対パスを曲フォルダ基準で解決する。
pub fn resolve_chart_asset_path(folder_path: &str, relative: &str) -> Option<PathBuf> {
    let relative = relative.trim();
    if relative.is_empty() {
        return None;
    }
    let path = Path::new(relative);
    let resolved =
        if path.is_absolute() { path.to_path_buf() } else { Path::new(folder_path).join(path) };
    if resolved.is_file() {
        return Some(resolved);
    }
    resolve_same_stem_image_file(Path::new(folder_path), path)
}

pub fn normalize_preview_file(chart_path: &Path, preview_file: &str) -> String {
    let Some(folder) = chart_path.parent() else {
        return preview_file.trim().to_string();
    };
    resolve_preview_file(folder, preview_file)
        .and_then(|path| relative_to_folder(folder, &path))
        .unwrap_or_else(|| preview_file.trim().to_string())
}

pub fn resolve_preview_file(folder: &Path, preview_file: &str) -> Option<PathBuf> {
    let preview_file = preview_file.trim();
    if !preview_file.is_empty() {
        let path = Path::new(preview_file);
        let resolved = if path.is_absolute() { path.to_path_buf() } else { folder.join(path) };
        if resolved.is_file() {
            return Some(resolved);
        }
        if let Some(path) = resolve_same_stem_audio_file(folder, path) {
            return Some(path);
        }
    }
    find_preview_prefix_audio_file(folder)
}

fn resolve_same_stem_audio_file(folder: &Path, relative: &Path) -> Option<PathBuf> {
    let base = if relative.is_absolute() { relative.to_path_buf() } else { folder.join(relative) };
    let stem = base.file_stem()?.to_str()?;
    let parent = base.parent().unwrap_or(folder);
    for extension in PREVIEW_AUDIO_EXTENSIONS {
        let candidate = parent.join(format!("{stem}.{extension}"));
        if candidate.is_file() {
            return Some(candidate);
        }
        let candidate = parent.join(format!("{stem}.{}", extension.to_ascii_uppercase()));
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

fn resolve_same_stem_image_file(folder: &Path, relative: &Path) -> Option<PathBuf> {
    let base = if relative.is_absolute() { relative.to_path_buf() } else { folder.join(relative) };
    let stem = base.file_stem()?.to_str()?;
    let parent = base.parent().unwrap_or(folder);
    for extension in CHART_IMAGE_EXTENSIONS {
        let candidate = parent.join(format!("{stem}.{extension}"));
        if candidate.is_file() {
            return Some(candidate);
        }
        let candidate = parent.join(format!("{stem}.{}", extension.to_ascii_uppercase()));
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

fn find_preview_prefix_audio_file(folder: &Path) -> Option<PathBuf> {
    let mut candidates = Vec::new();
    for entry in std::fs::read_dir(folder).ok()? {
        let entry = entry.ok()?;
        let path = entry.path();
        if !path.is_file() || !is_preview_audio_file(&path) {
            continue;
        }
        candidates.push(path);
    }
    candidates.sort_by_key(|path| preview_sort_key(path));
    candidates.into_iter().next()
}

fn is_preview_audio_file(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
        return false;
    };
    if !name.to_ascii_lowercase().starts_with("preview") {
        return false;
    }
    let Some(extension) = path.extension().and_then(|extension| extension.to_str()) else {
        return false;
    };
    PREVIEW_AUDIO_EXTENSIONS.iter().any(|candidate| extension.eq_ignore_ascii_case(candidate))
}

fn preview_sort_key(path: &Path) -> (String, usize, String) {
    let stem =
        path.file_stem().and_then(|stem| stem.to_str()).unwrap_or_default().to_ascii_lowercase();
    let name =
        path.file_name().and_then(|name| name.to_str()).unwrap_or_default().to_ascii_lowercase();
    let extension = path.extension().and_then(|extension| extension.to_str()).unwrap_or_default();
    let extension_rank = PREVIEW_AUDIO_EXTENSIONS
        .iter()
        .position(|candidate| extension.eq_ignore_ascii_case(candidate))
        .unwrap_or(PREVIEW_AUDIO_EXTENSIONS.len());
    (stem, extension_rank, name)
}

fn relative_to_folder(folder: &Path, path: &Path) -> Option<String> {
    path.strip_prefix(folder).ok().map(|relative| relative.to_string_lossy().into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn resolves_relative_path_under_folder() {
        let dir = std::env::temp_dir().join(format!(
            "bmz-chart-asset-{}",
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        let image = dir.join("stage.png");
        fs::write(&image, b"png").unwrap();

        let got = resolve_chart_asset_path(dir.to_str().unwrap(), "stage.png");
        assert_eq!(got.as_deref(), Some(image.as_path()));
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn empty_or_missing_paths_return_none() {
        let dir = std::env::temp_dir().join(format!(
            "bmz-chart-asset-empty-{}",
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        assert!(resolve_chart_asset_path(dir.to_str().unwrap(), "").is_none());
        assert!(resolve_chart_asset_path(dir.to_str().unwrap(), "missing.png").is_none());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn resolves_chart_meta_image_by_same_stem_extension() {
        let dir = temp_dir("meta-image-extension");
        let stage = dir.join("stage.png");
        fs::write(&stage, b"png").unwrap();

        let got = resolve_chart_asset_path(dir.to_str().unwrap(), "stage.bmp");

        assert_eq!(got.as_deref(), Some(stage.as_path()));
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn resolves_chart_meta_images_from_bga_compat_fixture() {
        let root = repo_root().join("data/songs/bga-compat");

        assert_eq!(
            fixture_relative(resolve_chart_asset_path(root.to_str().unwrap(), "stage.bmp")),
            Some("stage.png".to_string())
        );
        assert_eq!(
            fixture_relative(resolve_chart_asset_path(root.to_str().unwrap(), "banner.jpg")),
            Some("banner.gif".to_string())
        );
        assert_eq!(
            fixture_relative(resolve_chart_asset_path(root.to_str().unwrap(), "back.bmp")),
            Some("back.tga".to_string())
        );
    }

    #[test]
    fn normalizes_preview_to_existing_audio_extension() {
        let dir = temp_dir("preview-extension");
        fs::write(dir.join("_Preview.ogg"), b"ogg").unwrap();

        let got = normalize_preview_file(&dir.join("song.bms"), "_Preview.wav");

        assert_eq!(got, "_Preview.ogg");
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn finds_preview_prefix_audio_when_header_is_empty() {
        let dir = temp_dir("preview-prefix");
        fs::write(dir.join("preview.ogg"), b"ogg").unwrap();

        let got = normalize_preview_file(&dir.join("song.bms"), "");

        assert_eq!(got, "preview.ogg");
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn ignores_non_prefix_preview_when_header_is_empty() {
        let dir = temp_dir("preview-prefix-only");
        fs::write(dir.join("_Preview.ogg"), b"ogg").unwrap();

        let got = normalize_preview_file(&dir.join("song.bms"), "");

        assert_eq!(got, "");
        fs::remove_dir_all(dir).unwrap();
    }

    fn temp_dir(label: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "bmz-chart-asset-{label}-{}",
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn repo_root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
    }

    fn fixture_relative(path: Option<PathBuf>) -> Option<String> {
        path.and_then(|path| {
            path.strip_prefix(repo_root().join("data/songs/bga-compat"))
                .ok()
                .map(|relative| relative.to_string_lossy().replace('\\', "/"))
        })
    }
}

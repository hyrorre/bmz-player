use std::path::{Path, PathBuf};

const CHART_IMAGE_EXTENSIONS: &[&str] = &["jpg", "jpeg", "gif", "bmp", "png", "tga"];
const PREVIEW_AUDIO_EXTENSIONS: &[&str] = &["wav", "ogg", "mp3", "flac"];

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

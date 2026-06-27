use std::path::{Path, PathBuf};

use crate::model::BgaAssetKind;

pub const BEATORAJA_BGA_VIDEO_EXTENSIONS: &[&str] =
    &["mp4", "wmv", "m4v", "webm", "mpg", "mpeg", "m1v", "m2v", "avi"];
pub const BMZ_EXTRA_BGA_VIDEO_EXTENSIONS: &[&str] = &["mkv", "mov"];
pub const BEATORAJA_BGA_IMAGE_EXTENSIONS: &[&str] = &["jpg", "jpeg", "gif", "bmp", "png", "tga"];

pub fn bga_asset_kind(path: &Path) -> BgaAssetKind {
    match path.extension().and_then(|extension| extension.to_str()) {
        Some(extension) if is_bga_video_extension(extension) => BgaAssetKind::Video,
        _ => BgaAssetKind::Static,
    }
}

pub fn is_bga_video_extension(extension: &str) -> bool {
    BEATORAJA_BGA_VIDEO_EXTENSIONS
        .iter()
        .chain(BMZ_EXTRA_BGA_VIDEO_EXTENSIONS.iter())
        .any(|candidate| extension.eq_ignore_ascii_case(candidate))
}

pub fn resolve_bga_asset_path(base_dir: &Path, declared: &Path) -> Option<PathBuf> {
    let base =
        if declared.is_absolute() { declared.to_path_buf() } else { base_dir.join(declared) };
    let extension = declared.extension().and_then(|extension| extension.to_str());

    if base.exists()
        && let Some(extension) = extension
    {
        if is_beatoraja_video_extension(extension) {
            if let Some(resolved) =
                resolve_same_stem_with_extensions(&base, BEATORAJA_BGA_VIDEO_EXTENSIONS)
            {
                return Some(resolved);
            }
        } else if is_beatoraja_image_extension(extension)
            && let Some(resolved) =
                resolve_same_stem_with_extensions(&base, BEATORAJA_BGA_IMAGE_EXTENSIONS)
        {
            return Some(resolved);
        } else {
            return Some(base);
        }
    }

    let stem_base = if extension.is_some() { strip_extension(&base).unwrap_or(base) } else { base };
    resolve_same_stem_with_extensions(&stem_base, BEATORAJA_BGA_VIDEO_EXTENSIONS)
        .or_else(|| resolve_same_stem_with_extensions(&stem_base, BMZ_EXTRA_BGA_VIDEO_EXTENSIONS))
        .or_else(|| resolve_same_stem_with_extensions(&stem_base, BEATORAJA_BGA_IMAGE_EXTENSIONS))
}

fn is_beatoraja_video_extension(extension: &str) -> bool {
    BEATORAJA_BGA_VIDEO_EXTENSIONS.iter().any(|candidate| extension.eq_ignore_ascii_case(candidate))
}

fn is_beatoraja_image_extension(extension: &str) -> bool {
    BEATORAJA_BGA_IMAGE_EXTENSIONS.iter().any(|candidate| extension.eq_ignore_ascii_case(candidate))
}

fn strip_extension(path: &Path) -> Option<PathBuf> {
    let stem = path.file_stem()?;
    let parent = path.parent();
    Some(match parent {
        Some(parent) => parent.join(stem),
        None => PathBuf::from(stem),
    })
}

fn resolve_same_stem_with_extensions(
    base_without_extension: &Path,
    extensions: &[&str],
) -> Option<PathBuf> {
    for extension in extensions {
        let candidate = base_without_extension.with_extension(extension);
        if candidate.exists() {
            return Some(candidate);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_declared_image_to_same_stem_image_by_beatoraja_order() {
        let dir = temp_dir("image-order");
        std::fs::write(dir.join("bga.png"), b"png").unwrap();

        let resolved = resolve_bga_asset_path(&dir, Path::new("bga.bmp"));

        assert_eq!(resolved.as_deref(), Some(dir.join("bga.png").as_path()));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn resolves_declared_video_to_same_stem_video_by_beatoraja_order() {
        let dir = temp_dir("video-order");
        std::fs::write(dir.join("movie.mp4"), b"mp4").unwrap();
        std::fs::write(dir.join("movie.avi"), b"avi").unwrap();

        let resolved = resolve_bga_asset_path(&dir, Path::new("movie.mpg"));

        assert_eq!(resolved.as_deref(), Some(dir.join("movie.mp4").as_path()));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn resolves_missing_declared_path_to_video_before_image() {
        let dir = temp_dir("missing-video-first");
        std::fs::write(dir.join("asset.png"), b"png").unwrap();
        std::fs::write(dir.join("asset.webm"), b"webm").unwrap();

        let resolved = resolve_bga_asset_path(&dir, Path::new("asset.bmp"));

        assert_eq!(resolved.as_deref(), Some(dir.join("asset.webm").as_path()));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn classifies_beatoraja_video_extensions() {
        for extension in ["m4v", "webm", "m1v", "m2v"] {
            assert_eq!(
                bga_asset_kind(Path::new(&format!("movie.{extension}"))),
                BgaAssetKind::Video
            );
        }
    }

    #[test]
    fn keeps_bmz_extra_video_extensions() {
        assert_eq!(bga_asset_kind(Path::new("movie.mkv")), BgaAssetKind::Video);
        assert_eq!(bga_asset_kind(Path::new("movie.mov")), BgaAssetKind::Video);
    }

    fn temp_dir(label: &str) -> PathBuf {
        let stamp =
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let dir = std::env::temp_dir().join(format!("bmz-bga-asset-{label}-{stamp}"));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }
}

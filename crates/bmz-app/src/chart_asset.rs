use std::path::{Path, PathBuf};

/// BMS ヘッダで指定された相対パスを曲フォルダ基準で解決する。
pub fn resolve_chart_asset_path(folder_path: &str, relative: &str) -> Option<PathBuf> {
    let relative = relative.trim();
    if relative.is_empty() {
        return None;
    }
    let path = Path::new(relative);
    let resolved =
        if path.is_absolute() { path.to_path_buf() } else { Path::new(folder_path).join(path) };
    resolved.is_file().then_some(resolved)
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
}

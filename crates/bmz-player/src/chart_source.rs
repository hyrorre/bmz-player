//! Stable song locations are independent of disposable materialized archive paths.

use std::path::{Component, Path, PathBuf};

use anyhow::{Context, Result, bail, ensure};
use bmz_chart::model::PlayableChart;

use crate::song_archive::{self, ArchiveControl, ArchiveGeneration};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ChartLocator {
    File(PathBuf),
    Archive { container: PathBuf, entry: String },
}

impl ChartLocator {
    /// Accept archive!/entry and archive! directory roots. Entries may also name assets.
    pub fn parse(path: &Path) -> Result<Self> {
        let text = crate::paths::normalize_library_path(&path.to_string_lossy());
        let lower = text.to_ascii_lowercase();
        for (index, _) in text.match_indices('!') {
            if !text[index + 1..].is_empty() && !text[index + 1..].starts_with('/') {
                continue;
            }
            if ![".zip", ".rar", ".7z"].iter().any(|ext| lower[..index].ends_with(ext)) {
                continue;
            }
            let entry = text[index + 1..].strip_prefix('/').unwrap_or("");
            return Ok(Self::Archive {
                container: PathBuf::from(&text[..index]),
                entry: normalize_entry(entry)?,
            });
        }
        Ok(Self::File(PathBuf::from(text)))
    }

    pub fn canonicalize(&self) -> Result<Self> {
        let path = self
            .container_path()
            .canonicalize()
            .with_context(|| format!("resolve song source: {}", self.container_path().display()))?;
        let path = PathBuf::from(crate::paths::normalize_library_path(&path.to_string_lossy()));
        Ok(match self {
            Self::File(_) => Self::File(path),
            Self::Archive { entry, .. } => Self::Archive { container: path, entry: entry.clone() },
        })
    }

    pub fn to_path_buf(&self) -> PathBuf {
        match self {
            Self::File(path) => path.clone(),
            Self::Archive { container, entry } => {
                let container = crate::paths::normalize_library_path(&container.to_string_lossy());
                PathBuf::from(if entry.is_empty() {
                    format!("{container}!")
                } else {
                    format!("{container}!/{entry}")
                })
            }
        }
    }

    pub fn container_path(&self) -> &Path {
        match self {
            Self::File(path) => path,
            Self::Archive { container, .. } => container,
        }
    }

    pub fn is_archive(&self) -> bool {
        matches!(self, Self::Archive { .. })
    }

    pub fn readable(&self) -> bool {
        match self {
            Self::File(path) => path.is_file() && std::fs::File::open(path).is_ok(),
            Self::Archive { container, entry } => {
                song_archive::inspect(container, &ArchiveControl::default()).is_ok_and(|index| {
                    index
                        .entries
                        .iter()
                        .any(|candidate| !candidate.is_directory && candidate.name == *entry)
                })
            }
        }
    }

    pub fn read_bytes(&self) -> Result<Vec<u8>> {
        match self {
            Self::File(path) => {
                std::fs::read(path).with_context(|| format!("read chart: {}", path.display()))
            }
            Self::Archive { container, entry } => {
                Ok(song_archive::read_chart(container, entry, &ArchiveControl::default())?.bytes)
            }
        }
    }

    /// No default cache location: callers must explicitly supply their AppPaths/test root.
    pub fn materialize(&self, cache_root: &Path) -> Result<ResolvedChartSource> {
        match self {
            Self::File(path) => Ok(ResolvedChartSource {
                locator: self.clone(),
                path: path.clone(),
                archive_root: None,
                generation: None,
            }),
            Self::Archive { container, entry } => {
                let control = ArchiveControl::default();
                let index = song_archive::inspect(container, &control)?;
                let materialized =
                    song_archive::materialize(container, &index.generation, cache_root, &control)?;
                let resolved = ResolvedChartSource {
                    locator: self.clone(),
                    path: materialized.root.join(entry),
                    archive_root: Some(materialized.root),
                    generation: Some(materialized.generation),
                };
                resolved.validate_asset_path(&resolved.path)?;
                Ok(resolved)
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct ResolvedChartSource {
    pub locator: ChartLocator,
    pub path: PathBuf,
    pub archive_root: Option<PathBuf>,
    pub generation: Option<ArchiveGeneration>,
}

impl ResolvedChartSource {
    /// Resolve relative to a materialized directory or the parent of a chart/asset file.
    pub fn resolve_asset(&self, relative: &str) -> Result<PathBuf> {
        if self.archive_root.is_some() {
            ensure!(
                !relative.contains(':')
                    && !relative.contains('\0')
                    && !relative.starts_with(['/', '\\']),
                "unsafe archive asset: {relative}"
            );
        }
        let base = if self.path.is_dir() {
            self.path.as_path()
        } else {
            self.path.parent().unwrap_or(Path::new(""))
        };
        let path = base.join(relative.replace('\\', "/"));
        self.validate_asset_path(&path)?;
        Ok(path)
    }

    pub fn validate_asset_path(&self, path: &Path) -> Result<()> {
        let Some(root) = &self.archive_root else {
            return Ok(());
        };
        let root = lexical_absolute(root)?;
        let path = lexical_absolute(path)?;
        ensure!(path.starts_with(&root), "song asset escapes archive root: {}", path.display());
        ensure!(
            !path.strip_prefix(&root)?.to_string_lossy().contains(':'),
            "unsafe archive asset: {}",
            path.display()
        );
        // Also reject a replaced cache path containing a symlink/junction. Missing assets
        // are allowed here so normal extension fallback and missing-resource warnings work.
        let canonical_root = root.canonicalize().context("resolve archive cache root")?;
        let mut existing = path.as_path();
        while !existing.try_exists()? {
            existing = existing.parent().context("archive asset has no existing ancestor")?;
        }
        ensure!(
            existing.canonicalize()?.starts_with(canonical_root),
            "song asset escapes archive cache through a link: {}",
            path.display()
        );
        Ok(())
    }

    pub fn validate_chart_assets(&self, chart: &PlayableChart) -> Result<()> {
        if self.archive_root.is_none() {
            return Ok(());
        }
        for path in chart
            .sounds
            .iter()
            .map(|sound| &sound.path)
            .chain(chart.bga_assets.iter().map(|asset| &asset.path))
        {
            self.validate_asset_path(path)?;
        }
        // Audio decode can fall back to another extension after the declared path.
        for sound in &chart.sounds {
            for candidate in bmz_chart::sound_asset::sound_asset_candidates(&sound.path) {
                self.validate_asset_path(&candidate)?;
            }
        }
        for relative in [
            &chart.metadata.stage_file,
            &chart.metadata.backbmp_file,
            &chart.metadata.banner_file,
            &chart.metadata.preview_file,
        ] {
            if !relative.is_empty() {
                self.resolve_asset(relative)?;
            }
        }
        Ok(())
    }
}

pub(crate) fn is_archive_file(path: &Path) -> bool {
    path.extension().and_then(|ext| ext.to_str()).is_some_and(|ext| {
        ["zip", "rar", "7z"].iter().any(|candidate| ext.eq_ignore_ascii_case(candidate))
    })
}

pub(crate) fn archive_folder_has_document(index: &song_archive::ArchiveIndex, entry: &str) -> bool {
    let folder = Path::new(entry).parent();
    index.entries.iter().any(|candidate| {
        !candidate.is_directory
            && Path::new(&candidate.name).parent() == folder
            && Path::new(&candidate.name)
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("txt"))
    })
}

pub(crate) fn archive_preview_file(
    index: &song_archive::ArchiveIndex,
    entry: &str,
    preview: &str,
) -> String {
    let folder = Path::new(entry).parent().unwrap_or(Path::new(""));
    let find = |relative: &str| -> Option<String> {
        let joined = folder.join(relative.replace('\\', "/"));
        let name = normalize_entry(&joined.to_string_lossy()).ok()?;
        index
            .entries
            .iter()
            .any(|candidate| !candidate.is_directory && candidate.name == name)
            .then(|| relative.to_string())
    };
    if !preview.trim().is_empty() {
        if let Some(found) = find(preview.trim()) {
            return found;
        }
        let path = Path::new(preview.trim());
        for ext in ["wav", "ogg", "mp3", "flac"] {
            for extension in [ext.to_string(), ext.to_uppercase()] {
                if let Some(found) = find(&path.with_extension(extension).to_string_lossy()) {
                    return found;
                }
            }
        }
    }
    let mut candidates = index
        .entries
        .iter()
        .filter(|candidate| {
            let path = Path::new(&candidate.name);
            !candidate.is_directory
                && path.parent() == Some(folder)
                && path.file_name().is_some_and(|name| {
                    name.to_string_lossy().to_ascii_lowercase().starts_with("preview")
                })
                && path.extension().is_some_and(|ext| {
                    ["wav", "ogg", "mp3", "flac"]
                        .iter()
                        .any(|candidate| ext.eq_ignore_ascii_case(candidate))
                })
        })
        .filter_map(|candidate| Path::new(&candidate.name).file_name())
        .collect::<Vec<_>>();
    candidates.sort();
    candidates
        .first()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| preview.trim().to_string())
}

pub(crate) fn normalize_entry(entry: &str) -> Result<String> {
    let entry = entry.replace('\\', "/");
    ensure!(
        !entry.starts_with('/') && !entry.contains(':') && !entry.contains('\0'),
        "unsafe archive entry: {entry}"
    );
    let mut components = Vec::new();
    for component in entry.split('/') {
        match component {
            "" | "." => {}
            ".." => {
                ensure!(components.pop().is_some(), "archive entry escapes root: {entry}");
            }
            component => components.push(component),
        }
    }
    Ok(components.join("/"))
}

fn lexical_absolute(path: &Path) -> Result<PathBuf> {
    let absolute = std::path::absolute(path)?;
    let mut clean = PathBuf::new();
    for component in absolute.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                if !clean.pop() {
                    bail!("path escapes filesystem root: {}", path.display());
                }
            }
            other => clean.push(other.as_os_str()),
        }
    }
    Ok(clean)
}

#[cfg(test)]
mod tests;

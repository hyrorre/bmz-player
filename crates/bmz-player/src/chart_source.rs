//! Stable song locations. Archive charts keep virtual `archive!/entry` paths and read their
//! assets from memory, so no extracted path ever identifies a song.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail, ensure};
use bmz_chart::model::PlayableChart;

use crate::song_archive::{self, ArchiveControl};

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

    /// Opens the chart with a runtime virtual path bound to its resource set.
    /// The stable locator stays unchanged; nothing is extracted to disk.
    pub fn resolve(&self) -> Result<ResolvedChartSource> {
        match self {
            Self::File(path) => {
                Ok(ResolvedChartSource { locator: self.clone(), path: path.clone(), archive: None })
            }
            Self::Archive { container, entry } => {
                let opened = crate::archive_resources::open_chart(
                    container,
                    entry,
                    &ArchiveControl::default(),
                )?;
                Ok(ResolvedChartSource {
                    locator: self.clone(),
                    path: opened.resources.path(entry),
                    archive: Some(opened),
                })
            }
        }
    }
}

pub struct ResolvedChartSource {
    pub locator: ChartLocator,
    /// The chart file, or a runtime virtual path pinned to its archive resource set.
    /// Persist `locator`, never this runtime path.
    pub path: PathBuf,
    archive: Option<crate::archive_resources::OpenedChart>,
}

impl std::fmt::Debug for ResolvedChartSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ResolvedChartSource")
            .field("locator", &self.locator)
            .field("path", &self.path)
            .field("resources", &self.resources())
            .finish()
    }
}

impl ResolvedChartSource {
    /// The in-memory archive entries behind the virtual paths. The play keeps this alive.
    pub fn resources(&self) -> Option<&std::sync::Arc<bmz_song_resource::SongResourceSet>> {
        self.archive.as_ref().map(|opened| &opened.resources)
    }

    /// Resolve relative to the chart's folder (or a folder locator itself), inside its
    /// archive for archive charts.
    pub fn resolve_asset(&self, relative: &str) -> Result<PathBuf> {
        if self.archive.is_some() {
            ensure!(
                !relative.contains(':')
                    && !relative.contains('\0')
                    && !relative.starts_with(['/', '\\']),
                "unsafe archive asset: {relative}"
            );
        }
        let folder = match (&self.locator, &self.archive) {
            (ChartLocator::Archive { entry, .. }, Some(opened)) => {
                !opened.resources.contains(entry)
            }
            _ => self.path.is_dir(),
        };
        let base =
            if folder { self.path.as_path() } else { self.path.parent().unwrap_or(Path::new("")) };
        let path = base.join(relative.replace('\\', "/"));
        self.validate_asset_path(&path)?;
        Ok(path)
    }

    /// Archive chart assets must stay inside their own archive.
    pub fn validate_asset_path(&self, path: &Path) -> Result<()> {
        let ChartLocator::Archive { container, .. } = &self.locator else {
            return Ok(());
        };
        match bmz_song_resource::SongPath::parse(path) {
            bmz_song_resource::SongPath::Archive { container: parsed, .. }
                if parsed == crate::paths::normalize_library_path(&container.to_string_lossy()) =>
            {
                Ok(())
            }
            _ => bail!("song asset escapes archive root: {}", path.display()),
        }
    }

    pub fn validate_chart_assets(&self, chart: &PlayableChart) -> Result<()> {
        if self.archive.is_none() {
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

    /// Reads the archive entries `chart` uses into memory. Files need nothing.
    pub fn load_chart_assets(&self, chart: &PlayableChart) -> Result<()> {
        match &self.archive {
            Some(opened) => crate::archive_resources::load_chart_assets(
                opened,
                &self.path,
                chart,
                &ArchiveControl::default(),
            ),
            None => Ok(()),
        }
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

#[cfg(test)]
mod tests;

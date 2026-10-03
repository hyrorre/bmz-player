//! Read-only skin assets, including LR2's folder-shaped legacy DX archives.
//! No files are extracted. Callers enforce their roots with `resolve_with`.

use std::fs;
use std::path::{Component, Path, PathBuf};

use anyhow::{Context, Result, bail};

mod dxa;

/// A physical file or a file inside a DX archive. The logical identity includes
/// the entry name; the backing file supplies metadata for cache invalidation.
#[derive(Debug, Clone)]
pub struct SkinAsset {
    logical: PathBuf,
    backing: PathBuf,
    entry: Option<dxa::Entry>,
}

impl SkinAsset {
    pub fn resolve(path: &Path) -> Result<Self> {
        Self::resolve_with(path, |_| true)
    }

    /// Validate the canonical backing file before reading any archive bytes.
    /// Physical files take precedence over entries in same-named archives.
    pub fn resolve_with(path: &Path, mut allowed: impl FnMut(&Path) -> bool) -> Result<Self> {
        let normalized = ordinary_windows_path(path);
        let path = normalized.as_path();
        let physical = case_insensitive_path(path);
        if physical.is_file() {
            let backing = fs::canonicalize(&physical)?;
            if !allowed(&backing) {
                bail!("skin asset escapes allowed roots: {}", backing.display());
            }
            return Ok(Self { logical: backing.clone(), backing, entry: None });
        }
        for directory in path.ancestors().skip(1) {
            let Some(name) = directory.file_name() else { continue };
            let mut archive_name = name.to_os_string();
            archive_name.push(".dxa");
            let archive_path = case_insensitive_path(&directory.with_file_name(archive_name));
            if !archive_path.is_file() {
                continue;
            }
            let backing = fs::canonicalize(archive_path)?;
            if !allowed(&backing) {
                bail!("skin archive escapes allowed roots: {}", backing.display());
            }
            let relative = path.strip_prefix(directory)?;
            let requested = entry_name(relative)?;
            let entry = dxa::find_entry(&backing, &requested)
                .with_context(|| format!("failed to resolve DXA asset: {}", path.display()))?;
            let logical = backing.with_extension("").join(&entry.name);
            return Ok(Self { logical, backing, entry: Some(entry) });
        }
        bail!("skin asset not found: {}", path.display())
    }

    pub fn logical_path(&self) -> &Path {
        &self.logical
    }

    pub fn backing_path(&self) -> &Path {
        &self.backing
    }

    pub fn is_archived(&self) -> bool {
        self.entry.is_some()
    }

    pub fn read(&self) -> Result<Vec<u8>> {
        match &self.entry {
            Some(entry) => dxa::read_entry(&self.backing, entry),
            None => fs::read(&self.backing)
                .with_context(|| format!("failed to read skin asset: {}", self.backing.display())),
        }
    }
}

// A virtual path may be built by joining a slash-separated font page to a
// canonical Windows path. Verbatim paths disable mixed-separator handling.
fn ordinary_windows_path(path: &Path) -> PathBuf {
    #[cfg(windows)]
    {
        let text = path.to_string_lossy();
        if let Some(rest) = text.strip_prefix(r"\\?\UNC\") {
            return PathBuf::from(format!(r"\\{rest}"));
        }
        if let Some(rest) = text.strip_prefix(r"\\?\")
            && rest.as_bytes().get(1) == Some(&b':')
        {
            return PathBuf::from(rest);
        }
    }
    path.to_path_buf()
}

pub fn read(path: &Path) -> Result<Vec<u8>> {
    SkinAsset::resolve(path)?.read()
}

pub fn is_file(path: &Path) -> bool {
    SkinAsset::resolve(path).is_ok()
}

fn entry_name(path: &Path) -> Result<String> {
    let mut parts = Vec::new();
    for part in path.components() {
        let Component::Normal(part) = part else {
            bail!("invalid DXA entry path: {}", path.display())
        };
        let part = part.to_str().context("non-Unicode DXA entry path")?;
        dxa::validate_name(part)?;
        parts.push(part);
    }
    if parts.is_empty() {
        bail!("empty DXA entry path");
    }
    Ok(parts.join("/"))
}

/// LR2 packages commonly differ in ASCII case between CSV and disk names.
fn case_insensitive_path(path: &Path) -> PathBuf {
    if path.exists() {
        return path.to_path_buf();
    }
    let Some(parent) = path.parent() else { return path.to_path_buf() };
    let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
        return path.to_path_buf();
    };
    let parent = case_insensitive_path(parent);
    fs::read_dir(&parent)
        .ok()
        .into_iter()
        .flatten()
        .flatten()
        .find(|entry| {
            entry.file_name().to_str().is_some_and(|candidate| candidate.eq_ignore_ascii_case(name))
        })
        .map(|entry| entry.path())
        .unwrap_or_else(|| parent.join(name))
}

#[cfg(test)]
mod tests;

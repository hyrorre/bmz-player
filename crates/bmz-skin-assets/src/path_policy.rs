use crate::{
    canonicalize_skin_path,
    path_context::{lexical_normalize, simplify_verbatim_path},
};
use anyhow::{Context, Result, bail};
use std::path::{Path, PathBuf};

/// Host-supplied immutable authority shared by every skin format and traversal.
/// Resolving a child never adds a root.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkinPathPolicy {
    roots: Vec<PathBuf>,
    aliases: Vec<(PathBuf, PathBuf)>,
}

impl SkinPathPolicy {
    pub fn new(roots: impl IntoIterator<Item = PathBuf>) -> Result<Self> {
        let mut policy = Self { roots: Vec::new(), aliases: Vec::new() };
        for root in roots {
            if root.as_os_str().is_empty() {
                bail!("empty skin root");
            }
            let logical = lexical_normalize(&simplify_verbatim_path(std::path::absolute(&root)?))?;
            let canonical = match canonicalize_skin_path(&root) {
                Ok(path) => path,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                Err(error) => {
                    return Err(error).with_context(|| {
                        format!("failed to resolve skin root: {}", root.display())
                    });
                }
            };
            if !canonical.is_dir() {
                bail!("skin root is not a directory: {}", root.display());
            }
            if !policy.roots.contains(&canonical) {
                policy.roots.push(canonical.clone());
            }
            policy.aliases.push((logical, canonical));
        }
        policy.aliases.sort_by_key(|(path, _)| std::cmp::Reverse(path.components().count()));
        Ok(policy)
    }

    pub fn roots(&self) -> &[PathBuf] {
        &self.roots
    }

    /// Preserve explicitly configured aliases when passing authority to a new
    /// loader. Canonical roots alone would lose valid root-link references.
    pub fn root_paths(&self) -> Vec<PathBuf> {
        let mut paths = self.roots.clone();
        for (alias, _) in &self.aliases {
            if !paths.contains(alias) {
                paths.push(alias.clone());
            }
        }
        paths
    }

    pub fn lexical_candidate(&self, path: &Path) -> Result<PathBuf> {
        if path.as_os_str().is_empty() || path.to_string_lossy().contains('\0') {
            bail!("empty or invalid skin path");
        }
        let path = lexical_normalize(&simplify_verbatim_path(std::path::absolute(path)?))?;
        for (alias, root) in &self.aliases {
            if let Ok(relative) = path.strip_prefix(alias) {
                return Ok(root.join(relative));
            }
        }
        if self.roots.iter().any(|root| path.starts_with(root)) {
            return Ok(path);
        }
        bail!(
            "skin path escapes skin root: {}; explicitly select this external skin or configure its allowed roots",
            path.display()
        )
    }

    pub fn resolve(&self, path: &Path) -> Result<PathBuf> {
        // Reject unknown absolute/network paths before touching them.
        let candidate = self.lexical_candidate(path)?;
        let canonical = canonicalize_skin_path(&candidate).with_context(|| {
            format!(
                "failed to resolve skin path (missing, broken or cyclic link): {}",
                candidate.display()
            )
        })?;
        if !self.roots.iter().any(|root| canonical.starts_with(root)) {
            bail!(
                "skin link escapes skin root: {} -> {}",
                candidate.display(),
                canonical.display()
            );
        }
        Ok(canonical)
    }

    /// Names for the memory overlay, never permission to create disk files.
    pub fn resolve_or_missing(&self, path: &Path) -> Result<PathBuf> {
        let candidate = self.lexical_candidate(path)?;
        let mut existing = candidate.as_path();
        let mut tail = Vec::new();
        loop {
            match std::fs::symlink_metadata(existing) {
                Ok(_) => break,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    tail.push(
                        existing
                            .file_name()
                            .context("skin path has no existing parent")?
                            .to_os_string(),
                    );
                    existing = existing.parent().context("skin path has no existing parent")?;
                }
                Err(error) => return Err(error).context("failed to resolve skin path parent"),
            }
        }
        let mut resolved = self.resolve(existing)?;
        for part in tail.into_iter().rev() {
            resolved.push(part);
        }
        Ok(resolved)
    }
}

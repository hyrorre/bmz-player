use std::collections::BTreeSet;
use std::fs;
use std::path::{Component, Path, PathBuf};

use anyhow::{Context, Result, anyhow, bail};

/// A package-aware, sandboxed path context shared by Lua loading and the app's
/// asset decoder.
///
/// `entry_dir` preserves the traditional relative-path base, while
/// `library_roots` are the explicitly configured directories that contain skin
/// packages (for example `<data>/skins`). Every resolved existing path is
/// canonicalized and must remain below one of these roots.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkinPathContext {
    entry_file: PathBuf,
    entry_dir: PathBuf,
    library_roots: Vec<PathBuf>,
}

impl SkinPathContext {
    /// Builds a context from an entry skin and explicit skin-library roots.
    ///
    /// Missing library roots are ignored. If the entry is outside every
    /// configured library, its own directory becomes the sole safety boundary;
    /// this keeps external/legacy skins working without granting sibling access.
    pub fn new(
        entry_file: &Path,
        library_roots: impl IntoIterator<Item = PathBuf>,
    ) -> Result<Self> {
        let entry_file = canonicalize_skin_path(entry_file).with_context(|| {
            format!("failed to canonicalize skin entry: {}", entry_file.display())
        })?;
        let entry_dir = entry_file
            .parent()
            .ok_or_else(|| anyhow!("skin entry has no parent: {}", entry_file.display()))?
            .to_path_buf();

        let mut seen = BTreeSet::new();
        let mut roots = Vec::new();
        for root in library_roots {
            let Ok(root) = canonicalize_skin_path(&root) else {
                continue;
            };
            if root.is_dir() && seen.insert(root.clone()) {
                roots.push(root);
            }
        }

        if roots.iter().any(|root| entry_file.starts_with(root)) {
            roots.sort_by_key(|root| !entry_file.starts_with(root));
        } else {
            roots.clear();
            roots.push(entry_dir.clone());
        }

        Ok(Self { entry_file, entry_dir, library_roots: roots })
    }

    /// Compatibility context for low-level callers that do not know the app's
    /// configured skin library roots.
    pub fn for_entry(entry_file: &Path) -> Result<Self> {
        Self::new(entry_file, std::iter::empty())
    }

    pub fn entry_file(&self) -> &Path {
        &self.entry_file
    }

    pub fn entry_dir(&self) -> &Path {
        &self.entry_dir
    }

    pub fn library_roots(&self) -> &[PathBuf] {
        &self.library_roots
    }

    /// Deterministic sandbox package path. No host Lua paths or native module
    /// paths are inherited by the VM.
    ///
    /// Normal app loads expose the same logical shape as beatoraja's runtime
    /// loader (`?.lua;skin/<entry-directory>/?.lua`). Some established skins
    /// inspect that string to recover their installed package or extension
    /// directory. Physical paths are still resolved and sandboxed by this
    /// context rather than by Lua itself.
    ///
    /// Low-level callers that only provide the entry directory as their safety
    /// root have no portable `skin/...` alias to expose, so retain the absolute
    /// entry-local templates for those callers.
    pub fn initial_package_path(&self) -> String {
        if let Some(relative) = self.library_roots.iter().find_map(|root| {
            self.entry_dir
                .strip_prefix(root)
                .ok()
                .filter(|relative| !relative.as_os_str().is_empty())
        }) {
            let relative = relative.to_string_lossy().replace('\\', "/");
            return format!("?.lua;skin/{relative}/?.lua");
        }
        let entry = self.entry_dir.to_string_lossy().replace('\\', "/");
        format!("{entry}/?.lua;{entry}/?/init.lua")
    }

    /// Resolves an existing regular file using the common path rules.
    pub fn resolve_file(&self, requested: &str) -> Result<PathBuf> {
        self.resolve_existing(requested, ExistingKind::File)
    }

    /// Resolve a static image/font, including legacy DXA virtual folders.
    /// Lua modules and ordinary file APIs continue to require physical files.
    pub fn resolve_asset(&self, requested: &str) -> Result<PathBuf> {
        let candidates = self.candidate_paths(requested)?;
        let mut failure = None;
        for candidate in candidates {
            match bmz_skin_assets::SkinAsset::resolve_with(&candidate, |backing| {
                // Normalize Windows extended prefixes exactly as other paths.
                canonicalize_skin_path(backing)
                    .is_ok_and(|path| self.library_roots.iter().any(|root| path.starts_with(root)))
            }) {
                Ok(asset) => return Ok(simplify_verbatim_path(asset.logical_path().to_path_buf())),
                Err(error) => failure = Some(error),
            }
        }
        Err(failure.unwrap_or_else(|| anyhow!("skin asset not found: {requested}")))
    }

    /// Resolves an existing file or directory using the common path rules.
    pub fn resolve_path(&self, requested: &str) -> Result<PathBuf> {
        self.resolve_existing(requested, ExistingKind::Any)
    }

    /// Resolves a path that may not exist yet using the common sandbox rules.
    ///
    /// Existing paths are still canonicalized so symlink escapes are rejected.
    /// For a missing path, the returned lexical path is only a name inside the
    /// sandbox; callers must use an existing-path resolver before reading it.
    pub fn resolve_path_or_missing(&self, requested: &str) -> Result<PathBuf> {
        let candidates = self.candidate_paths(requested)?;
        for candidate in &candidates {
            if candidate.exists() {
                return self.validate_existing(candidate, ExistingKind::Any);
            }
        }
        candidates
            .into_iter()
            .next()
            .ok_or_else(|| anyhow!("skin path has no sandboxed candidate: {requested}"))
    }

    /// Resolves an existing directory using the common path rules.
    pub fn resolve_directory(&self, requested: &str) -> Result<PathBuf> {
        self.resolve_existing(requested, ExistingKind::Directory)
    }

    /// Resolves one `package.path` template for a module name.
    ///
    /// The caller is responsible for trying templates in their Lua-visible
    /// order. `Ok(None)` means this template had no matching regular file.
    pub fn resolve_package_candidate(
        &self,
        template: &str,
        module_name: &str,
    ) -> Result<Option<PathBuf>> {
        if template.contains('\0') || module_name.contains('\0') {
            bail!("Lua module path contains NUL");
        }
        let module_path = module_name.replace(['.', '\\'], "/");
        let requested = template.replace('\\', "/").replace('?', &module_path);
        let candidates = self.candidate_paths(&requested)?;
        for candidate in candidates {
            if !candidate.is_file() {
                continue;
            }
            return self.validate_existing(&candidate, ExistingKind::File).map(Some);
        }
        Ok(None)
    }

    /// Enumerates a single-wildcard path below the sandbox roots.
    pub fn wildcard_candidates(&self, requested: &str) -> Result<Vec<PathBuf>> {
        let requested = strip_beatoraja_asset_filter(requested).replace('\\', "/");
        if requested.contains('\0') {
            bail!("skin wildcard path contains NUL");
        }
        let Some((prefix, suffix)) = requested.split_once('*') else {
            return self.resolve_path(&requested).map(|path| vec![path]);
        };
        if suffix.contains('*') {
            bail!("skin paths support only one wildcard: {requested}");
        }

        let slash = prefix.rfind('/').map(|index| index + 1).unwrap_or(0);
        let (directory_request, name_prefix) = prefix.split_at(slash);
        let directory_request = directory_request.trim_end_matches('/');
        let directories = self.existing_candidates(
            if directory_request.is_empty() { "." } else { directory_request },
            ExistingKind::Directory,
        )?;
        let suffix = suffix.trim_start_matches('/');
        let nested =
            !suffix.is_empty() && requested.as_bytes().get(prefix.len() + 1) == Some(&b'/');
        let mut resolved = Vec::new();
        let mut seen = BTreeSet::new();

        for directory in directories {
            let mut group = Vec::new();
            let Ok(entries) = fs::read_dir(&directory) else {
                continue;
            };
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if !name.starts_with(name_prefix) {
                    continue;
                }
                let candidate = if nested {
                    entry.path().join(suffix)
                } else {
                    if !name.ends_with(suffix) {
                        continue;
                    }
                    entry.path()
                };
                if !candidate.exists() {
                    continue;
                }
                let canonical = self.validate_existing(&candidate, ExistingKind::Any)?;
                if seen.insert(canonical.clone()) {
                    group.push(canonical);
                }
            }
            group.sort();
            resolved.extend(group);
        }
        Ok(resolved)
    }

    /// Returns portable selection strings for the physical wildcard matches.
    /// The wildcard is replaced in the original logical pattern, so profile
    /// settings do not need to store machine-specific absolute paths.
    pub fn wildcard_candidate_values(&self, requested: &str) -> Result<Vec<String>> {
        let pattern = strip_beatoraja_asset_filter(requested).replace('\\', "/");
        let Some((prefix, suffix)) = pattern.split_once('*') else {
            self.resolve_path(&pattern)?;
            return Ok(vec![pattern]);
        };
        if suffix.contains('*') {
            bail!("skin paths support only one wildcard: {requested}");
        }
        let candidates = self.wildcard_candidates(&pattern)?;
        let mut values = Vec::with_capacity(candidates.len());
        for candidate in candidates {
            let wildcard = if let Some(nested_suffix) = suffix.strip_prefix('/') {
                let suffix_components = Path::new(nested_suffix).components().count();
                let wildcard_path = (0..suffix_components)
                    .try_fold(candidate.as_path(), |path, _| path.parent())
                    .and_then(Path::file_name)
                    .and_then(|name| name.to_str());
                wildcard_path.map(str::to_string)
            } else {
                let name_prefix = prefix.rsplit('/').next().unwrap_or(prefix);
                candidate
                    .file_name()
                    .and_then(|name| name.to_str())
                    .and_then(|name| name.strip_prefix(name_prefix))
                    .and_then(|name| name.strip_suffix(suffix))
                    .map(str::to_string)
            };
            let Some(wildcard) = wildcard else { continue };
            values.push(format!("{prefix}{wildcard}{suffix}"));
        }
        values.sort();
        values.dedup();
        Ok(values)
    }

    /// Resolves a saved file choice either relative to the directory containing
    /// the pattern's wildcard or as its own explicit path.
    pub fn resolve_selected_for_pattern(&self, pattern: &str, selected: &str) -> Option<PathBuf> {
        let pattern = strip_beatoraja_asset_filter(pattern).replace('\\', "/");
        let selected = selected.replace('\\', "/");
        let selected_is_basename = !selected.contains('/') && !Path::new(&selected).is_absolute();
        if selected_is_basename && let Some(star) = pattern.find('*') {
            let prefix = &pattern[..star];
            let slash = prefix.rfind('/').map(|index| index + 1).unwrap_or(0);
            let directory = &prefix[..slash];
            if let Ok(path) = self.resolve_path(&format!("{directory}{selected}")) {
                return Some(path);
            }
        }
        self.resolve_path(&selected).ok()
    }

    fn resolve_existing(&self, requested: &str, kind: ExistingKind) -> Result<PathBuf> {
        let candidates = self.candidate_paths(requested)?;
        for candidate in &candidates {
            if kind.matches(candidate) {
                return self.validate_existing(candidate, kind);
            }
        }
        let tried =
            candidates.iter().map(|path| path.display().to_string()).collect::<Vec<_>>().join(", ");
        bail!("skin path not found: {requested} (tried: {tried})")
    }

    fn existing_candidates(&self, requested: &str, kind: ExistingKind) -> Result<Vec<PathBuf>> {
        let mut resolved = Vec::new();
        let mut seen = BTreeSet::new();
        for candidate in self.candidate_paths(requested)? {
            if !kind.matches(&candidate) {
                continue;
            }
            let canonical = self.validate_existing(&candidate, kind)?;
            if seen.insert(canonical.clone()) {
                resolved.push(canonical);
            }
        }
        Ok(resolved)
    }

    fn candidate_paths(&self, requested: &str) -> Result<Vec<PathBuf>> {
        if requested.contains('\0') {
            bail!("skin path contains NUL");
        }
        let requested = requested.replace('\\', "/");
        if looks_like_windows_absolute(&requested) && !cfg!(windows) {
            bail!("skin path uses an unsupported absolute prefix: {requested}");
        }
        let path = Path::new(&requested);
        let raw = if path.is_absolute() {
            vec![path.to_path_buf()]
        } else if let Some(relative) = requested.strip_prefix("skin/") {
            if relative.is_empty() {
                bail!("skin path alias is empty");
            }
            let mut paths = Vec::with_capacity(self.library_roots.len() + 1);
            if let Some((package, package_relative)) = relative.split_once('/') {
                let entry_only =
                    self.library_roots.len() == 1 && self.library_roots[0] == self.entry_dir;
                let exact_package_exists =
                    self.library_roots.iter().any(|root| root.join(package).is_dir());
                // beatoraja skins often hard-code their original package name,
                // even when the installed directory was renamed. Preserve that
                // self-alias only when it cannot mask a real sibling package.
                if entry_only {
                    paths.push(self.entry_dir.join(package_relative));
                } else if !exact_package_exists
                    && let Some(entry_package_root) = self.entry_package_root()
                {
                    paths.push(entry_package_root.join(package_relative));
                }
            }
            paths.extend(self.library_roots.iter().map(|root| root.join(relative)));
            paths
        } else {
            let mut paths = Vec::with_capacity(self.library_roots.len() + 1);
            paths.push(self.entry_dir.join(&requested));
            paths.extend(self.library_roots.iter().map(|root| root.join(&requested)));
            paths
        };

        let mut candidates = Vec::new();
        let mut seen = BTreeSet::new();
        for candidate in raw {
            let candidate = lexical_normalize(&candidate)?;
            if !self.is_lexically_allowed(&candidate) {
                continue;
            }
            if seen.insert(candidate.clone()) {
                candidates.push(candidate);
            }
        }
        if candidates.is_empty() {
            bail!("skin path escapes skin root (configured library roots): {requested}");
        }
        Ok(candidates)
    }

    fn entry_package_root(&self) -> Option<PathBuf> {
        self.library_roots.iter().find_map(|root| {
            let relative = self.entry_dir.strip_prefix(root).ok()?;
            let Component::Normal(package) = relative.components().next()? else {
                return None;
            };
            let package_root = root.join(package);
            package_root.is_dir().then_some(package_root)
        })
    }

    fn validate_existing(&self, path: &Path, kind: ExistingKind) -> Result<PathBuf> {
        let canonical = canonicalize_skin_path(path)
            .with_context(|| format!("failed to canonicalize skin path: {}", path.display()))?;
        if !self.library_roots.iter().any(|root| canonical.starts_with(root)) {
            bail!(
                "skin path escapes skin root (configured library roots): {}",
                canonical.display()
            );
        }
        if !kind.matches(&canonical) {
            bail!("skin path has unexpected type: {}", canonical.display());
        }
        Ok(canonical)
    }

    fn is_lexically_allowed(&self, path: &Path) -> bool {
        self.library_roots.iter().any(|root| path.starts_with(root))
    }
}

#[derive(Debug, Clone, Copy)]
enum ExistingKind {
    Any,
    File,
    Directory,
}

impl ExistingKind {
    fn matches(self, path: &Path) -> bool {
        match self {
            Self::Any => path.exists(),
            Self::File => path.is_file(),
            Self::Directory => path.is_dir(),
        }
    }
}

fn strip_beatoraja_asset_filter(path: &str) -> &str {
    path.split_once('|').map_or(path, |(asset_path, _)| asset_path)
}

fn looks_like_windows_absolute(path: &str) -> bool {
    path.starts_with("//")
        || (path.as_bytes().get(1) == Some(&b':')
            && path.as_bytes().first().is_some_and(u8::is_ascii_alphabetic))
}

fn lexical_normalize(path: &Path) -> Result<PathBuf> {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Prefix(prefix) => normalized.push(prefix.as_os_str()),
            Component::RootDir => normalized.push(component.as_os_str()),
            Component::CurDir => {}
            Component::Normal(part) => normalized.push(part),
            Component::ParentDir => {
                if !normalized.pop() {
                    bail!("skin path traverses above filesystem root: {}", path.display());
                }
            }
        }
    }
    Ok(normalized)
}

/// Canonicalizes paths while keeping normal mixed-separator behavior on
/// Windows (where `std` otherwise returns a `\\?\` verbatim path).
pub(crate) fn canonicalize_skin_path(path: &Path) -> std::io::Result<PathBuf> {
    path.canonicalize().map(simplify_verbatim_path)
}

#[cfg(windows)]
fn simplify_verbatim_path(path: PathBuf) -> PathBuf {
    let text = path.as_os_str().to_string_lossy();
    if let Some(rest) = text.strip_prefix(r"\\?\UNC\") {
        return PathBuf::from(format!(r"\\{rest}"));
    }
    if let Some(rest) = text.strip_prefix(r"\\?\") {
        let bytes = rest.as_bytes();
        if bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' {
            return PathBuf::from(rest);
        }
    }
    path
}

#[cfg(not(windows))]
fn simplify_verbatim_path(path: PathBuf) -> PathBuf {
    path
}

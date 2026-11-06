use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// One recursive catalog scan, including symbolic links and Windows junctions.
pub(crate) struct DirectoryScan {
    policy: bmz_skin_assets::SkinPathPolicy,
    visited: HashSet<PathBuf>,
    files: HashSet<PathBuf>,
    entries: usize,
    skipped: usize,
    rejected: usize,
    first_rejection: Option<String>,
    limited: bool,
}

pub(crate) const MAX_SCAN_DEPTH: usize = 64;
pub(crate) const MAX_SCAN_ENTRIES: usize = 100_000;

impl DirectoryScan {
    pub(crate) fn new(roots: impl IntoIterator<Item = PathBuf>) -> anyhow::Result<Self> {
        Ok(Self {
            policy: bmz_skin_assets::SkinPathPolicy::new(roots)?,
            visited: HashSet::new(),
            files: HashSet::new(),
            entries: 0,
            skipped: 0,
            rejected: 0,
            first_rejection: None,
            limited: false,
        })
    }

    pub(crate) fn read_dir_once(&mut self, dir: &Path, depth: usize) -> Option<Vec<PathBuf>> {
        if depth > MAX_SCAN_DEPTH || self.entries >= MAX_SCAN_ENTRIES {
            self.limited = true;
            return None;
        }
        let canonical = match self.policy.resolve(dir) {
            Ok(path) => path,
            Err(error) => {
                self.reject(error);
                return None;
            }
        };
        if !self.visited.insert(canonical.clone()) {
            self.skipped += 1;
            return None;
        }
        // Enumerate the validated real directory. Keep logical paths only for
        // stable resource:/data: identities, never as additional authority.
        let entries = std::fs::read_dir(&canonical).ok()?;
        let mut candidates = Vec::new();
        for entry in entries {
            if self.entries >= MAX_SCAN_ENTRIES {
                self.limited = true;
                break;
            }
            self.entries += 1;
            let Ok(entry) = entry else {
                self.skipped += 1;
                continue;
            };
            let real = match self.policy.resolve(&entry.path()) {
                Ok(path) => path,
                Err(error) => {
                    self.reject(error);
                    continue;
                }
            };
            let is_alias = entry.path() != real;
            candidates.push((is_alias, entry.file_name(), real));
        }
        candidates.sort_by(|a, b| (&a.0, &a.1).cmp(&(&b.0, &b.1)));
        Some(candidates.into_iter().map(|(_, name, _)| dir.join(name)).collect())
    }

    /// Deduplicate after the caller recognizes a candidate's logical name.
    /// A non-skin file can have a valid .luaskin alias; sound aliases can also
    /// intentionally assign several marker names to one audio file.
    pub(crate) fn accept_file(&mut self, path: &Path) -> bool {
        let Ok(real) = self.policy.resolve(path) else { return false };
        if self.files.insert(real) {
            true
        } else {
            self.skipped += 1;
            false
        }
    }

    fn reject(&mut self, error: anyhow::Error) {
        self.rejected += 1;
        self.first_rejection.get_or_insert_with(|| format!("{error:#}"));
    }
}

impl Drop for DirectoryScan {
    fn drop(&mut self) {
        if self.limited {
            tracing::warn!(
                entries = self.entries,
                max_depth = MAX_SCAN_DEPTH,
                "directory scan limit reached; remaining candidates omitted"
            );
        }
        if self.rejected > 0 {
            tracing::debug!(rejected = self.rejected, reason = ?self.first_rejection, "directory scan rejected paths; first reason shown");
        }
        if self.skipped > 0 {
            tracing::debug!(
                skipped = self.skipped,
                "directory scan omitted already visited paths or unreadable entries"
            );
        }
    }
}

#[cfg(all(test, any(unix, windows)))]
pub(crate) mod test_support {
    use super::*;

    pub(crate) struct LinkedDirectories {
        base: PathBuf,
        pub(crate) root: PathBuf,
        pub(crate) linked_root: PathBuf,
        pub(crate) nested: PathBuf,
        links: Vec<PathBuf>,
    }

    impl LinkedDirectories {
        pub(crate) fn new() -> Self {
            let stamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let base = std::env::temp_dir()
                .join(format!("bmz-directory-scan-{}-{stamp}", std::process::id()));
            let root = base.join("root");
            let linked_root = base.join("linked-root");
            let nested = root.join("nested");
            std::fs::create_dir_all(&nested).unwrap();
            let mut fixture = Self { base, root, linked_root, nested, links: Vec::new() };
            for (link, target) in [
                (fixture.root.join("self"), fixture.root.clone()),
                (fixture.nested.join("parent"), fixture.root.clone()),
                (fixture.root.join("alias-a"), fixture.nested.clone()),
                (fixture.root.join("alias-b"), fixture.nested.clone()),
                (fixture.linked_root.clone(), fixture.root.clone()),
            ] {
                create_directory_link(&link, &target);
                fixture.links.push(link);
            }
            fixture
        }

        pub(crate) fn link(&mut self, link: PathBuf, target: &Path) {
            create_directory_link(&link, target);
            self.links.push(link);
        }
    }

    #[cfg(unix)]
    fn create_directory_link(link: &Path, target: &Path) {
        std::os::unix::fs::symlink(target, link).unwrap();
    }

    #[cfg(windows)]
    fn create_directory_link(link: &Path, target: &Path) {
        // Junction creation needs no Developer Mode or symbolic-link privilege.
        let output = std::process::Command::new("powershell.exe")
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                "$ErrorActionPreference = 'Stop'; New-Item -ItemType Junction -Path $env:BMZ_TEST_LINK -Target $env:BMZ_TEST_TARGET | Out-Null",
            ])
            .env("BMZ_TEST_LINK", link)
            .env("BMZ_TEST_TARGET", target)
            .output()
            .expect("start PowerShell to create a test junction");
        assert!(
            output.status.success(),
            "create test junction {}: {}",
            link.display(),
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(link.canonicalize().unwrap(), target.canonicalize().unwrap());
    }

    impl Drop for LinkedDirectories {
        fn drop(&mut self) {
            // Remove links explicitly before deleting the fixture's own tree.
            for link in self.links.iter().rev() {
                #[cfg(windows)]
                let _ = std::fs::remove_dir(link);
                #[cfg(unix)]
                let _ = std::fs::remove_file(link);
            }
            let _ = std::fs::remove_dir_all(&self.base);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn walk(scan: &mut DirectoryScan, dir: &Path, depth: usize, files: &mut Vec<PathBuf>) {
        for path in scan.read_dir_once(dir, depth).unwrap_or_default() {
            if path.is_dir() {
                walk(scan, &path, depth + 1, files);
            } else if scan.accept_file(&path) {
                files.push(path);
            }
        }
    }

    #[test]
    #[cfg(any(unix, windows))]
    fn root_links_cycles_aliases_and_outside_directories_are_bounded() {
        let outside = tempfile::tempdir().unwrap();
        std::fs::write(outside.path().join("outside.json"), "{}").unwrap();
        let mut fixture = test_support::LinkedDirectories::new();
        fixture.link(fixture.root.join("outside"), outside.path());
        let second = fixture.root.join("second");
        std::fs::create_dir(&second).unwrap();
        fixture.link(second.join("back"), &fixture.nested.clone());
        fixture.link(fixture.nested.join("forward"), &second);
        std::fs::write(fixture.nested.join("inside.json"), "{}").unwrap();
        let mut scan = DirectoryScan::new([fixture.linked_root.clone()]).unwrap();
        let mut files = Vec::new();
        walk(&mut scan, &fixture.linked_root, 0, &mut files);
        assert_eq!(files, vec![fixture.linked_root.join("nested/inside.json")]);
        assert_eq!(scan.visited.len(), 3);
        assert!(scan.rejected > 0);
        assert!(scan.skipped > 0);
    }

    #[test]
    fn deep_acyclic_trees_and_entry_budget_stop() {
        let root = tempfile::tempdir().unwrap();
        let mut dir = root.path().to_path_buf();
        for _ in 0..MAX_SCAN_DEPTH + 2 {
            dir.push("d");
            std::fs::create_dir(&dir).unwrap();
        }
        std::fs::write(dir.join("deep.json"), "{}").unwrap();
        let mut scan = DirectoryScan::new([root.path().to_path_buf()]).unwrap();
        let mut files = Vec::new();
        walk(&mut scan, root.path(), 0, &mut files);
        assert!(files.is_empty());
        assert!(scan.limited);
        assert_eq!(scan.visited.len(), MAX_SCAN_DEPTH + 1);
        for i in 0..3 {
            std::fs::write(root.path().join(format!("{i}.json")), "{}").unwrap();
        }
        let mut scan = DirectoryScan::new([root.path().to_path_buf()]).unwrap();
        scan.entries = MAX_SCAN_ENTRIES - 1;
        assert_eq!(scan.read_dir_once(root.path(), 0).unwrap().len(), 1);
        assert!(scan.limited);
        assert_eq!(scan.entries, MAX_SCAN_ENTRIES);
        assert!(scan.read_dir_once(root.path(), 0).is_none());
    }
}

use std::collections::HashSet;
use std::fs::ReadDir;
use std::path::{Path, PathBuf};

/// One recursive catalog scan, including symbolic links and Windows junctions.
#[derive(Default)]
pub(crate) struct DirectoryScan {
    visited: HashSet<PathBuf>,
}

impl DirectoryScan {
    pub(crate) fn read_dir_once(&mut self, dir: &Path) -> Option<ReadDir> {
        // Canonical paths identify directories only. Keep entry paths relative to
        // the caller's root so catalog paths and explicitly linked roots survive.
        // Do not fall back to an unresolved path: it cannot rule out a cycle.
        let canonical = dir.canonicalize().ok()?;
        if self.visited.contains(&canonical) {
            return None;
        }
        let entries = std::fs::read_dir(dir).ok()?;
        self.visited.insert(canonical);
        Some(entries)
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

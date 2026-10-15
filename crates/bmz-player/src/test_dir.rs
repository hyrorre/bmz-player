use std::path::{Path, PathBuf};

/// A not-yet-created path inside a private `TempDir`. Drop removes the directory even when
/// the test fails, so nothing is left in the shared temp root.
pub(crate) struct TestDir {
    path: PathBuf,
    _parent: tempfile::TempDir,
}

impl TestDir {
    pub(crate) fn new(name: &str) -> Self {
        let parent = tempfile::Builder::new().prefix(&format!("{name}-")).tempdir().unwrap();
        Self { path: parent.path().join(name), _parent: parent }
    }
}

impl std::ops::Deref for TestDir {
    type Target = Path;

    fn deref(&self) -> &Path {
        &self.path
    }
}

impl AsRef<Path> for TestDir {
    fn as_ref(&self) -> &Path {
        &self.path
    }
}

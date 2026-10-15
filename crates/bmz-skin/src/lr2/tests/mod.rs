use super::*;

/// A fresh, exclusively created directory. The name used to be `{name}-{nanos}` only, so
/// two test processes could share it. Tests still own the path's lifetime, as before.
fn unique_test_dir(name: &str) -> PathBuf {
    tempfile::Builder::new().prefix(&format!("{name}-")).tempdir().unwrap().keep()
}

#[path = "cases_01.rs"]
mod cases_01;
#[path = "cases_02.rs"]
mod cases_02;
mod play_compat;
mod refs;
mod result_compat;
mod scenes;

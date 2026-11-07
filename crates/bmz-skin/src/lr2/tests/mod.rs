use super::*;

/// A fresh, exclusively created directory, removed when the returned guard drops.
fn unique_test_dir(name: &str) -> crate::tests::TestDir {
    crate::tests::TestDir::created(name)
}

fn installed_skin_context(path: &Path) -> crate::SkinPathContext {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data/skins");
    crate::SkinPathContext::new(path, [root]).unwrap()
}

#[path = "cases_01.rs"]
mod cases_01;
#[path = "cases_02.rs"]
mod cases_02;
mod litone;
mod play_compat;
mod refs;
mod result_compat;
mod scenes;
mod select_drawing;

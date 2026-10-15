use super::*;

fn unique_skin_test_dir(tag: &str) -> crate::tests::TestDir {
    crate::tests::TestDir::created(&format!("bmz-lua-{tag}"))
}

fn test_skin_path_context(root: &Path) -> SkinPathContext {
    let entry = root.join("test.luaskin");
    fs::write(&entry, "return {}").unwrap();
    SkinPathContext::new(&entry, [root.to_path_buf()]).unwrap()
}

#[path = "tests/cases_01.rs"]
mod cases_01;
#[path = "tests/cases_02.rs"]
mod cases_02;
#[path = "tests/cases_03.rs"]
mod cases_03;

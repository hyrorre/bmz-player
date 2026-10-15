use super::*;

use std::fs;

fn unique_test_dir(name: &str) -> crate::test_dir::TestDir {
    crate::test_dir::TestDir::new(name)
}

fn test_offset_def(name: &str, id: i32) -> SkinOffsetDef {
    SkinOffsetDef {
        category: "test".to_string(),
        name: name.to_string(),
        id,
        x: true,
        y: true,
        w: true,
        h: true,
        r: true,
        a: true,
    }
}

#[path = "tests/cases_01.rs"]
mod cases_01;
#[path = "tests/cases_02.rs"]
mod cases_02;
#[path = "tests/cases_03.rs"]
mod cases_03;
#[path = "tests/cases_04.rs"]
mod cases_04;
#[path = "tests/lr2_skin_settings.rs"]
mod lr2_skin_settings;

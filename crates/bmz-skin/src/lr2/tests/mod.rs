use super::*;
use std::time::{SystemTime, UNIX_EPOCH};

fn unique_test_dir(name: &str) -> PathBuf {
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    std::env::temp_dir().join(format!("{name}-{nanos}"))
}

#[path = "cases_01.rs"]
mod cases_01;
#[path = "cases_02.rs"]
mod cases_02;
mod scenes;

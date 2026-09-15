//! Portable package validation and recoverable replacement. No audio/GPU dependencies.
pub mod archive;
pub mod manifest;
pub mod process;
pub mod transaction;

pub const PROTOCOL: u32 = 1;
pub const MANIFEST: &str = "bmz-package.json";
pub const WORK_DIR: &str = ".bmz-update";

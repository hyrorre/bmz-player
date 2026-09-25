//! Portable package validation and recoverable replacement. No audio/GPU dependencies.
pub mod archive;
pub mod manifest;
pub mod process;
pub mod transaction;

pub const PROTOCOL: u32 = 2;
pub const MANIFEST: &str = "updater/bmz-package.json";
pub const HELPER: &str = "updater/bmz-updater.exe";
pub const INSTANCE_LOCK: &str = "updater/instance.lock";
pub const UPDATE_LOCK: &str = "updater/update.lock";
pub const WORK_DIR: &str = "updater";
pub const LEGACY_MANIFEST: &str = "bmz-package.json";
pub const LEGACY_HELPER: &str = "bmz-updater.exe";
pub const LEGACY_INSTANCE_LOCK: &str = ".bmz-instance.lock";
pub const LEGACY_UPDATE_LOCK: &str = ".bmz-updater.lock";
pub const LEGACY_WORK_DIR: &str = ".bmz-update";

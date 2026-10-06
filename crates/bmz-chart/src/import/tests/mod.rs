use std::io::Write;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use bmz_core::lane::Lane;

use crate::hash::compute_chart_identity;
use crate::model::{
    BgaAssetId, BgaAssetKind, BgaEventKind, JudgeRankKind, LongNoteStyle, NoteKind, TimingEventKind,
};

use super::*;

static TEMP_FILE_COUNTER: AtomicU64 = AtomicU64::new(0);

fn write_temp_file_with_ext(text: &str, ext: &str) -> std::path::PathBuf {
    let stamp = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    let counter = TEMP_FILE_COUNTER.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir()
        .join(format!("bmz-chart-import-{}-{stamp}-{counter}.{ext}", std::process::id()));
    let mut file = std::fs::File::create(&path).unwrap();
    file.write_all(text.as_bytes()).unwrap();
    file.sync_all().unwrap();
    path
}

fn write_temp_bms(text: &str) -> std::path::PathBuf {
    let stamp = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    let counter = TEMP_FILE_COUNTER.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir()
        .join(format!("bmz-chart-import-{}-{stamp}-{counter}.bms", std::process::id()));
    let mut file = std::fs::File::create(&path).unwrap();
    file.write_all(text.as_bytes()).unwrap();
    file.sync_all().unwrap();
    path
}

fn write_temp_file(path: &Path) {
    let mut file = std::fs::File::create(path).unwrap();
    file.write_all(b"").unwrap();
    file.sync_all().unwrap();
}

fn repo_root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn bga_asset_path_for_key(chart: &PlayableChart, key: u16) -> (BgaAssetKind, String) {
    let asset_id = chart.bga_asset_by_bmp_key[&key];
    let asset = chart.bga_assets.iter().find(|asset| asset.id == asset_id).unwrap();
    (asset.kind, asset.path.strip_prefix(repo_root()).unwrap().to_string_lossy().replace('\\', "/"))
}

fn bga_asset_manifest(
    chart: &PlayableChart,
) -> Vec<(u16, BgaAssetId, std::path::PathBuf, BgaAssetKind)> {
    let mut manifest = chart
        .bga_asset_by_bmp_key
        .iter()
        .map(|(&key, &asset_id)| {
            let asset = chart.bga_assets.iter().find(|asset| asset.id == asset_id).unwrap();
            (key, asset_id, asset.path.clone(), asset.kind)
        })
        .collect::<Vec<_>>();
    manifest.sort_by_key(|(key, ..)| *key);
    manifest
}

mod bmp00;
#[path = "cases_01.rs"]
mod cases_01;
#[path = "cases_02.rs"]
mod cases_02;

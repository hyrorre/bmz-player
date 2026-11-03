use super::*;
use crate::storage::library_db::LibraryDatabase;
use std::collections::HashMap;

pub struct DownloadScanRegistration {
    pub task_ids: Vec<DownloadTaskId>,
    pub scan_path: PathBuf,
    pub identities: Vec<ChartDownloadIdentity>,
}

pub fn downloaded_chart_identities(
    db: &LibraryDatabase,
    scan_path: &Path,
) -> Result<Vec<ChartDownloadIdentity>> {
    let archive_root;
    let scan_path = if crate::song_archive::is_archive(scan_path) {
        archive_root = crate::chart_source::ChartLocator::Archive {
            container: scan_path.into(),
            entry: String::new(),
        }
        .to_path_buf();
        archive_root.as_path()
    } else {
        scan_path
    };
    let mut folders = db.list_descendant_folder_paths(&scan_path.to_string_lossy())?;
    folders.push(scan_path.to_string_lossy().into_owned());
    let refs: Vec<_> = folders.iter().map(String::as_str).collect();
    Ok(db
        .list_charts_in_folders(&refs)?
        .into_iter()
        .map(|chart| ChartDownloadIdentity { sha256: Some(chart.sha256), md5: Some(chart.md5) })
        .collect())
}

/// Verify a shared scan target once, then fan out its registration to every task.
pub fn verify_download_registrations(
    db: &LibraryDatabase,
    downloads: Vec<(DownloadTaskId, ChartDownloadResult)>,
) -> Result<Vec<DownloadScanRegistration>> {
    let mut groups: HashMap<PathBuf, Vec<DownloadTaskId>> = HashMap::new();
    for (id, result) in downloads {
        groups.entry(result.scan_path).or_default().push(id);
    }
    groups
        .into_iter()
        .map(|(scan_path, task_ids)| {
            let identities = downloaded_chart_identities(db, &scan_path)?;
            Ok(DownloadScanRegistration { task_ids, scan_path, identities })
        })
        .collect()
}

/// Late completions can join a scan that is already verifying this archive.
pub fn attach_download_registrations(
    pending: &mut Vec<(DownloadTaskId, ChartDownloadResult)>,
    scan_path: &Path,
) -> Vec<DownloadTaskId> {
    let mut ids = Vec::new();
    pending.retain(|(id, result)| {
        if result.scan_path == scan_path {
            ids.push(*id);
            false
        } else {
            true
        }
    });
    ids
}

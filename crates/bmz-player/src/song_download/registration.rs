use super::*;
use crate::storage::library_db::LibraryDatabase;
use std::collections::HashMap;

pub struct DownloadScanRegistration {
    pub task_ids: Vec<DownloadTaskId>,
    pub chart_dir: PathBuf,
    pub identities: Vec<ChartDownloadIdentity>,
}

pub fn downloaded_chart_identities(
    db: &LibraryDatabase,
    chart_dir: &Path,
) -> Result<Vec<ChartDownloadIdentity>> {
    let mut folders = db.list_descendant_folder_paths(&chart_dir.to_string_lossy())?;
    folders.push(chart_dir.to_string_lossy().into_owned());
    let refs: Vec<_> = folders.iter().map(String::as_str).collect();
    Ok(db
        .list_charts_in_folders(&refs)?
        .into_iter()
        .map(|chart| ChartDownloadIdentity { sha256: Some(chart.sha256), md5: Some(chart.md5) })
        .collect())
}

/// Verify a shared directory once, then fan out its registration to every task.
pub fn verify_download_registrations(
    db: &LibraryDatabase,
    downloads: Vec<(DownloadTaskId, ChartDownloadResult)>,
) -> Result<Vec<DownloadScanRegistration>> {
    let mut groups: HashMap<PathBuf, Vec<DownloadTaskId>> = HashMap::new();
    for (id, result) in downloads {
        groups.entry(result.chart_dir).or_default().push(id);
    }
    groups
        .into_iter()
        .map(|(chart_dir, task_ids)| {
            let identities = downloaded_chart_identities(db, &chart_dir)?;
            Ok(DownloadScanRegistration { task_ids, chart_dir, identities })
        })
        .collect()
}

/// Late completions can join a scan that is already verifying this archive.
pub fn attach_download_registrations(
    pending: &mut Vec<(DownloadTaskId, ChartDownloadResult)>,
    chart_dir: &Path,
) -> Vec<DownloadTaskId> {
    let mut ids = Vec::new();
    pending.retain(|(id, result)| {
        if result.chart_dir == chart_dir {
            ids.push(*id);
            false
        } else {
            true
        }
    });
    ids
}

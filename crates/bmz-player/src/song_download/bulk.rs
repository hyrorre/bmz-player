use super::*;
use crate::storage::library_db::{LibraryDatabase, TableEntryListItem};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BulkDownloadScope {
    pub source_url: String,
    pub level: Option<String>,
}

impl BulkDownloadScope {
    pub fn from_path(path: &str) -> Option<Self> {
        use crate::screens::select_model::{TablePath, parse_table_path};
        match parse_table_path(path)? {
            TablePath::Table { source_url } => {
                Some(Self { source_url: source_url.into(), level: None })
            }
            TablePath::Level { source_url, level } => {
                Some(Self { source_url: source_url.into(), level: Some(level.into()) })
            }
            TablePath::Root => None,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct BulkDownloadCounts {
    pub total: usize,
    pub owned: usize,
    pub downloadable: usize,
    pub unavailable: usize,
    pub merged: usize,
    pub queued: usize,
}

#[derive(Debug)]
pub struct BulkDownloadPlan {
    pub scope: BulkDownloadScope,
    pub counts: BulkDownloadCounts,
    pub requests: Vec<ChartDownloadRequest>,
}

#[derive(Default)]
pub struct BulkDownloadPreview {
    pub preparing: bool,
    pub plan: Option<BulkDownloadPlan>,
    pub error: Option<String>,
}

pub fn prepare_bulk_download(
    db: &LibraryDatabase,
    config: &ChartDownloadsConfig,
    scope: BulkDownloadScope,
    data_dir: &Path,
) -> Result<BulkDownloadPlan> {
    let rows = db.registered_table_entries_at_level(&scope.source_url, scope.level.as_deref())?;
    Ok(plan_rows(rows, config, scope, data_dir))
}

fn plan_rows(
    rows: Vec<TableEntryListItem>,
    config: &ChartDownloadsConfig,
    scope: BulkDownloadScope,
    data_dir: &Path,
) -> BulkDownloadPlan {
    let mut counts = BulkDownloadCounts { total: rows.len(), ..Default::default() };
    let mut requests = Vec::new();
    let mut sha256s = HashSet::new();
    let mut md5s = std::collections::HashMap::new();
    for row in rows {
        if row.chart.is_some() {
            counts.owned += 1;
            continue;
        }
        let metadata = ChartDownloadMetadata {
            md5: row.md5,
            sha256: row.sha256,
            url: row.url,
            append_url: row.append_url,
            ipfs: row.ipfs,
            append_ipfs: row.append_ipfs,
        };
        let identity = ChartDownloadIdentity::from_metadata(&metadata);
        if identity.sha256.is_none() && identity.md5.is_none() {
            counts.unavailable += 1;
            continue;
        }
        let action = choose_missing_chart_action(config, &metadata);
        if !matches!(action, MissingChartAction::Http { .. } | MissingChartAction::Ipfs { .. }) {
            counts.unavailable += 1;
            continue;
        }
        counts.downloadable += 1;
        let duplicate = if let Some(sha) = identity.sha256 {
            sha256s.contains(&sha) || identity.md5.is_some_and(|md5| md5s.get(&md5) == Some(&None))
        } else {
            identity.md5.is_some_and(|md5| md5s.contains_key(&md5))
        };
        if let Some(sha) = identity.sha256 {
            sha256s.insert(sha);
        }
        if let Some(md5) = identity.md5 {
            md5s.insert(md5, identity.sha256);
        }
        if duplicate {
            counts.merged += 1;
            continue;
        }
        requests.push(ChartDownloadRequest {
            request_id: 0,
            identity,
            action,
            title: row.title,
            data_dir: data_dir.into(),
        });
    }
    BulkDownloadPlan { scope, counts, requests }
}

pub struct BulkDownloadBatch {
    pub scope: BulkDownloadScope,
    pub counts: BulkDownloadCounts,
    pub targets: Vec<(DownloadTaskId, ChartDownloadIdentity)>,
}

impl BulkDownloadBatch {
    pub fn results(&self, manager: &ChartDownloadManager) -> (usize, usize) {
        let mut succeeded = 0;
        let mut failed = 0;
        for (id, identity) in &self.targets {
            if let Some(task) = manager.task(*id) {
                if task.registered.iter().any(|registered| identity.overlaps(registered)) {
                    succeeded += 1;
                } else if task.state() == DownloadState::Failed {
                    failed += 1;
                }
            }
        }
        (succeeded, failed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::difficulty_table::{FetchedDifficultyTable, FetchedTableEntry};

    #[test]
    fn owned_charts_are_excluded_with_normalized_hashes_and_hashless_rows_remain_missing() {
        use crate::config::app_config::{AppConfig, PathEntry};
        let root = tempfile::tempdir().unwrap();
        let songs = root.path().join("songs");
        std::fs::create_dir(&songs).unwrap();
        let chart_path = songs.join("chart.bms");
        std::fs::write(&chart_path, "#TITLE Owned\n#BPM 120\n#00011:01\n").unwrap();
        let db_path = root.path().join("library.db");
        crate::storage::migration::migrate_library_db(&db_path).unwrap();
        let mut db = LibraryDatabase::open(&db_path).unwrap();
        let roots = vec![PathEntry {
            path: songs.to_string_lossy().into_owned(),
            enabled: true,
            recursive: true,
        }];
        let mut config = AppConfig::default();
        config.scan.use_everything = false;
        crate::storage::scan::scan_song_roots(&mut db, &roots, &config.scan, 1, true).unwrap();
        db.set_configured_song_roots(&roots).unwrap();
        let id = db.chart_id_by_chart_file_path(&chart_path).unwrap().unwrap();
        let chart = db.list_charts_by_ids(&[id]).unwrap().pop().unwrap();
        db.upsert_difficulty_table(&FetchedDifficultyTable {
            source_url: "test".into(),
            head_url: String::new(),
            name: "Table".into(),
            symbol: "T".into(),
            level_order: vec![],
            courses: vec![],
            fetched_at: 1,
            entries: vec![
                FetchedTableEntry {
                    level: "1".into(),
                    sha256: format!(
                        " {} ",
                        crate::storage::common::hash_to_hex(&chart.sha256).to_uppercase()
                    ),
                    ..Default::default()
                },
                FetchedTableEntry {
                    level: "1".into(),
                    md5: crate::storage::common::hash_to_hex(&chart.md5).to_uppercase(),
                    ..Default::default()
                },
                FetchedTableEntry {
                    level: "1".into(),
                    title: "Owned".into(),
                    ..Default::default()
                },
            ],
        })
        .unwrap();
        let plan = prepare_bulk_download(
            &db,
            &config.downloads,
            BulkDownloadScope { source_url: "test".into(), level: None },
            root.path(),
        )
        .unwrap();
        assert_eq!((plan.counts.total, plan.counts.owned, plan.counts.unavailable), (3, 2, 1));
    }

    #[test]
    fn bulk_table_and_level_use_batched_hashes_and_count_unavailable_and_duplicates() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("library.db");
        crate::storage::migration::migrate_library_db(&path).unwrap();
        let mut db = LibraryDatabase::open(&path).unwrap();
        db.upsert_difficulty_table(&FetchedDifficultyTable {
            source_url: "https://table.test".into(),
            head_url: String::new(),
            name: "Table".into(),
            symbol: "T".into(),
            level_order: vec![],
            courses: vec![],
            fetched_at: 1,
            entries: vec![
                FetchedTableEntry { level: "1".into(), md5: "AB".repeat(16), ..Default::default() },
                FetchedTableEntry { level: "2".into(), md5: "ab".repeat(16), ..Default::default() },
                FetchedTableEntry { level: "2".into(), md5: "../bad".into(), ..Default::default() },
                FetchedTableEntry {
                    level: "2".into(),
                    url: "https://manual.test/".into(),
                    ..Default::default()
                },
                FetchedTableEntry { level: "1".into(), md5: "cc".repeat(16), ..Default::default() },
            ],
        })
        .unwrap();
        let config = ChartDownloadsConfig {
            http_enabled: true,
            http_api_url: "https://api.test/{md5}".into(),
            ..Default::default()
        };
        let scope = BulkDownloadScope { source_url: "https://table.test".into(), level: None };
        let all = prepare_bulk_download(&db, &config, scope.clone(), root.path()).unwrap();
        assert_eq!(
            (
                all.counts.total,
                all.counts.downloadable,
                all.counts.unavailable,
                all.counts.merged,
                all.requests.len()
            ),
            (5, 3, 2, 1, 2)
        );
        let level = prepare_bulk_download(
            &db,
            &config,
            BulkDownloadScope { level: Some("2".into()), ..scope.clone() },
            root.path(),
        )
        .unwrap();
        assert_eq!((level.counts.total, level.requests.len()), (3, 1));
        let empty = prepare_bulk_download(
            &db,
            &config,
            BulkDownloadScope { level: Some("empty".into()), ..scope },
            root.path(),
        )
        .unwrap();
        assert_eq!(empty.counts.total, 0);
        assert!(empty.requests.is_empty());
    }
}

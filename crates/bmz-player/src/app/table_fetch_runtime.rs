use std::collections::HashSet;
use std::sync::mpsc::Receiver;
use std::time::Instant;

use anyhow::Result;

use crate::bootstrap::BootstrappedApp;
use crate::config::app_config::AppConfig;
use crate::ir::table::RianTableIdentity;
use crate::table_cmd::{
    TableFetchDownloadBatchResult, TableFetchDownloadOutcome, TableFetchOutcome,
};

#[path = "table_fetch_runtime/store.rs"]
mod store;
pub(super) use store::{PendingTableStore, TableStoreCompletion};

pub(super) enum TableFetchWorkerEvent {
    Downloaded(TableFetchDownloadOutcome),
    Finished(Result<TableFetchDownloadBatchResult>),
}

pub(super) struct TableFetchProgress {
    pub(super) label: String,
    pub(super) total: usize,
    pub(super) completed: usize,
    pub(super) succeeded: usize,
    pub(super) failed: usize,
    pub(super) outcomes: Vec<TableFetchOutcome>,
}

pub(super) struct RianTableFetchWorkerResult {
    pub(super) generation: u64,
    pub(super) identity: RianTableIdentity,
    pub(super) result: RianTableFetchOutcome,
}

pub(super) enum RianTableFetchOutcome {
    Completed(Result<Vec<crate::difficulty_table::FetchedDifficultyTable>>),
    Paused,
}

/// 通常の難易度表と rianIR 表を取得する background worker のライフサイクル。
///
/// DB保存は同時1件のworkerへ渡す。開始と結果の適用はSelect中だけ行う。
pub(super) struct TableFetchRuntime {
    pub(super) startup_urls: Option<Vec<String>>,
    pub(super) pending: Option<Receiver<TableFetchWorkerEvent>>,
    pub(super) pending_store: Option<PendingTableStore>,
    pub(super) pending_urls: HashSet<String>,
    pub(super) queued_urls: Vec<String>,
    pub(super) progress: Option<TableFetchProgress>,
    pub(super) rian_identity: Option<RianTableIdentity>,
    pub(super) rian_generation: u64,
    pub(super) pending_rian: Option<Receiver<RianTableFetchWorkerResult>>,
    pub(super) rian_last_started_at: Option<Instant>,
    pub(super) rian_next_refresh_at: Option<Instant>,
    /// Play/Decide/Result中に要求されたrefreshを次のSelectまで保持する。
    pub(super) rian_refresh_queued: bool,
    /// queued refreshに手動要求が1件でも含まれる場合は短いcooldownを適用する。
    pub(super) rian_refresh_manual: bool,
}

impl TableFetchRuntime {
    pub(super) fn new(startup_urls: Vec<String>, rian_identity: Option<RianTableIdentity>) -> Self {
        Self {
            startup_urls: Some(startup_urls),
            pending: None,
            pending_store: None,
            pending_urls: HashSet::new(),
            queued_urls: Vec::new(),
            progress: None,
            rian_identity,
            rian_generation: 0,
            pending_rian: None,
            rian_last_started_at: None,
            rian_next_refresh_at: None,
            rian_refresh_queued: false,
            rian_refresh_manual: false,
        }
    }

    pub(super) fn filter_new_urls(&self, urls: Vec<String>) -> Vec<String> {
        let mut seen = HashSet::new();
        urls.into_iter()
            .filter(|url| seen.insert(url.clone()))
            .filter(|url| !self.pending_urls.contains(url))
            .filter(|url| !self.queued_urls.iter().any(|queued| queued == url))
            .collect()
    }
}

pub(super) fn startup_difficulty_table_fetch_urls_for_boot(boot: &BootstrappedApp) -> Vec<String> {
    let fetched_source_urls: HashSet<String> =
        match boot.library_db.list_difficulty_table_sources_with_current_download_metadata() {
            Ok(source_urls) => source_urls.into_iter().collect(),
            Err(error) => {
                tracing::warn!(
                    %error,
                    "failed to list difficulty tables with current download metadata"
                );
                HashSet::new()
            }
        };
    startup_difficulty_table_fetch_urls(&boot.app_config, &fetched_source_urls)
}

fn startup_difficulty_table_fetch_urls(
    app_config: &AppConfig,
    fetched_source_urls: &HashSet<String>,
) -> Vec<String> {
    app_config
        .tables
        .sources
        .iter()
        .filter(|source| source.enabled)
        .filter(|source| {
            app_config.tables.auto_fetch_on_startup || !fetched_source_urls.contains(&source.url)
        })
        .map(|source| source.url.clone())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::app_config::{DifficultyTableSource, DifficultyTablesConfig};

    #[test]
    fn startup_table_fetch_urls_include_unfetched_enabled_sources() {
        let config = AppConfig {
            tables: DifficultyTablesConfig {
                sources: vec![
                    DifficultyTableSource {
                        url: "https://example.com/fetched".to_string(),
                        enabled: true,
                    },
                    DifficultyTableSource {
                        url: "https://example.com/missing".to_string(),
                        enabled: true,
                    },
                    DifficultyTableSource {
                        url: "https://example.com/disabled".to_string(),
                        enabled: false,
                    },
                ],
                auto_fetch_on_startup: false,
            },
            ..AppConfig::default()
        };
        let fetched = HashSet::from(["https://example.com/fetched".to_string()]);

        assert_eq!(
            startup_difficulty_table_fetch_urls(&config, &fetched),
            vec!["https://example.com/missing".to_string()]
        );
    }

    #[test]
    fn startup_table_fetch_urls_include_all_enabled_sources_when_auto_fetch_is_on() {
        let config = AppConfig {
            tables: DifficultyTablesConfig {
                sources: vec![
                    DifficultyTableSource {
                        url: "https://example.com/fetched".to_string(),
                        enabled: true,
                    },
                    DifficultyTableSource {
                        url: "https://example.com/missing".to_string(),
                        enabled: true,
                    },
                    DifficultyTableSource {
                        url: "https://example.com/disabled".to_string(),
                        enabled: false,
                    },
                ],
                auto_fetch_on_startup: true,
            },
            ..AppConfig::default()
        };
        let fetched = HashSet::from(["https://example.com/fetched".to_string()]);

        assert_eq!(
            startup_difficulty_table_fetch_urls(&config, &fetched),
            vec![
                "https://example.com/fetched".to_string(),
                "https://example.com/missing".to_string(),
            ]
        );
    }

    #[test]
    fn filter_new_urls_deduplicates_input_pending_and_queued_urls() {
        let mut runtime = TableFetchRuntime::new(Vec::new(), None);
        runtime.pending_urls.insert("https://example.com/pending".to_string());
        runtime.queued_urls.push("https://example.com/queued".to_string());

        assert_eq!(
            runtime.filter_new_urls(vec![
                "https://example.com/pending".to_string(),
                "https://example.com/new".to_string(),
                "https://example.com/new".to_string(),
                "https://example.com/queued".to_string(),
                "https://example.com/another".to_string(),
            ]),
            vec!["https://example.com/new".to_string(), "https://example.com/another".to_string(),]
        );
    }
}

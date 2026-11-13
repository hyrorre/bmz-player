use std::collections::HashSet;
use std::future::Future;

use anyhow::{Result, bail};
use futures_util::stream::{self, Stream, StreamExt};

use crate::cli::TableCommand;
use crate::config::app_config::DifficultyTableSource;
use crate::config::load::load_app_config;
use crate::config::save::save_app_config;
use crate::paths::{AppPaths, resolve_app_paths};
use crate::storage::library_db::LibraryDatabase;
use crate::storage::migration::migrate_library_db;

/// Upper bound for simultaneous difficulty-table source downloads.
///
/// A source may require an HTML page, a header JSON, and one or more data
/// files.  Limiting concurrent sources keeps first-run downloads responsive
/// without overwhelming table hosts.
pub const DIFFICULTY_TABLE_FETCH_CONCURRENCY: usize = 4;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableFetchSuccess {
    pub url: String,
    pub name: String,
    pub symbol: String,
    pub entries: usize,
    pub courses: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableFetchFailure {
    pub url: String,
    pub error: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TableFetchOutcome {
    Succeeded(TableFetchSuccess),
    Failed(TableFetchFailure),
}

/// App側のmaintenance workerが取得だけを済ませ、Selectへ戻ってからDBへ保存するための結果。
///
/// CLIは従来どおり取得直後に保存する。リアルタイム描画を持つAppだけがこの中間結果を使い、
/// Play中に`library.db`のwriter transactionを発生させない。
pub(crate) enum TableFetchDownloadOutcome {
    Succeeded(crate::difficulty_table::FetchedDifficultyTable),
    Failed(TableFetchFailure),
}

pub(crate) struct TableFetchDownloadBatchResult {
    pub(crate) requested: usize,
    pub(crate) remaining_urls: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TableFetchReport {
    pub requested: usize,
    pub outcomes: Vec<TableFetchOutcome>,
}

impl TableFetchReport {
    pub fn succeeded_count(&self) -> usize {
        self.outcomes
            .iter()
            .filter(|outcome| matches!(outcome, TableFetchOutcome::Succeeded(_)))
            .count()
    }

    pub fn failed_count(&self) -> usize {
        self.outcomes
            .iter()
            .filter(|outcome| matches!(outcome, TableFetchOutcome::Failed(_)))
            .count()
    }
}

pub async fn run_table_command(cmd: TableCommand) -> Result<()> {
    let app_paths = resolve_app_paths()?;
    run_table_command_with_paths(cmd, &app_paths).await
}

pub async fn run_table_command_with_paths(cmd: TableCommand, app_paths: &AppPaths) -> Result<()> {
    match cmd {
        TableCommand::Add { url } => add_table(app_paths, &url).await,
        TableCommand::List => list_tables(app_paths),
        TableCommand::Fetch { url } => fetch_tables(app_paths, url.as_deref()).await,
    }
}

async fn add_table(app_paths: &AppPaths, url: &str) -> Result<()> {
    app_paths.ensure_dirs()?;

    let mut app_config = if app_paths.config_toml.exists() {
        load_app_config(&app_paths.config_toml)?
    } else {
        Default::default()
    };

    if app_config.tables.sources.iter().any(|s| s.url == url) {
        bail!("already configured: {url}");
    }
    app_config.tables.sources.push(DifficultyTableSource { url: url.to_string(), enabled: true });
    save_app_config(&app_paths.config_toml, &app_config)?;
    println!("Added {url} to config");

    migrate_library_db(&app_paths.library_db)?;
    let mut library_db = LibraryDatabase::open(&app_paths.library_db)?;

    fetch_table_url(url, &mut library_db).await?;
    println!("Stored.");

    Ok(())
}

fn list_tables(app_paths: &AppPaths) -> Result<()> {
    migrate_library_db(&app_paths.library_db)?;
    let library_db = LibraryDatabase::open(&app_paths.library_db)?;
    let tables = library_db.list_difficulty_tables()?;

    if tables.is_empty() {
        println!("No difficulty tables stored. Use `table add <URL>` to add one.");
        return Ok(());
    }

    for t in &tables {
        let levels = t.level_order.join(", ");
        println!("{} ({}) — levels: [{}]", t.name, t.symbol, levels);
    }
    Ok(())
}

async fn fetch_tables(app_paths: &AppPaths, url: Option<&str>) -> Result<()> {
    app_paths.ensure_dirs()?;

    migrate_library_db(&app_paths.library_db)?;
    let mut library_db = LibraryDatabase::open(&app_paths.library_db)?;

    if let Some(url) = url {
        fetch_table_url(url, &mut library_db).await?;
        return Ok(());
    }

    let app_config = if app_paths.config_toml.exists() {
        load_app_config(&app_paths.config_toml)?
    } else {
        Default::default()
    };

    let sources: Vec<_> = app_config.tables.sources.iter().filter(|s| s.enabled).collect();

    if sources.is_empty() {
        println!("No difficulty table sources configured. Use `table add <URL>` to add one.");
        return Ok(());
    }

    let urls = sources.iter().map(|source| source.url.clone()).collect();
    let report = fetch_table_urls(urls, &mut library_db).await?;
    for outcome in &report.outcomes {
        print_table_fetch_outcome(outcome);
    }

    println!("\n{} succeeded, {} failed.", report.succeeded_count(), report.failed_count());
    Ok(())
}

pub async fn fetch_table_url(url: &str, library_db: &mut LibraryDatabase) -> Result<()> {
    let report = fetch_table_urls(vec![url.to_string()], library_db).await?;
    let Some(outcome) = report.outcomes.into_iter().next() else {
        bail!("no difficulty table URL provided");
    };
    match outcome {
        TableFetchOutcome::Succeeded(success) => {
            print_table_fetch_success(&success);
            Ok(())
        }
        TableFetchOutcome::Failed(failure) => bail!("{}", failure.error),
    }
}

/// Fetches a batch of table sources concurrently and stores each completed
/// table through the provided single SQLite connection.
///
/// Network requests are bounded, but database writes remain sequential so the
/// main app and this worker do not contend with several SQLite writers.
pub async fn fetch_table_urls(
    urls: Vec<String>,
    library_db: &mut LibraryDatabase,
) -> Result<TableFetchReport> {
    fetch_table_urls_with_progress(urls, library_db, |_| {}).await
}

/// Same as [`fetch_table_urls`], reporting each completed source immediately.
///
/// The callback is invoked after a successful table has been committed, or
/// after the source has failed.  Consumers can therefore display reliable
/// progress without accessing the worker's SQLite connection.
pub async fn fetch_table_urls_with_progress<F>(
    urls: Vec<String>,
    library_db: &mut LibraryDatabase,
    mut on_outcome: F,
) -> Result<TableFetchReport>
where
    F: FnMut(&TableFetchOutcome),
{
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let urls = unique_table_urls(urls);
    let requested = urls.len();
    let client = crate::difficulty_table::build_difficulty_table_client()?;
    let mut pending = pending_table_fetches(urls, move |url| {
        let client = client.clone();
        async move {
            crate::difficulty_table::fetch_difficulty_table_with_client(&client, &url, now).await
        }
    });
    let mut outcomes = Vec::with_capacity(requested);

    while let Some((url, fetched)) = pending.next().await {
        let outcome = match fetched {
            Ok(table) => match store_fetched_table(library_db, &table) {
                Ok(success) => TableFetchOutcome::Succeeded(success),
                Err(error) => TableFetchOutcome::Failed(TableFetchFailure {
                    url,
                    error: format!("failed to store difficulty table: {error:#}"),
                }),
            },
            Err(error) => TableFetchOutcome::Failed(TableFetchFailure {
                url,
                error: format!("failed to fetch difficulty table: {error:#}"),
            }),
        };
        on_outcome(&outcome);
        outcomes.push(outcome);
    }

    Ok(TableFetchReport { requested, outcomes })
}

/// 難易度表を並行取得するが、DBには保存せず取得結果を順次呼び出し元へ渡す。
///
/// Appはこの関数をbackground workerで実行し、結果をSelect画面で受信したときだけ
/// 保存workerを開始する。Select外で届いた結果は次のSelectまで保留する。
/// Select中に開始済みのtransactionは画面遷移後もworker上で完了させる。
pub(crate) async fn download_table_urls_with_progress<F>(
    urls: Vec<String>,
    mut maintenance_allowed: tokio::sync::watch::Receiver<bool>,
    mut on_outcome: F,
) -> Result<TableFetchDownloadBatchResult>
where
    F: FnMut(TableFetchDownloadOutcome),
{
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let urls = unique_table_urls(urls);
    let requested = urls.len();
    let mut remaining_urls = urls.clone();
    let client = crate::difficulty_table::build_difficulty_table_client()?;
    let mut pending = pending_table_fetches(urls, move |url| {
        let client = client.clone();
        async move {
            crate::difficulty_table::fetch_difficulty_table_with_client(&client, &url, now).await
        }
    });

    loop {
        if !*maintenance_allowed.borrow() {
            break;
        }
        let next = tokio::select! {
            biased;
            changed = maintenance_allowed.changed() => {
                if changed.is_err() || !*maintenance_allowed.borrow() {
                    break;
                }
                continue;
            }
            next = pending.next() => next,
        };
        let Some((url, fetched)) = next else {
            break;
        };
        remaining_urls.retain(|remaining| remaining != &url);
        let outcome = match fetched {
            Ok(table) => TableFetchDownloadOutcome::Succeeded(table),
            Err(error) => TableFetchDownloadOutcome::Failed(TableFetchFailure {
                url,
                error: format!("failed to fetch difficulty table: {error:#}"),
            }),
        };
        on_outcome(outcome);
    }

    Ok(TableFetchDownloadBatchResult { requested, remaining_urls })
}

fn unique_table_urls(urls: Vec<String>) -> Vec<String> {
    let mut seen = HashSet::new();
    urls.into_iter().filter(|url| seen.insert(url.clone())).collect()
}

fn pending_table_fetches<F, Fut>(
    urls: Vec<String>,
    fetch: F,
) -> impl Stream<Item = (String, Result<crate::difficulty_table::FetchedDifficultyTable>)>
where
    F: Fn(String) -> Fut,
    Fut: Future<Output = Result<crate::difficulty_table::FetchedDifficultyTable>>,
{
    stream::iter(urls.into_iter().map(move |url| {
        let future = fetch(url.clone());
        async move { (url, future.await) }
    }))
    .buffer_unordered(DIFFICULTY_TABLE_FETCH_CONCURRENCY)
}

pub(crate) fn store_fetched_table(
    library_db: &mut LibraryDatabase,
    table: &crate::difficulty_table::FetchedDifficultyTable,
) -> Result<TableFetchSuccess> {
    library_db.upsert_difficulty_table(table)?;

    let source = format!("table:{}", table.source_url);
    for (position, course) in table.courses.iter().enumerate() {
        library_db.upsert_course(&source, course, position as i64, table.fetched_at)?;
    }

    Ok(TableFetchSuccess {
        url: table.source_url.clone(),
        name: table.name.clone(),
        symbol: table.symbol.clone(),
        entries: table.entries.len(),
        courses: table.courses.len(),
    })
}

fn print_table_fetch_outcome(outcome: &TableFetchOutcome) {
    match outcome {
        TableFetchOutcome::Succeeded(success) => print_table_fetch_success(success),
        TableFetchOutcome::Failed(failure) => println!("FAILED {}: {}", failure.url, failure.error),
    }
}

fn print_table_fetch_success(success: &TableFetchSuccess) {
    println!(
        "Fetched {}: {} ({}) — {} entries",
        success.url, success.name, success.symbol, success.entries
    );
    if success.courses > 0 {
        println!("  {} course(s) stored.", success.courses);
    }
}

#[cfg(test)]
#[path = "table_cmd/profile.rs"]
pub(crate) mod profile;

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;

    #[test]
    fn unique_table_urls_keeps_first_occurrence_order() {
        assert_eq!(
            unique_table_urls(vec![
                "https://example.com/a".to_string(),
                "https://example.com/b".to_string(),
                "https://example.com/a".to_string(),
            ]),
            vec!["https://example.com/a".to_string(), "https://example.com/b".to_string()]
        );
    }

    #[test]
    fn table_fetch_report_counts_each_outcome() {
        let report = TableFetchReport {
            requested: 2,
            outcomes: vec![
                TableFetchOutcome::Succeeded(TableFetchSuccess {
                    url: "https://example.com/ok".to_string(),
                    name: "OK".to_string(),
                    symbol: "★".to_string(),
                    entries: 1,
                    courses: 0,
                }),
                TableFetchOutcome::Failed(TableFetchFailure {
                    url: "https://example.com/no".to_string(),
                    error: "offline".to_string(),
                }),
            ],
        };

        assert_eq!(report.succeeded_count(), 1);
        assert_eq!(report.failed_count(), 1);
    }

    #[tokio::test]
    async fn app_table_download_keeps_urls_queued_while_maintenance_is_paused() {
        let (_maintenance_tx, maintenance_rx) = tokio::sync::watch::channel(false);
        let mut outcome_count = 0;

        let result = download_table_urls_with_progress(
            vec![
                "https://example.invalid/a.json".to_string(),
                "https://example.invalid/a.json".to_string(),
                "https://example.invalid/b.json".to_string(),
            ],
            maintenance_rx,
            |_| outcome_count += 1,
        )
        .await
        .unwrap();

        assert_eq!(result.requested, 2);
        assert_eq!(
            result.remaining_urls,
            vec![
                "https://example.invalid/a.json".to_string(),
                "https://example.invalid/b.json".to_string(),
            ]
        );
        assert_eq!(outcome_count, 0);
    }

    #[tokio::test]
    async fn pending_table_fetches_limits_concurrent_network_requests() {
        let active = Arc::new(AtomicUsize::new(0));
        let max_active = Arc::new(AtomicUsize::new(0));
        let urls = (0..DIFFICULTY_TABLE_FETCH_CONCURRENCY * 2)
            .map(|index| format!("https://example.com/{index}"))
            .collect();
        let mut pending = pending_table_fetches(urls, {
            let active = Arc::clone(&active);
            let max_active = Arc::clone(&max_active);
            move |_url| {
                let active = Arc::clone(&active);
                let max_active = Arc::clone(&max_active);
                async move {
                    let current = active.fetch_add(1, Ordering::SeqCst) + 1;
                    max_active.fetch_max(current, Ordering::SeqCst);
                    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
                    active.fetch_sub(1, Ordering::SeqCst);
                    Err(anyhow::anyhow!("test fetch failure"))
                }
            }
        });

        while pending.next().await.is_some() {}

        assert!(max_active.load(Ordering::SeqCst) > 1);
        assert!(max_active.load(Ordering::SeqCst) <= DIFFICULTY_TABLE_FETCH_CONCURRENCY);
        assert_eq!(active.load(Ordering::SeqCst), 0);
    }
}

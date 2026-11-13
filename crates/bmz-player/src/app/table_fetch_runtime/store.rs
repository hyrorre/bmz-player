use super::*;
use crate::difficulty_table::FetchedDifficultyTable;
use crate::storage::library_db::LibraryDatabase;
use crate::table_cmd::{TableFetchFailure, TableFetchSuccess};
use std::path::PathBuf;
use std::sync::mpsc::{self, TryRecvError};

pub(in crate::app) enum PendingTableStore {
    Ordinary { url: String, rx: Receiver<Result<TableFetchSuccess>> },
    Account { generation: u64, identity: RianTableIdentity, rx: Receiver<Result<(usize, usize)>> },
    Cleanup { rx: Receiver<Result<()>> },
}

pub(in crate::app) enum TableStoreCompletion {
    Ordinary(TableFetchOutcome),
    Account { generation: u64, identity: RianTableIdentity, result: Result<(usize, usize)> },
    Cleanup(Result<()>),
}

impl PendingTableStore {
    pub(in crate::app) fn ordinary(path: PathBuf, table: FetchedDifficultyTable) -> Self {
        Self::Ordinary {
            url: table.source_url.clone(),
            rx: spawn_store(path, move |db| crate::table_cmd::store_fetched_table(db, &table)),
        }
    }

    pub(in crate::app) fn account(
        path: PathBuf,
        generation: u64,
        identity: RianTableIdentity,
        tables: Vec<FetchedDifficultyTable>,
    ) -> Self {
        let worker_identity = identity.clone();
        Self::Account {
            generation,
            identity,
            rx: spawn_store(path, move |db| {
                crate::ir::table::store_account_tables(db, &worker_identity, &tables)
            }),
        }
    }

    pub(in crate::app) fn cleanup(path: PathBuf, identity: RianTableIdentity) -> Self {
        Self::Cleanup {
            rx: spawn_store(path, move |db| {
                db.delete_difficulty_tables_by_source_prefix(identity.source_prefix())?;
                db.delete_table_courses_by_source_prefix(identity.source_prefix())?;
                Ok(())
            }),
        }
    }

    pub(in crate::app) fn poll(&self) -> Option<TableStoreCompletion> {
        match self {
            Self::Ordinary { url, rx } => poll(rx).map(|result| {
                TableStoreCompletion::Ordinary(match result {
                    Ok(success) => TableFetchOutcome::Succeeded(success),
                    Err(error) => TableFetchOutcome::Failed(TableFetchFailure {
                        url: url.clone(),
                        error: format!("failed to store difficulty table: {error:#}"),
                    }),
                })
            }),
            Self::Account { generation, identity, rx } => {
                poll(rx).map(|result| TableStoreCompletion::Account {
                    generation: *generation,
                    identity: identity.clone(),
                    result,
                })
            }
            Self::Cleanup { rx } => poll(rx).map(TableStoreCompletion::Cleanup),
        }
    }
}

fn poll<T>(rx: &Receiver<Result<T>>) -> Option<Result<T>> {
    match rx.try_recv() {
        Ok(result) => Some(result),
        Err(TryRecvError::Empty) => None,
        Err(TryRecvError::Disconnected) => Some(Err(anyhow::anyhow!("table store disconnected"))),
    }
}

fn spawn_store<T: Send + 'static>(
    path: PathBuf,
    store: impl FnOnce(&mut LibraryDatabase) -> Result<T> + Send + 'static,
) -> Receiver<Result<T>> {
    let (tx, rx) = mpsc::channel();
    let error_tx = tx.clone();
    if let Err(error) = std::thread::Builder::new().name("table-store".into()).spawn(move || {
        let result = LibraryDatabase::open(&path).and_then(|mut db| store(&mut db));
        let _ = tx.send(result);
    }) {
        let _ = error_tx.send(Err(error.into()));
    }
    rx
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    #[test]
    fn table_store_wait_and_disconnect_never_block_the_caller() {
        let data = crate::bootstrap::profile_tests::ProfileTestDir::new();
        let boot = data.boot();
        let (release, blocked) = mpsc::channel();
        let rx = spawn_store(boot.app_paths.library_db.clone(), move |_| {
            blocked.recv_timeout(Duration::from_secs(10))?;
            Ok(42)
        });
        assert!(poll(&rx).is_none());
        release.send(()).unwrap();
        assert_eq!(rx.recv_timeout(Duration::from_secs(10)).unwrap().unwrap(), 42);
        let (tx, rx) = mpsc::channel::<Result<()>>();
        drop(tx);
        assert!(poll(&rx).unwrap().is_err());
    }

    #[test]
    fn table_store_completion_means_rows_are_committed() {
        let data = crate::bootstrap::profile_tests::ProfileTestDir::new();
        let boot = data.boot();
        let pending = PendingTableStore::ordinary(
            boot.app_paths.library_db.clone(),
            crate::table_cmd::profile::synthetic_table(100),
        );
        let deadline = Instant::now() + Duration::from_secs(10);
        let result = loop {
            if let Some(result) = pending.poll() {
                break result;
            }
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(1));
        };
        assert!(
            matches!(result, TableStoreCompletion::Ordinary(TableFetchOutcome::Succeeded(success)) if success.entries == 100)
        );
        assert_eq!(boot.library_db.list_difficulty_tables().unwrap().len(), 1);
        assert_eq!(
            boot.library_db
                .conn()
                .query_row("SELECT COUNT(*) FROM difficulty_table_entries", [], |row| row
                    .get::<_, i64>(0))
                .unwrap(),
            100
        );
    }

    #[test]
    #[ignore = "release database dispatch probe; run alone with --ignored --nocapture"]
    fn profile_table_store_dispatch() -> Result<()> {
        anyhow::ensure!(!cfg!(debug_assertions), "run with --release");
        for count in [1_000, 10_000, 50_000] {
            let data = crate::bootstrap::profile_tests::ProfileTestDir::new();
            let boot = data.boot();
            let mut samples = Vec::new();
            for iteration in 0..6 {
                let table = crate::table_cmd::profile::synthetic_table(count);
                let start = Instant::now();
                let pending = PendingTableStore::ordinary(boot.app_paths.library_db.clone(), table);
                let elapsed = start.elapsed().as_secs_f64() * 1000.0;
                let deadline = Instant::now() + Duration::from_secs(30);
                loop {
                    if let Some(result) = pending.poll() {
                        assert!(matches!(
                            result,
                            TableStoreCompletion::Ordinary(TableFetchOutcome::Succeeded(_))
                        ));
                        break;
                    }
                    assert!(Instant::now() < deadline);
                    std::thread::sleep(Duration::from_millis(1));
                }
                if iteration > 0 {
                    samples.push(elapsed);
                }
            }
            samples.sort_by(f64::total_cmp);
            eprintln!(
                "table_store entries={count} dispatch_median_ms={:.3} dispatch_max_ms={:.3}",
                samples[2], samples[4]
            );
        }
        Ok(())
    }
}

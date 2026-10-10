use super::*;
use crate::chart_source::{ChartLocator, archive_preview_file};
use crate::song_archive::{self, ArchiveControl};
use bmz_chart::import::{BmsRandomSource, import_chart_bytes_with_random_source};
use rusqlite::OptionalExtension;

/// A failed decoder/CRC must roll back the whole container, including earlier members.
pub(super) fn import_archive(
    db: &mut LibraryDatabase,
    container: &Path,
    root_id: i64,
    scanned_at: i64,
    force: bool,
    fingerprints: &HashMap<String, super::super::library_db::ChartFileFingerprint>,
    entries: &[&ChartFileEntry],
) -> Result<ScanReport> {
    let control = ArchiveControl::default();
    let index = song_archive::inspect(container, &control)?;
    if let Some(record) = song_archive::generation_record(container) {
        db.store_archive_generation(&record)?;
    }
    let archive_key = crate::storage::library_db::library_path_key(container);
    let previous: Option<(String, i64)> = db
        .conn()
        .query_row(
            "SELECT generation, import_version FROM song_archive_scans WHERE path = ?1",
            [&archive_key],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    let allowed = entries
        .iter()
        .map(|entry| crate::storage::library_db::library_path_key(&entry.path))
        .collect::<std::collections::HashSet<_>>();
    // Failed charts have no linked chart (import_version 0). They are recorded for this
    // generation, so an unchanged archive is not decoded again just to fail once more.
    let registered = entries.iter().all(|entry| {
        fingerprints.get(&crate::storage::library_db::library_path_key(&entry.path)).is_some_and(
            |fingerprint| matches!(fingerprint.import_version, 0 | CHART_IMPORT_VERSION),
        )
    });
    let mut report = ScanReport::default();
    if !force
        && registered
        && previous.as_ref().is_some_and(|(generation, version)| {
            generation == &index.generation.fingerprint && *version == CHART_IMPORT_VERSION
        })
    {
        report.summary.skipped = usize_to_u32(entries.len());
        return Ok(report);
    }
    let modified_at = std::fs::metadata(container)?
        .modified()?
        .duration_since(UNIX_EPOCH)
        .map(|time| time.as_secs() as i64)
        .unwrap_or(0);
    let started = Instant::now();
    let tx = db.conn_mut().transaction()?;
    let scanned = song_archive::scan_charts(container, &control, |entry, bytes| {
        let path =
            ChartLocator::Archive { container: container.to_path_buf(), entry: entry.name.clone() }
                .to_path_buf();
        if !allowed.contains(&crate::storage::library_db::library_path_key(&path)) {
            return Ok(());
        }
        let parsed = catch_unwind(AssertUnwindSafe(|| {
            import_chart_bytes_with_random_source(&path, bytes, BmsRandomSource::Seed(None), false)
        }));
        let result = parsed.unwrap_or_else(|_| {
            Err(ImportError::Parse {
                path: path.clone(),
                message: "chart import panicked".to_string(),
            })
        });
        match result {
            Ok(mut imported) => {
                imported.chart.metadata.preview_file = archive_preview_file(
                    &index,
                    &entry.name,
                    &imported.chart.metadata.preview_file,
                );
                let record = ChartImportRecord {
                    root_id: Some(root_id),
                    file_path: &path,
                    file_size: bytes.len() as u64,
                    modified_at,
                    scanned_at,
                    chart: &imported.chart,
                };
                let (_, chart_file_id) = LibraryDatabase::write_chart_import(&tx, &record)?;
                report.summary.warnings += LibraryDatabase::write_import_warnings(
                    &tx,
                    chart_file_id,
                    &imported.warnings,
                    scanned_at,
                )? as u32;
                report.summary.imported += 1;
            }
            Err(error) => {
                let message = error.to_string();
                LibraryDatabase::write_failed_chart(
                    &tx,
                    Some(root_id),
                    &path,
                    bytes.len() as u64,
                    modified_at,
                    scanned_at,
                    &message,
                )?;
                report.summary.failed += 1;
                report.failures.push(ScanFailure { path, message });
            }
        }
        Ok(())
    })?;
    anyhow::ensure!(
        scanned.generation == index.generation,
        "archive changed between discovery and import"
    );
    // A changed archive or CHART_IMPORT_VERSION retries failed charts; an unchanged one
    // keeps its recorded failures instead of re-decoding the whole container each scan.
    tx.execute("INSERT INTO song_archive_scans (path, generation, import_version) VALUES (?1, ?2, ?3)
        ON CONFLICT(path) DO UPDATE SET generation = excluded.generation, import_version = excluded.import_version",
        rusqlite::params![archive_key, scanned.generation.fingerprint, CHART_IMPORT_VERSION])?;
    tx.commit()?;
    report.timing.parse_ms = started.elapsed().as_millis();
    Ok(report)
}

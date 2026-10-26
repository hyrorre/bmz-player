use std::path::Path;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::thread::JoinHandle;
use std::time::Instant;

use crate::chart_source::ChartLocator;
use crate::song_archive::ArchiveControl;
use crate::storage::library_db::LibraryDatabase;

#[derive(Default)]
pub(super) struct SelectArchivePreload {
    target: Option<(i64, String)>,
    started: bool,
    worker: Option<(Arc<AtomicBool>, JoinHandle<()>)>,
}

impl SelectArchivePreload {
    pub(super) fn update(
        &mut self,
        target: Option<(i64, &str)>,
        stable: bool,
        db_path: &std::path::Path,
    ) {
        if target != self.target.as_ref().map(|(id, folder)| (*id, folder.as_str())) {
            if let Some((cancel, _)) = &self.worker {
                cancel.store(true, Ordering::Relaxed);
            }
            self.target = target.map(|(id, folder)| (id, folder.to_owned()));
            self.started = false;
        }
        if self.worker.as_ref().is_some_and(|(_, worker)| worker.is_finished())
            && let Some((_, worker)) = self.worker.take()
        {
            let _ = worker.join();
        }
        if self.worker.is_some() || self.started || !stable {
            return;
        }
        let Some((chart_id, folder)) = self.target.clone() else {
            return;
        };
        let db_path = db_path.to_path_buf();
        let cancel = Arc::new(AtomicBool::new(false));
        let control = ArchiveControl { cancelled: Some(cancel.clone()), ..Default::default() };
        let worker =
            std::thread::Builder::new().name("select-archive-preload".into()).spawn(move || {
                crate::generated_preview::lower_current_thread_priority();
                let started = Instant::now();
                let result = prefetch(&db_path, chart_id, &folder, &control);
                if !control.cancelled.as_ref().is_some_and(|flag| flag.load(Ordering::Relaxed)) {
                    match result {
                        Ok(true) => tracing::info!(
                            chart_id,
                            elapsed_ms = started.elapsed().as_millis(),
                            "selected archive prefetch complete"
                        ),
                        Ok(false) => {}
                        Err(error) => {
                            tracing::debug!(chart_id, %error, "selected archive prefetch skipped")
                        }
                    }
                }
            });
        match worker {
            Ok(worker) => {
                self.started = true;
                self.worker = Some((cancel, worker));
            }
            Err(error) => tracing::warn!(%error, "failed to start selected archive prefetch"),
        }
    }
}

fn prefetch(
    db_path: &Path,
    chart_id: i64,
    folder: &str,
    control: &ArchiveControl,
) -> anyhow::Result<bool> {
    control.check()?;
    let db = LibraryDatabase::open(db_path)?;
    if let Some(source) = db.available_chart_source(chart_id)?
        && source.chart.folder_path == folder
        && let ChartLocator::Archive { container, entry } = source.locator()?
    {
        return crate::archive_resources::prefetch_chart(&container, &entry, control);
    }
    Ok(false)
}

impl Drop for SelectArchivePreload {
    fn drop(&mut self) {
        if let Some((cancel, worker)) = self.worker.take() {
            cancel.store(true, Ordering::Relaxed);
            let _ = worker.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bootstrap::profile_tests::ProfileTestDir;
    use crate::config::app_config::{AppConfig, PathEntry, ScanConfig};

    #[test]
    fn stable_archive_selection_primes_resources_once_and_change_cancels_it() {
        let data = ProfileTestDir::new();
        let mut boot = data.boot();
        let root = data.paths.data_dir.join("prefetch-songs");
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("song.7z");
        let mut writer = sevenz_rust2::ArchiveWriter::create(&path).unwrap();
        let entries = [
            ("chart.bms", &b"#TITLE Prefetch\n#BPM 120\n#WAV01 kick.wav\n#00111:01\n"[..]),
            ("kick.wav", &b"audio"[..]),
            ("unused.wav", &b"unused"[..]),
        ];
        writer
            .push_archive_entries(
                entries
                    .iter()
                    .map(|(name, _)| sevenz_rust2::ArchiveEntry::new_file(name))
                    .collect(),
                entries
                    .iter()
                    .map(|(_, bytes)| sevenz_rust2::SourceReader::new(std::io::Cursor::new(*bytes)))
                    .collect(),
            )
            .unwrap();
        writer.finish().unwrap();
        let roots = [PathEntry {
            path: root.to_string_lossy().into_owned(),
            enabled: true,
            recursive: true,
        }];
        boot.library_db.set_configured_song_roots(&roots).unwrap();
        crate::storage::scan::scan_song_roots(
            &mut boot.library_db,
            &roots,
            &ScanConfig { use_everything: false, ..AppConfig::default().scan },
            10,
            false,
        )
        .unwrap();
        let chart = boot
            .library_db
            .list_all_charts()
            .unwrap()
            .into_iter()
            .find(|chart| chart.folder_path.contains("song.7z!"))
            .unwrap();
        let target = Some((chart.chart_id, chart.folder_path.as_str()));
        let mut runtime = SelectArchivePreload::default();
        runtime.update(target, false, &data.paths.library_db);
        assert!(runtime.worker.is_none());
        runtime.update(target, true, &data.paths.library_db);
        let cancel = runtime.worker.as_ref().unwrap().0.clone();
        let started = Instant::now();
        while !runtime.worker.as_ref().unwrap().1.is_finished() {
            assert!(started.elapsed() < std::time::Duration::from_secs(10));
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        runtime.update(target, true, &data.paths.library_db);
        assert!(runtime.started);
        assert!(runtime.worker.is_none());
        assert!(bmz_song_resource::find(&path).unwrap().loaded("kick.wav").is_some());
        // A completed choice is not repeatedly loaded on every frame.
        runtime.update(target, true, &data.paths.library_db);
        assert!(runtime.worker.is_none());
        // Cancellation also applies to a worker waiting on another archive reader.
        runtime.worker = Some((cancel.clone(), std::thread::spawn(|| {})));
        runtime.update(None, false, &data.paths.library_db);
        assert!(cancel.load(Ordering::Relaxed));
    }
}

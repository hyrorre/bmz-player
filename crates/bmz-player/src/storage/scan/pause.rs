use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::Instant;

use rayon::prelude::*;

use crate::song_archive::ArchiveControl;

/// Holds a background scan while the app is outside Select.
///
/// A scan stops only where it holds no SQLite write transaction. Rayon workers never
/// wait on the pause: work not yet started is handed back, and an archive read in
/// progress is cancelled and repeated after the scan resumes. Without a handle
/// ([`ScanPause::default`]) a scan never pauses.
#[derive(Clone, Default)]
pub struct ScanPause {
    state: Option<Arc<PauseState>>,
}

struct PauseState {
    /// Read by archive readers through [`ArchiveControl::cancelled`].
    flag: Arc<AtomicBool>,
    /// Counts pauses, so an interruption is recognized after a quick resume too.
    epoch: AtomicU64,
    paused: Mutex<bool>,
    resumed: Condvar,
}

impl ScanPause {
    pub fn new() -> Self {
        Self {
            state: Some(Arc::new(PauseState {
                flag: Arc::new(AtomicBool::new(false)),
                epoch: AtomicU64::new(0),
                paused: Mutex::new(false),
                resumed: Condvar::new(),
            })),
        }
    }

    pub fn set_paused(&self, paused: bool) {
        let Some(state) = &self.state else {
            return;
        };
        let mut current = state.paused.lock().unwrap_or_else(|error| error.into_inner());
        if *current == paused {
            return;
        }
        *current = paused;
        if paused {
            state.epoch.fetch_add(1, Ordering::SeqCst);
        }
        state.flag.store(paused, Ordering::SeqCst);
        if !paused {
            state.resumed.notify_all();
        }
    }

    pub fn is_paused(&self) -> bool {
        self.state.as_ref().is_some_and(|state| state.flag.load(Ordering::SeqCst))
    }

    /// Blocks the calling scan thread until the app resumes the scan. Never call it
    /// on a rayon worker.
    pub(super) fn wait(&self) {
        let Some(state) = &self.state else {
            return;
        };
        let mut paused = state.paused.lock().unwrap_or_else(|error| error.into_inner());
        if !*paused {
            return;
        }
        let started = Instant::now();
        tracing::info!("song scan paused outside Select");
        while *paused {
            paused = state.resumed.wait(paused).unwrap_or_else(|error| error.into_inner());
        }
        tracing::info!(paused_ms = started.elapsed().as_millis(), "song scan resumed");
    }

    pub(super) fn epoch(&self) -> u64 {
        self.state.as_ref().map_or(0, |state| state.epoch.load(Ordering::SeqCst))
    }

    /// Whether work begun at `epoch` may have been cut short by a pause.
    pub(super) fn interrupted_since(&self, epoch: u64) -> bool {
        self.is_paused() || self.epoch() != epoch
    }

    /// Archive reads give up as soon as the scan pauses.
    pub(super) fn archive_control(&self) -> ArchiveControl {
        ArchiveControl {
            cancelled: self.state.as_ref().map(|state| Arc::clone(&state.flag)),
            ..Default::default()
        }
    }

    /// Repeats an archive operation that a pause cancelled, waiting on this thread.
    pub(super) fn retry_archive<T>(
        &self,
        mut operation: impl FnMut(&ArchiveControl) -> anyhow::Result<T>,
    ) -> anyhow::Result<T> {
        let control = self.archive_control();
        loop {
            self.wait();
            let epoch = self.epoch();
            match operation(&control) {
                Err(_) if self.interrupted_since(epoch) => continue,
                result => return result,
            }
        }
    }

    /// Maps `items` in parallel and returns the results in order. `work` returns
    /// `None` when a pause interrupted it; such items, and items not started while
    /// paused, run again after this thread waits for the scan to resume.
    pub(super) fn par_map<T: Sync, R: Send>(
        &self,
        items: &[T],
        work: impl Fn(&T) -> Option<R> + Sync,
    ) -> Vec<R> {
        let mut results: Vec<Option<R>> = items.iter().map(|_| None).collect();
        let mut pending: Vec<usize> = (0..items.len()).collect();
        loop {
            let done: Vec<(usize, Option<R>)> = pending
                .par_iter()
                .map(|&index| {
                    let result = if self.is_paused() { None } else { work(&items[index]) };
                    (index, result)
                })
                .collect();
            pending.clear();
            for (index, result) in done {
                match result {
                    Some(result) => results[index] = Some(result),
                    None => pending.push(index),
                }
            }
            if pending.is_empty() {
                break;
            }
            self.wait();
        }
        results.into_iter().map(|result| result.expect("every item completed")).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;
    use std::time::Duration;

    #[test]
    fn default_handle_never_pauses() {
        let pause = ScanPause::default();
        pause.set_paused(true);
        assert!(!pause.is_paused());
        pause.wait();
        assert!(pause.archive_control().cancelled.is_none());
    }

    #[test]
    fn wait_blocks_until_resumed() {
        let pause = ScanPause::new();
        pause.set_paused(true);
        let worker = {
            let pause = pause.clone();
            std::thread::spawn(move || {
                pause.wait();
                Instant::now()
            })
        };
        std::thread::sleep(Duration::from_millis(50));
        assert!(!worker.is_finished());
        let resumed_at = Instant::now();
        pause.set_paused(false);
        assert!(worker.join().unwrap() >= resumed_at);
    }

    #[test]
    fn par_map_repeats_paused_items_in_order_without_blocking_workers() {
        let pause = ScanPause::new();
        let calls = AtomicUsize::new(0);
        let items: Vec<usize> = (0..64).collect();
        let resumer = {
            let pause = pause.clone();
            std::thread::spawn(move || {
                std::thread::sleep(Duration::from_millis(50));
                pause.set_paused(false);
            })
        };
        let results = pause.par_map(&items, |&item| {
            if calls.fetch_add(1, Ordering::SeqCst) == 3 {
                // The pause arrives while this item is in progress.
                pause.set_paused(true);
                return None;
            }
            Some(item * 2)
        });
        resumer.join().unwrap();
        assert_eq!(results, items.iter().map(|item| item * 2).collect::<Vec<_>>());
        assert!(calls.load(Ordering::SeqCst) > items.len());
    }

    #[test]
    fn retry_archive_repeats_an_operation_cancelled_by_a_pause() {
        let pause = ScanPause::new();
        let mut attempts = 0;
        let result = pause.retry_archive(|control| {
            attempts += 1;
            if attempts == 1 {
                pause.set_paused(true);
                let error = control.check().unwrap_err();
                let resumer = pause.clone();
                std::thread::spawn(move || {
                    std::thread::sleep(Duration::from_millis(20));
                    resumer.set_paused(false);
                });
                return Err(error);
            }
            control.check()?;
            Ok(attempts)
        });
        assert_eq!(result.unwrap(), 2);
        // A real failure is not repeated.
        let failed: anyhow::Result<()> = pause.retry_archive(|_| anyhow::bail!("broken"));
        assert!(failed.is_err());
    }
}

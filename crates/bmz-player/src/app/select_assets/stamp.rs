use std::path::Path;
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::time::{Duration, Instant};

const REFRESH_INTERVAL: Duration = Duration::from_secs(1);

/// Only the selected source is polled. At most one metadata request is in flight;
/// rapid cursor changes coalesce rather than starting a thread for every chart.
#[derive(Default)]
pub(super) struct SelectedArchiveStamp {
    folder: Option<String>,
    archive: bool,
    generation: u64,
    value: Option<String>,
    checked_at: Option<Instant>,
    pending: Option<(u64, Receiver<String>)>,
}

pub(super) fn is_archive(folder: &str) -> bool {
    crate::chart_source::ChartLocator::parse(Path::new(folder))
        .is_ok_and(|locator| locator.is_archive())
}

impl SelectedArchiveStamp {
    pub(super) fn update(&mut self, folder: Option<&str>, now: Instant) {
        self.update_with(folder, now, |folder| {
            let (tx, rx) = mpsc::channel();
            std::thread::Builder::new().name("select-archive-stamp".into()).spawn(move || {
                if let Some(stamp) = crate::chart_asset::archive_asset_stamp(&folder) {
                    let _ = tx.send(stamp);
                }
            })?;
            Ok(rx)
        });
    }

    fn update_with(
        &mut self,
        folder: Option<&str>,
        now: Instant,
        spawn: impl FnOnce(String) -> std::io::Result<Receiver<String>>,
    ) {
        if self.folder.as_deref() != folder {
            self.folder = folder.map(str::to_owned);
            self.archive = folder.is_some_and(is_archive);
            self.generation = self.generation.wrapping_add(1);
            self.value = None;
            self.checked_at = None;
        }
        if let Some((generation, rx)) = &self.pending {
            let result = match rx.try_recv() {
                Err(TryRecvError::Empty) => return,
                result => result,
            };
            if *generation == self.generation {
                self.checked_at = Some(now);
                if let Ok(value) = result {
                    self.value = Some(value);
                }
            }
            self.pending = None;
        }
        if !self.archive
            || self.checked_at.is_some_and(|checked| now.duration_since(checked) < REFRESH_INTERVAL)
        {
            return;
        }
        if let Some(folder) = &self.folder {
            self.checked_at = Some(now);
            match spawn(folder.clone()) {
                Ok(rx) => self.pending = Some((self.generation, rx)),
                Err(error) => tracing::warn!(%error, "failed to start archive metadata check"),
            }
        }
    }

    /// Outer None means an archive has not been checked yet. Native sources need
    /// no stamp and remain immediately usable, including outside the Select loop.
    pub(super) fn get(&self, folder: &str) -> Option<Option<&str>> {
        if self.folder.as_deref() == Some(folder) {
            if self.archive { self.value.as_deref().map(Some) } else { Some(None) }
        } else if is_archive(folder) {
            None
        } else {
            Some(None)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coalesces_changes_and_discards_old_generation_even_when_returning_to_same_folder() {
        let mut cache = SelectedArchiveStamp::default();
        let now = Instant::now();
        let (tx, rx) = mpsc::channel();
        cache.update_with(Some("a.zip!/song"), now, |_| Ok(rx));
        for folder in ["b.zip!/song", "a.zip!/song"] {
            cache.update_with(Some(folder), now, |_| panic!("worker still busy"));
            assert_eq!(cache.get(folder), None);
        }
        tx.send("obsolete".into()).unwrap();
        let (tx, rx) = mpsc::channel();
        cache.update_with(Some("a.zip!/song"), now, |_| Ok(rx));
        assert_eq!(cache.get("a.zip!/song"), None);
        tx.send("current".into()).unwrap();
        cache.update_with(Some("a.zip!/song"), now, |_| panic!("already checked"));
        assert_eq!(cache.get("a.zip!/song"), Some(Some("current")));
    }

    #[test]
    fn refreshes_in_background_and_keeps_last_stamp_until_completion() {
        let mut cache = SelectedArchiveStamp::default();
        let now = Instant::now();
        let (tx, rx) = mpsc::channel();
        cache.update_with(Some("a.zip!/song"), now, |_| Ok(rx));
        tx.send("before".into()).unwrap();
        cache.update_with(Some("a.zip!/song"), now, |_| panic!("checked"));
        cache.update_with(Some("a.zip!/song"), now, |_| panic!("checked"));
        let (tx, rx) = mpsc::channel();
        cache.update_with(Some("a.zip!/song"), now + REFRESH_INTERVAL, |_| Ok(rx));
        assert_eq!(cache.get("a.zip!/song"), Some(Some("before")));
        tx.send("after".into()).unwrap();
        cache.update_with(Some("a.zip!/song"), now + REFRESH_INTERVAL, |_| panic!("checked"));
        assert_eq!(cache.get("a.zip!/song"), Some(Some("after")));
        cache.update_with(Some("native/song"), now + REFRESH_INTERVAL, |_| panic!("native"));
        assert_eq!(cache.get("native/song"), Some(None));
        assert_eq!(cache.get("a.zip!/song"), None);
    }
}

use super::*;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

/// One worker per output stream. The callback's two fixed-capacity buffers are
/// exchanged under a short lock; PCM is destroyed after releasing that lock.
pub(super) struct SourceReaper {
    stop: Arc<AtomicBool>,
    thread: std::thread::Thread,
    sources: RetiredOutputSources,
}

impl SourceReaper {
    pub(super) fn new(sources: RetiredOutputSources) -> std::io::Result<Self> {
        let stop = Arc::new(AtomicBool::new(false));
        let stopping = stop.clone();
        let worker_sources = sources.clone();
        let handle =
            std::thread::Builder::new().name("audio-source-retire".into()).spawn(move || {
                let mut batch = Vec::with_capacity(OUTPUT_COMMAND_QUEUE_CAPACITY);
                loop {
                    let stop_before_drain = stopping.load(Ordering::Acquire);
                    if let Ok(mut sources) = worker_sources.lock() {
                        std::mem::swap(&mut *sources, &mut batch);
                    }
                    batch.clear();
                    if stop_before_drain {
                        break;
                    }
                    // Also drain when no window frames are being produced.
                    std::thread::park_timeout(Duration::from_millis(20));
                }
                // At shutdown the stream has already stopped. The remaining queue
                // and its last PCM owners are released on this worker as well.
                drop(worker_sources);
            })?;
        let thread = handle.thread().clone();
        // Shutdown is signalled by the guard; the UI never joins a PCM Drop.
        drop(handle);
        Ok(Self { stop, thread, sources })
    }

    pub(super) fn wake(&self) {
        // Redraw calls this even with no retired sources. Avoid waking a thread
        // every frame; contention is handled by its periodic drain instead.
        let pending = self.sources.try_lock().is_ok_and(|sources| !sources.is_empty());
        if pending {
            self.thread.unpark();
        }
    }
}

impl Drop for SourceReaper {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        self.thread.unpark();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reaper_releases_sources_without_waiting_on_the_window_or_callback() {
        let queue = Arc::new(Mutex::new(Vec::with_capacity(OUTPUT_COMMAND_QUEUE_CAPACITY)));
        let engine = Arc::new(Mutex::new(AudioEngine::new(48_000)));
        let weak = Arc::downgrade(&engine);
        let reaper = SourceReaper::new(queue.clone()).unwrap();
        let mut locked = queue.lock().unwrap();
        locked.push(RenderAudioSource {
            id: 1,
            kind: CpalOutputSourceKind::Play,
            active: false,
            engine: RenderAudioEngine::Legacy(engine),
        });
        reaper.wake(); // returns even while the queue mutex is held here
        drop(locked);
        let deadline = Instant::now() + Duration::from_secs(5);
        while weak.upgrade().is_some() {
            assert!(Instant::now() < deadline);
            std::thread::yield_now();
        }
        assert_eq!(queue.lock().unwrap().capacity(), OUTPUT_COMMAND_QUEUE_CAPACITY);
        drop(reaper);
    }
}

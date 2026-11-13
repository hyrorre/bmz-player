use super::*;
use std::sync::{
    OnceLock,
    atomic::AtomicUsize,
    mpsc::{Sender, channel},
};

// Includes decoders waiting for native teardown. A stuck demuxer cannot create
// an unbounded backlog of threads/frames when the user keeps changing scenes.
const MAX_LIVE_DECODERS: usize = 64;
static LIVE: AtomicUsize = AtomicUsize::new(0);
static REAPER: OnceLock<std::result::Result<Sender<RetiredDecoder>, String>> = OnceLock::new();

struct Permit(&'static AtomicUsize);
impl Permit {
    fn acquire(counter: &'static AtomicUsize) -> Option<Self> {
        let mut count = counter.load(Ordering::Acquire);
        loop {
            if count >= MAX_LIVE_DECODERS {
                return None;
            }
            match counter.compare_exchange_weak(
                count,
                count + 1,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => return Some(Self(counter)),
                Err(actual) => count = actual,
            }
        }
    }
}
impl Drop for Permit {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::Release);
    }
}

pub(crate) struct Retirement {
    sender: Sender<RetiredDecoder>,
    permit: Permit,
}

struct RetiredDecoder {
    thread: Option<std::thread::JoinHandle<()>>,
    receiver: Option<Receiver<QueuedDecodedFrame>>,
    pending: VecDeque<DecodedFrame>,
    current: Option<DecodedFrame>,
    clocked: Option<Arc<Mutex<ClockedFrameState>>>,
    _channel_decode: Arc<ChannelDecodeState>,
    // Released after the thread and frame buffers, including on error.
    _permit: Permit,
}

impl RetiredDecoder {
    fn finish(mut self) {
        drop(self.receiver.take());
        if let Some(handle) = self.thread.take()
            && handle.join().is_err()
        {
            tracing::warn!("video decode thread panicked before retirement");
        }
        self.pending.clear();
        self.current = None;
        self.clocked = None;
    }
}

impl Retirement {
    pub(crate) fn new() -> Result<Self> {
        let permit = Permit::acquire(&LIVE).ok_or_else(|| {
            anyhow::anyhow!(
                "video decoder limit reached ({MAX_LIVE_DECODERS}, including pending teardown)"
            )
        })?;
        let sender = REAPER
            .get_or_init(|| {
                let (tx, rx) = channel::<RetiredDecoder>();
                std::thread::Builder::new()
                    .name("video-retire".into())
                    .spawn(move || {
                        while let Ok(retired) = rx.recv() {
                            retired.finish();
                        }
                    })
                    .map(|_| tx)
                    .map_err(|error| error.to_string())
            })
            .as_ref()
            .map_err(|error| anyhow::anyhow!("video retirement worker: {error}"))?
            .clone();
        Ok(Self { sender, permit })
    }

    pub(crate) fn retire(self, decoder: &mut VideoBgaDecoder) {
        // The channel is logically bounded by permits, so send neither waits
        // for capacity nor drops native/video resources on the frame thread.
        let retired = RetiredDecoder {
            thread: decoder.decode_thread.take(),
            receiver: decoder.receiver.take(),
            pending: std::mem::take(&mut decoder.pending),
            current: decoder.current.take(),
            clocked: decoder.clocked_frames.take(),
            _channel_decode: std::mem::take(&mut decoder.channel_decode),
            _permit: self.permit,
        };
        if let Err(error) = self.sender.send(retired) {
            tracing::error!("video retirement worker disconnected");
            // Only a worker failure uses synchronous cleanup; never leak a native thread.
            error.0.finish();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn admission_counts_pending_teardown_until_permit_is_released() {
        static COUNT: AtomicUsize = AtomicUsize::new(0);
        let mut permits: Vec<_> =
            (0..MAX_LIVE_DECODERS).map(|_| Permit::acquire(&COUNT).unwrap()).collect();
        assert!(Permit::acquire(&COUNT).is_none());
        permits.pop();
        let replacement = Permit::acquire(&COUNT).unwrap();
        assert!(Permit::acquire(&COUNT).is_none());
        drop((permits, replacement));
        assert_eq!(COUNT.load(Ordering::Acquire), 0);
    }
}

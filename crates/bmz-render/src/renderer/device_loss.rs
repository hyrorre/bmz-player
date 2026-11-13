use std::sync::{
    Mutex,
    atomic::{AtomicBool, Ordering},
};

/// The callback only publishes state. No renderer/window calls or GPU work are
/// allowed here. Healthy frames read one atomic flag and never lock the mutex.
#[derive(Default)]
pub(super) struct DeviceLoss {
    lost: AtomicBool,
    shutting_down: AtomicBool,
    message: Mutex<Option<String>>,
}

impl DeviceLoss {
    pub(super) fn notify(&self, reason: wgpu::DeviceLostReason, message: String) {
        if reason == wgpu::DeviceLostReason::Destroyed && self.shutting_down.load(Ordering::Acquire)
        {
            return;
        }
        let mut slot = self.message.lock().unwrap_or_else(|error| error.into_inner());
        if !self.lost.load(Ordering::Relaxed) {
            *slot = Some(format!("DeviceLost ({reason:?}): {message}"));
            self.lost.store(true, Ordering::Release);
        }
    }

    pub(super) fn is_lost(&self) -> bool {
        self.lost.load(Ordering::Acquire)
    }

    pub(super) fn take_message(&self) -> Option<String> {
        if !self.is_lost() {
            return None;
        }
        self.message.lock().unwrap_or_else(|error| error.into_inner()).take()
    }

    pub(super) fn begin_shutdown(&self) {
        self.shutting_down.store(true, Ordering::Release);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn destruction_during_shutdown_is_not_a_device_failure() {
        let state = DeviceLoss::default();
        state.begin_shutdown();
        state.notify(wgpu::DeviceLostReason::Destroyed, String::new());
        assert!(!state.is_lost());
        assert!(state.take_message().is_none());
    }

    #[test]
    fn unexpected_loss_is_persistent_and_notified_once() {
        let state = DeviceLoss::default();
        state.notify(wgpu::DeviceLostReason::Unknown, "original cause".into());
        state.notify(wgpu::DeviceLostReason::Destroyed, "later notice".into());
        assert!(state.take_message().unwrap().contains("original cause"));
        assert!(state.take_message().is_none());
        assert!(state.is_lost());
    }
}

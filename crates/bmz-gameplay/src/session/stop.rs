use std::sync::atomic::{AtomicU8, Ordering};

use super::GameSession;

#[derive(Clone, Copy)]
#[repr(u8)]
pub enum PlayEndRequest {
    ManualStop = 1,
    PracticeRoundEnd,
    Finish,
}

#[derive(Debug, Default)]
pub struct PlayEndSignal(AtomicU8);

impl PlayEndSignal {
    pub fn request(&self, request: PlayEndRequest) {
        self.0.store(request as u8, Ordering::Release);
    }

    pub(super) fn is_pending(&self) -> bool {
        self.0.load(Ordering::Relaxed) != 0
    }
}

impl GameSession {
    pub(super) fn play_end_reached(&mut self) -> bool {
        self.manually_stopped
            || self.state == super::PlayState::Finished
            || self.apply_play_end_request()
    }

    /// Apply an exit request at a scoring boundary on the session owner.
    pub(crate) fn apply_play_end_request(&mut self) -> bool {
        if !self.play_end.is_pending() {
            return false;
        }
        match self.play_end.0.swap(0, Ordering::AcqRel) {
            request if request == PlayEndRequest::ManualStop as u8 => self.request_manual_stop(),
            request if request == PlayEndRequest::PracticeRoundEnd as u8 => {
                self.request_practice_round_end()
            }
            request if request == PlayEndRequest::Finish as u8 => self.request_finish(),
            _ => false,
        }
    }
}

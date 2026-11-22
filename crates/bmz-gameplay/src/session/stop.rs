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
            || self.apply_play_end_request(false)
    }

    /// Finish uses a completed scoring update as its replay cutoff. Manual stops can interrupt it.
    pub(crate) fn apply_play_end_request(&mut self, allow_finish: bool) -> bool {
        // Outside Playing a Finish can never apply. Consume it so it is not left pending.
        let allow_finish = allow_finish || self.state != super::PlayState::Playing;
        let Ok(request) =
            self.play_end.0.try_update(Ordering::AcqRel, Ordering::Acquire, |request| {
                (request != 0 && (allow_finish || request != PlayEndRequest::Finish as u8))
                    .then_some(0)
            })
        else {
            return false;
        };
        match request {
            request if request == PlayEndRequest::ManualStop as u8 => self.request_manual_stop(),
            request if request == PlayEndRequest::PracticeRoundEnd as u8 => {
                self.request_practice_round_end()
            }
            request if request == PlayEndRequest::Finish as u8 => self.request_finish(),
            _ => false,
        }
    }
}

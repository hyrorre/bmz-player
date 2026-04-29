use bmz_core::input::InputEvent;
use bmz_core::replay::ReplayEvent;
use bmz_core::time::TimeUs;

#[derive(Debug, Clone, Default)]
pub struct ReplayRecorder {
    pub events: Vec<ReplayEvent>,
}

impl ReplayRecorder {
    pub fn record(&mut self, input: InputEvent) {
        self.events.push(ReplayEvent { lane: input.lane, kind: input.kind, time: input.time });
    }
}

#[derive(Debug, Clone, Default)]
pub struct ReplayPlayer {
    pub events: Vec<ReplayEvent>,
    pub next_index: usize,
}

impl ReplayPlayer {
    pub fn poll_until(&mut self, now: TimeUs) -> Vec<InputEvent> {
        let mut out = Vec::new();
        while let Some(event) = self.events.get(self.next_index).copied() {
            if event.time > now {
                break;
            }
            self.next_index += 1;
            out.push(InputEvent {
                lane: event.lane,
                kind: event.kind,
                time: event.time,
                source: bmz_core::input::InputSource::Replay,
            });
        }
        out
    }
}

use bmz_core::input::InputEvent;
use bmz_core::replay::ReplayEvent;
use bmz_core::time::TimeUs;

#[derive(Debug, Clone, Default)]
pub struct ReplayRecorder {
    pub events: Vec<ReplayEvent>,
}

impl ReplayRecorder {
    pub fn record(&mut self, input: InputEvent) {
        self.events.push(ReplayEvent {
            lane: input.lane,
            kind: input.kind,
            time: input.time,
            device_kind: input.device_kind,
            scratch_direction: input.scratch_direction,
        });
    }
}

#[derive(Debug, Clone, Default)]
pub struct ReplayPlayer {
    pub branch_decisions: Option<Vec<bmz_core::replay::BranchDecision>>,
    pub events: Vec<ReplayEvent>,
    pub next_index: usize,
}

impl ReplayPlayer {
    pub fn new(events: Vec<ReplayEvent>) -> Self {
        Self { events, ..Self::default() }
    }
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
                device_kind: event.device_kind,
                scratch_direction: event.scratch_direction,
            });
        }
        out
    }

    pub fn skip_before(&mut self, start_time: TimeUs) {
        self.next_index = self.events.partition_point(|event| event.time < start_time);
    }
}

#[cfg(test)]
mod tests {
    use bmz_core::input::{InputDeviceKind, InputKind, InputSource, ScratchDirection};
    use bmz_core::lane::Lane;

    use super::*;

    #[test]
    fn recorder_and_player_preserve_scratch_direction() {
        let input = InputEvent {
            lane: Lane::Scratch,
            kind: InputKind::Press,
            time: TimeUs(123_456),
            source: InputSource::Human,
            device_kind: InputDeviceKind::Controller,
            scratch_direction: Some(ScratchDirection::Up),
        };
        let mut recorder = ReplayRecorder::default();
        recorder.record(input);

        assert_eq!(recorder.events[0].scratch_direction, Some(ScratchDirection::Up));

        let mut player = ReplayPlayer::new(recorder.events);
        let replayed = player.poll_until(TimeUs(123_456));
        assert_eq!(replayed.len(), 1);
        assert_eq!(replayed[0].source, InputSource::Replay);
        assert_eq!(replayed[0].scratch_direction, Some(ScratchDirection::Up));
    }

    #[test]
    fn player_skip_before_keeps_boundary_event() {
        let mut player = ReplayPlayer {
            branch_decisions: None,
            events: vec![
                ReplayEvent {
                    lane: bmz_core::lane::Lane::Key1,
                    kind: InputKind::Press,
                    time: TimeUs(1),
                    device_kind: InputDeviceKind::Keyboard,
                    scratch_direction: None,
                },
                ReplayEvent {
                    lane: bmz_core::lane::Lane::Key1,
                    kind: InputKind::Release,
                    time: TimeUs(2),
                    device_kind: InputDeviceKind::Keyboard,
                    scratch_direction: None,
                },
            ],
            next_index: 0,
        };

        player.skip_before(TimeUs(2));

        assert_eq!(player.poll_until(TimeUs(2)).len(), 1);
    }
}

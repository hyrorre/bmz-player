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
    pub events: Vec<ReplayEvent>,
    pub next_index: usize,
    /// Runtime-only chart-time cursor; never serialized into the replay file.
    pub next_scoring_time: Option<TimeUs>,
}

impl ReplayPlayer {
    /// Use a fixed chart-time cadence for replay misses and HCN integration, so
    /// batching inputs into different audio/render frames cannot change scoring.
    pub(crate) fn scoring_times_until(&mut self, now: TimeUs) -> Vec<TimeUs> {
        const STEP_US: i64 = 1_000;
        // READY does not score before chart zero, including negative recorded inputs.
        let mut time = self.next_scoring_time.unwrap_or(TimeUs(0));
        let mut times = Vec::new();
        while time <= now {
            times.push(time);
            time = TimeUs(time.0.saturating_add(STEP_US));
        }
        self.next_scoring_time = Some(time);
        times
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
        self.next_scoring_time = Some(start_time);
    }
}

#[cfg(test)]
mod tests {
    use bmz_core::input::{InputDeviceKind, InputKind, InputSource, ScratchDirection};
    use bmz_core::lane::Lane;

    use super::*;

    #[test]
    fn scoring_cursor_does_not_repeat_ticks_and_resumes_after_seek() {
        let mut replay = ReplayPlayer::default();
        assert_eq!(
            replay.scoring_times_until(TimeUs(2_500)),
            vec![TimeUs(0), TimeUs(1_000), TimeUs(2_000)]
        );
        assert!(replay.scoring_times_until(TimeUs(2_999)).is_empty());
        assert_eq!(replay.scoring_times_until(TimeUs(4_000)), vec![TimeUs(3_000), TimeUs(4_000)]);
        replay.skip_before(TimeUs(10_000));
        assert_eq!(replay.scoring_times_until(TimeUs(10_000)), vec![TimeUs(10_000)]);
    }

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

        let mut player =
            ReplayPlayer { events: recorder.events, next_index: 0, next_scoring_time: None };
        let replayed = player.poll_until(TimeUs(123_456));
        assert_eq!(replayed.len(), 1);
        assert_eq!(replayed[0].source, InputSource::Replay);
        assert_eq!(replayed[0].scratch_direction, Some(ScratchDirection::Up));
    }

    #[test]
    fn player_skip_before_keeps_boundary_event() {
        let mut player = ReplayPlayer {
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
            next_scoring_time: None,
        };

        player.skip_before(TimeUs(2));

        assert_eq!(player.poll_until(TimeUs(2)).len(), 1);
    }
}

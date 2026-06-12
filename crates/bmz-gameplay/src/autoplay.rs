use bmz_chart::model::{NoteKind, PlayableChart};
use bmz_core::input::{InputDeviceKind, InputEvent, InputKind, InputSource};
use bmz_core::lane::{LANE_COUNT, Lane};
use bmz_core::time::TimeUs;

#[derive(Debug, Clone)]
pub struct AutoplayController {
    next_note_index: [usize; LANE_COUNT],
}

impl Default for AutoplayController {
    fn default() -> Self {
        Self { next_note_index: [0; LANE_COUNT] }
    }
}

impl AutoplayController {
    pub fn poll_until(&mut self, chart: &PlayableChart, now: TimeUs) -> Vec<InputEvent> {
        let mut out = Vec::new();
        for lane in Lane::ALL {
            let notes = chart.notes_for_lane(lane);
            let index = &mut self.next_note_index[lane.index()];
            while let Some(note) = notes.get(*index) {
                if note.time > now {
                    break;
                }
                *index += 1;
                match note.kind {
                    NoteKind::Tap | NoteKind::LongStart => out.push(InputEvent {
                        lane,
                        kind: InputKind::Press,
                        time: note.time,
                        source: InputSource::Auto,
                        device_kind: InputDeviceKind::Keyboard,
                        scratch_direction: None,
                    }),
                    NoteKind::LongEnd => out.push(InputEvent {
                        lane,
                        kind: InputKind::Release,
                        time: note.time,
                        source: InputSource::Auto,
                        device_kind: InputDeviceKind::Keyboard,
                        scratch_direction: None,
                    }),
                    NoteKind::Invisible | NoteKind::Mine => {}
                }
            }
        }
        out
    }
}

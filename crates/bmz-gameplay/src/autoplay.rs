use bmz_chart::model::{NoteKind, PlayableChart};
use bmz_core::input::{InputDeviceKind, InputEvent, InputKind, InputSource, ScratchDirection};
use bmz_core::lane::{LANE_COUNT, Lane};
use bmz_core::time::TimeUs;

#[derive(Debug, Clone)]
pub struct AutoplayController {
    next_note_index: [usize; LANE_COUNT],
    lanes: [bool; LANE_COUNT],
    next_scratch_direction: [ScratchDirection; LANE_COUNT],
}

impl Default for AutoplayController {
    fn default() -> Self {
        Self {
            next_note_index: [0; LANE_COUNT],
            lanes: [true; LANE_COUNT],
            next_scratch_direction: [ScratchDirection::Down; LANE_COUNT],
        }
    }
}

impl AutoplayController {
    pub fn for_lanes(lanes: &[Lane]) -> Self {
        let mut enabled = [false; LANE_COUNT];
        for lane in lanes {
            enabled[lane.index()] = true;
        }
        Self {
            next_note_index: [0; LANE_COUNT],
            lanes: enabled,
            next_scratch_direction: [ScratchDirection::Down; LANE_COUNT],
        }
    }

    pub fn is_full(&self) -> bool {
        self.lanes.iter().all(|enabled| *enabled)
    }

    pub fn is_lane_enabled(&self, lane: Lane) -> bool {
        self.lanes[lane.index()]
    }

    pub fn poll_until(&mut self, chart: &PlayableChart, now: TimeUs) -> Vec<InputEvent> {
        let mut out = Vec::new();
        for lane in Lane::ALL {
            if !self.is_lane_enabled(lane) {
                continue;
            }
            let lane_index = lane.index();
            let notes = chart.notes_for_lane(lane);
            while let Some(note) = notes.get(self.next_note_index[lane_index]) {
                if note.time > now {
                    break;
                }
                self.next_note_index[lane_index] += 1;
                match note.kind {
                    NoteKind::Tap | NoteKind::LongStart => {
                        let scratch_direction = self.next_scratch_direction(lane);
                        out.push(InputEvent {
                            lane,
                            kind: InputKind::Press,
                            time: note.time,
                            source: InputSource::Auto,
                            device_kind: InputDeviceKind::Keyboard,
                            scratch_direction,
                        });
                    }
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

    fn next_scratch_direction(&mut self, lane: Lane) -> Option<ScratchDirection> {
        if !matches!(lane, Lane::Scratch | Lane::Scratch2) {
            return None;
        }
        let index = lane.index();
        let direction = self.next_scratch_direction[index];
        self.next_scratch_direction[index] = match direction {
            ScratchDirection::Up => ScratchDirection::Down,
            ScratchDirection::Down => ScratchDirection::Up,
        };
        Some(direction)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use bmz_chart::model::{
        BarLine, BgaArgbEvent, BgaAssetRef, BgaEvent, BgaKeyboundEvent, BgaOpacityEvent,
        ChartMetadata, ChartTextEvent, ChartVolumeEvent, JudgeRankEvent, LongNotePair, NoteEvent,
        ScrollEvent, SoundAssetRef, SoundEvent, SpeedEvent, SwBgaDefinition, TimingEvent,
    };
    use bmz_core::chart::ChartIdentity;
    use bmz_core::ids::NoteId;
    use bmz_core::time::ChartTick;

    use super::*;

    #[test]
    fn autoplay_alternates_scratch_press_directions() {
        let mut chart = chart_with_scratch_taps([TimeUs(0), TimeUs(1_000_000)]);
        let mut autoplay = AutoplayController::default();

        let inputs = autoplay.poll_until(&chart, TimeUs(2_000_000));

        assert_eq!(inputs.len(), 2);
        assert_eq!(inputs[0].scratch_direction, Some(ScratchDirection::Down));
        assert_eq!(inputs[1].scratch_direction, Some(ScratchDirection::Up));

        chart.lane_notes[Lane::Scratch.index()].push(NoteEvent {
            id: NoteId(3),
            lane: Lane::Scratch,
            kind: NoteKind::Tap,
            tick: ChartTick(384),
            time: TimeUs(3_000_000),
            sound: None,
            damage: None,
        });
        let inputs = autoplay.poll_until(&chart, TimeUs(3_000_000));

        assert_eq!(inputs.len(), 1);
        assert_eq!(inputs[0].scratch_direction, Some(ScratchDirection::Down));
    }

    #[test]
    fn autoplay_keeps_non_scratch_press_directionless() {
        let mut chart = chart_with_scratch_taps([]);
        chart.lane_notes[Lane::Key1.index()].push(NoteEvent {
            id: NoteId(1),
            lane: Lane::Key1,
            kind: NoteKind::Tap,
            tick: ChartTick(0),
            time: TimeUs(0),
            sound: None,
            damage: None,
        });
        let mut autoplay = AutoplayController::default();

        let inputs = autoplay.poll_until(&chart, TimeUs(0));

        assert_eq!(inputs.len(), 1);
        assert_eq!(inputs[0].scratch_direction, None);
    }

    fn chart_with_scratch_taps<const N: usize>(times: [TimeUs; N]) -> PlayableChart {
        let mut lane_notes = std::array::from_fn(|_| Vec::new());
        for (index, time) in times.into_iter().enumerate() {
            lane_notes[Lane::Scratch.index()].push(NoteEvent {
                id: NoteId(index as u32 + 1),
                lane: Lane::Scratch,
                kind: NoteKind::Tap,
                tick: ChartTick(index as u64 * 192),
                time,
                sound: None,
                damage: None,
            });
        }

        PlayableChart {
            identity: ChartIdentity { file_md5: [0; 16], file_sha256: [0; 32] },
            metadata: ChartMetadata::default(),
            lane_notes,
            long_notes: Vec::<LongNotePair>::new(),
            bgm_events: Vec::<SoundEvent>::new(),
            bga_events: Vec::<BgaEvent>::new(),
            timing_events: Vec::<TimingEvent>::new(),
            scroll_events: Vec::<ScrollEvent>::new(),
            speed_events: Vec::<SpeedEvent>::new(),
            judge_rank_events: Vec::<JudgeRankEvent>::new(),
            bgm_volume_events: Vec::<ChartVolumeEvent>::new(),
            key_volume_events: Vec::<ChartVolumeEvent>::new(),
            text_events: Vec::<ChartTextEvent>::new(),
            bga_opacity_events: Vec::<BgaOpacityEvent>::new(),
            bga_argb_events: Vec::<BgaArgbEvent>::new(),
            swbga_definitions: Vec::<SwBgaDefinition>::new(),
            bga_keybound_events: Vec::<BgaKeyboundEvent>::new(),
            bga_asset_by_bmp_key: HashMap::new(),
            bar_lines: Vec::<BarLine>::new(),
            sounds: Vec::<SoundAssetRef>::new(),
            bga_assets: Vec::<BgaAssetRef>::new(),
            total_notes: times.len() as u32,
            end_time: times.last().copied().unwrap_or(TimeUs(0)),
        }
    }
}

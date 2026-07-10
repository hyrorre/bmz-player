use bmz_core::lane::Lane;

use crate::model::LongNoteStyle;

use super::error::ImportWarning;
use super::intermediate::{LaneObject, LaneObjectSource, LongNotePairDraft, ResolvedLaneEvent};

pub fn normalize_lane_objects(
    lane: Lane,
    objects: &[LaneObject],
    lnobj_wav_key: Option<u16>,
    warnings: &mut Vec<ImportWarning>,
) -> Vec<ResolvedLaneEvent> {
    let mut out = Vec::new();

    out.extend(resolve_long_channel_lane(lane, objects, warnings));

    let visible: Vec<_> = objects
        .iter()
        .filter(|object| object.source == LaneObjectSource::Visible)
        .cloned()
        .collect();

    if let Some(key) = lnobj_wav_key {
        out.extend(resolve_lnobj_lane(lane, &visible, key, warnings));
    } else {
        out.extend(visible.into_iter().map(|object| ResolvedLaneEvent::Tap {
            lane,
            tick: object.tick,
            time: object.time,
            wav_key: object.wav_key,
        }));
    }

    out.extend(objects.iter().filter(|object| object.source == LaneObjectSource::Invisible).map(
        |object| ResolvedLaneEvent::Invisible {
            lane,
            tick: object.tick,
            time: object.time,
            wav_key: object.wav_key,
        },
    ));

    out.extend(objects.iter().filter_map(|object| match object.source {
        LaneObjectSource::Mine { damage } => Some(ResolvedLaneEvent::Mine {
            lane,
            tick: object.tick,
            time: object.time,
            wav_key: object.wav_key,
            damage,
        }),
        _ => None,
    }));

    // LR2/beatoraja move visible notes covered by a long-channel pair to the
    // BGM lane. At the boundaries the long start/end replaces the visible
    // note as well. This is used both for a keysound layered at the LN start
    // and for older LNTYPE 2 charts that densely place normal notes under LN.
    let long_ranges: Vec<_> = out
        .iter()
        .filter_map(|event| match event {
            ResolvedLaneEvent::Long { pair } => Some((pair.start_tick, pair.end_tick)),
            _ => None,
        })
        .collect();
    let unterminated_long_start_tick = {
        let mut long_objects =
            objects.iter().filter(|object| object.source == LaneObjectSource::LongChannel);
        let mut pending = None;
        for object in &mut long_objects {
            pending = if pending.is_some() { None } else { Some(object.tick) };
        }
        pending
    };
    out.retain(|event| {
        !matches!(event, ResolvedLaneEvent::Tap { tick, .. }
            if long_ranges.iter().any(|(start, end)| start <= tick && tick <= end)
                || unterminated_long_start_tick == Some(*tick))
    });

    out
}

pub fn resolve_long_channel_lane(
    lane: Lane,
    objects: &[LaneObject],
    warnings: &mut Vec<ImportWarning>,
) -> Vec<ResolvedLaneEvent> {
    let mut out = Vec::new();
    let mut pending: Option<&LaneObject> = None;

    for object in objects.iter().filter(|object| object.source == LaneObjectSource::LongChannel) {
        match pending.take() {
            None => pending = Some(object),
            Some(start) => {
                // beatoraja stores same-WAV LN ends as -2, which is silent in its audio driver.
                let end_wav_key =
                    if object.wav_key == start.wav_key { None } else { object.wav_key };
                out.push(ResolvedLaneEvent::Long {
                    pair: LongNotePairDraft {
                        lane,
                        style: LongNoteStyle::ChannelPair,
                        start_tick: start.tick,
                        end_tick: object.tick,
                        start_time: start.time,
                        end_time: object.time,
                        end_wav_key,
                        wav_key: start.wav_key,
                    },
                });
            }
        }
    }

    if pending.is_some() {
        warnings.push(ImportWarning::UnterminatedLongNote { lane });
    }

    out
}

pub fn resolve_lnobj_lane(
    lane: Lane,
    visible: &[LaneObject],
    lnobj_wav_key: u16,
    warnings: &mut Vec<ImportWarning>,
) -> Vec<ResolvedLaneEvent> {
    let mut out = Vec::new();
    let mut pending_start: Option<&LaneObject> = None;

    for object in visible {
        if object.wav_key == Some(lnobj_wav_key) {
            if let Some(start) = pending_start.take() {
                out.push(ResolvedLaneEvent::Long {
                    pair: LongNotePairDraft {
                        lane,
                        style: LongNoteStyle::LnObj,
                        start_tick: start.tick,
                        end_tick: object.tick,
                        start_time: start.time,
                        end_time: object.time,
                        end_wav_key: None,
                        wav_key: start.wav_key,
                    },
                });
            } else {
                warnings.push(ImportWarning::LnobjWithoutStart { lane });
            }
        } else if let Some(previous) = pending_start.replace(object) {
            out.push(ResolvedLaneEvent::Tap {
                lane,
                tick: previous.tick,
                time: previous.time,
                wav_key: previous.wav_key,
            });
        }
    }

    if let Some(last) = pending_start {
        out.push(ResolvedLaneEvent::Tap {
            lane,
            tick: last.tick,
            time: last.time,
            wav_key: last.wav_key,
        });
    }

    out
}

#[cfg(test)]
mod tests {
    use bmz_core::time::{ChartTick, TimeUs};

    use super::*;

    fn long_object(tick: u64, wav_key: u16) -> LaneObject {
        LaneObject {
            lane: Lane::Key1,
            tick: ChartTick(tick),
            time: TimeUs(tick as i64 * 1_000),
            wav_key: Some(wav_key),
            source: LaneObjectSource::LongChannel,
        }
    }

    fn visible_object(tick: u64, wav_key: u16) -> LaneObject {
        LaneObject {
            lane: Lane::Key1,
            tick: ChartTick(tick),
            time: TimeUs(tick as i64 * 1_000),
            wav_key: Some(wav_key),
            source: LaneObjectSource::Visible,
        }
    }

    #[test]
    fn long_channel_pair_keeps_end_wav_key() {
        let mut warnings = Vec::new();
        let events = resolve_long_channel_lane(
            Lane::Key1,
            &[long_object(0, 7), long_object(96, 8)],
            &mut warnings,
        );

        let ResolvedLaneEvent::Long { pair } = &events[0] else {
            panic!("expected long note event");
        };
        assert_eq!(pair.wav_key, Some(7));
        assert_eq!(pair.end_wav_key, Some(8));
        assert!(warnings.is_empty());
    }

    #[test]
    fn long_channel_pair_omits_matching_end_wav_key() {
        let mut warnings = Vec::new();
        let events = resolve_long_channel_lane(
            Lane::Key1,
            &[long_object(0, 7), long_object(96, 7)],
            &mut warnings,
        );

        let ResolvedLaneEvent::Long { pair } = &events[0] else {
            panic!("expected long note event");
        };
        assert_eq!(pair.wav_key, Some(7));
        assert_eq!(pair.end_wav_key, None);
        assert!(warnings.is_empty());
    }

    #[test]
    fn lnobj_pair_does_not_use_marker_wav_as_end_keysound() {
        let mut warnings = Vec::new();
        let events = resolve_lnobj_lane(
            Lane::Key1,
            &[visible_object(0, 7), visible_object(96, 99)],
            99,
            &mut warnings,
        );

        let ResolvedLaneEvent::Long { pair } = &events[0] else {
            panic!("expected long note event");
        };
        assert_eq!(pair.wav_key, Some(7));
        assert_eq!(pair.end_wav_key, None);
        assert!(warnings.is_empty());
    }

    #[test]
    fn visible_note_at_long_channel_start_is_merged_into_long_note() {
        let mut warnings = Vec::new();
        let events = normalize_lane_objects(
            Lane::Key1,
            &[visible_object(0, 7), long_object(0, 7), long_object(96, 8)],
            None,
            &mut warnings,
        );

        assert_eq!(events.len(), 1, "events: {events:?}");
        assert!(matches!(events[0], ResolvedLaneEvent::Long { .. }));
        assert!(warnings.is_empty());
    }

    #[test]
    fn visible_notes_inside_and_at_end_of_long_channel_pair_are_removed() {
        let mut warnings = Vec::new();
        let events = normalize_lane_objects(
            Lane::Key1,
            &[
                long_object(0, 7),
                visible_object(48, 8),
                visible_object(96, 9),
                long_object(96, 10),
                visible_object(144, 11),
            ],
            None,
            &mut warnings,
        );

        assert_eq!(events.len(), 2, "events: {events:?}");
        assert!(matches!(events[0], ResolvedLaneEvent::Long { .. }));
        assert!(matches!(events[1], ResolvedLaneEvent::Tap { tick: ChartTick(144), .. }));
        assert!(warnings.is_empty());
    }

    #[test]
    fn visible_note_at_unterminated_long_start_is_removed() {
        let mut warnings = Vec::new();
        let events = normalize_lane_objects(
            Lane::Key1,
            &[visible_object(0, 7), long_object(0, 7), visible_object(96, 8)],
            None,
            &mut warnings,
        );

        assert_eq!(events.len(), 1, "events: {events:?}");
        assert!(matches!(events[0], ResolvedLaneEvent::Tap { tick: ChartTick(96), .. }));
        assert!(matches!(warnings.as_slice(), [ImportWarning::UnterminatedLongNote { .. }]));
    }
}

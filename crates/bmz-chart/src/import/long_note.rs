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
            Some(start) => out.push(ResolvedLaneEvent::Long {
                pair: LongNotePairDraft {
                    lane,
                    style: LongNoteStyle::ChannelPair,
                    start_tick: start.tick,
                    end_tick: object.tick,
                    start_time: start.time,
                    end_time: object.time,
                    wav_key: start.wav_key,
                },
            }),
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

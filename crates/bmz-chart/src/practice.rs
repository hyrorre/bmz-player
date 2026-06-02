//! Practice-mode chart transforms (section masking).

use bmz_core::time::TimeUs;

use crate::model::{NoteKind, PlayableChart};

/// Margin after the practice section end when computing chart `end_time` (µs).
pub const PRACTICE_END_MARGIN_US: i64 = 500_000;

/// Hides judgeable notes outside `[start_us, end_us)` by converting them to
/// [`NoteKind::Invisible`], then recomputes `total_notes` and `end_time`.
///
/// Long-note pairs are removed as a unit when either endpoint lies outside the
/// section (beatoraja `PracticeModifier` behaviour).
pub fn apply_practice_section(chart: &mut PlayableChart, start_us: TimeUs, end_us: TimeUs) {
    let start = start_us.0;
    let end = end_us.0.max(start);
    let notes_before = chart.total_notes;

    let mut hidden_ids = std::collections::HashSet::new();
    for pair in &chart.long_notes {
        if pair.start_time.0 < start || pair.start_time.0 >= end || pair.end_time.0 >= end {
            hidden_ids.insert(pair.start_note_id);
            hidden_ids.insert(pair.end_note_id);
        }
    }

    for lane_notes in &mut chart.lane_notes {
        for note in lane_notes.iter_mut() {
            if hidden_ids.contains(&note.id) {
                note.kind = NoteKind::Invisible;
                continue;
            }
            if !is_judgeable_kind(note.kind) {
                continue;
            }
            if note.time.0 < start || note.time.0 >= end {
                note.kind = NoteKind::Invisible;
            }
        }
    }

    chart.long_notes.retain(|pair| {
        !hidden_ids.contains(&pair.start_note_id) && !hidden_ids.contains(&pair.end_note_id)
    });

    chart.total_notes = count_judgeable_notes(&chart.lane_notes);
    if let Some(total) = chart.metadata.total
        && notes_before > 0
    {
        chart.metadata.total = Some(total * f64::from(chart.total_notes) / f64::from(notes_before));
    }

    let lane_end = chart
        .lane_notes
        .iter()
        .flat_map(|notes| notes.iter().filter(|n| is_judgeable_kind(n.kind)).map(|n| n.time.0))
        .max()
        .unwrap_or(start);
    let bgm_end = chart.bgm_events.iter().map(|event| event.time.0).max().unwrap_or(0);
    chart.end_time = TimeUs(lane_end.max(bgm_end).max(end) + PRACTICE_END_MARGIN_US);
}

fn is_judgeable_kind(kind: NoteKind) -> bool {
    matches!(kind, NoteKind::Tap | NoteKind::LongStart | NoteKind::LongEnd | NoteKind::Mine)
}

fn count_judgeable_notes(
    lane_notes: &[Vec<crate::model::NoteEvent>; bmz_core::lane::LANE_COUNT],
) -> u32 {
    lane_notes
        .iter()
        .flat_map(|notes| notes.iter())
        .filter(|note| matches!(note.kind, NoteKind::Tap | NoteKind::LongStart))
        .count() as u32
}

#[cfg(test)]
mod tests {
    use bmz_core::chart::ChartIdentity;
    use bmz_core::ids::NoteId;
    use bmz_core::lane::Lane;
    use bmz_core::time::{ChartTick, TimeUs};

    use super::*;
    use crate::model::{ChartMetadata, NoteEvent, NoteKind, PlayableChart};

    fn tap(id: u32, time_ms: i64) -> NoteEvent {
        NoteEvent {
            id: NoteId(id),
            lane: Lane::Key1,
            kind: NoteKind::Tap,
            tick: ChartTick(0),
            time: TimeUs(time_ms * 1000),
            sound: None,
            damage: None,
        }
    }

    #[test]
    fn apply_practice_section_masks_out_of_range_notes() {
        let mut chart = PlayableChart {
            identity: ChartIdentity { file_md5: [0; 16], file_sha256: [0; 32] },
            metadata: ChartMetadata { total: Some(300.0), ..Default::default() },
            lane_notes: {
                let mut lanes = std::array::from_fn(|_| Vec::new());
                lanes[Lane::Key1.index()] = vec![tap(1, 500), tap(2, 1500), tap(3, 2500)];
                lanes
            },
            long_notes: Vec::new(),
            bgm_events: Vec::new(),
            bga_events: Vec::new(),
            timing_events: Vec::new(),
            scroll_events: Vec::new(),
            speed_events: Vec::new(),
            judge_rank_events: Vec::new(),
            bgm_volume_events: Vec::new(),
            key_volume_events: Vec::new(),
            text_events: Vec::new(),
            bga_opacity_events: Vec::new(),
            bga_argb_events: Vec::new(),
            swbga_definitions: Vec::new(),
            bga_keybound_events: Vec::new(),
            bga_asset_by_bmp_key: Default::default(),
            bar_lines: Vec::new(),
            sounds: Vec::new(),
            bga_assets: Vec::new(),
            total_notes: 3,
            end_time: TimeUs(3_000_000),
        };

        apply_practice_section(&mut chart, TimeUs(1_000_000), TimeUs(2_000_000));

        assert_eq!(chart.total_notes, 1);
        assert!(matches!(chart.lane_notes[Lane::Key1.index()][0].kind, NoteKind::Invisible));
        assert!(matches!(chart.lane_notes[Lane::Key1.index()][1].kind, NoteKind::Tap));
        assert!(matches!(chart.lane_notes[Lane::Key1.index()][2].kind, NoteKind::Invisible));
        assert_eq!(chart.metadata.total, Some(100.0));
    }
}

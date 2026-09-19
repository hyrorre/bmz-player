use super::*;

impl ChartAnalysis {
    pub fn from_chart(chart: &PlayableChart) -> Self {
        let canonical_total_notes =
            ChartLnCounts::from_chart(chart).canonical_total_notes(chart.total_notes);
        let seconds = analysis_distribution_seconds(chart);
        let mut distribution = vec![ChartDistributionSecond::default(); seconds.max(1)];
        let mut lane_notes = Lane::ALL
            .iter()
            .map(|lane| ChartLaneNotes { lane_index: lane.index() as u8, ..Default::default() })
            .collect::<Vec<_>>();

        for pair in &chart.long_notes {
            let start_sec = second_index(pair.start_time.0);
            let end_sec = second_index(pair.end_time.0).min(distribution.len().saturating_sub(1));
            for second in distribution.iter_mut().take(end_sec + 1).skip(start_sec) {
                add_long_body(second, pair.lane, 1);
            }
        }

        let long_end_modes = chart
            .long_notes
            .iter()
            .map(|pair| (pair.end_note_id, pair.mode.unwrap_or(chart.metadata.long_note_mode)))
            .collect::<std::collections::HashMap<_, _>>();
        let mut bpm_note_counts: Vec<(f64, u32)> = Vec::new();
        let mut total_countdown = canonical_total_notes as i64
            - gauge_border_note_count(chart.metadata.total, canonical_total_notes);
        let mut border_sec = 0usize;

        for notes in &chart.lane_notes {
            for note in notes {
                let sec = second_index(note.time.0);
                let Some(slot) = distribution.get_mut(sec) else {
                    continue;
                };
                let lane = note.lane;
                let lane_slot = &mut lane_notes[lane.index()];
                match note.kind {
                    NoteKind::Tap => {
                        if is_scratch_lane(lane) {
                            slot.scratch_taps = slot.scratch_taps.saturating_add(1);
                            lane_slot.normal_notes = lane_slot.normal_notes.saturating_add(1);
                        } else {
                            slot.key_taps = slot.key_taps.saturating_add(1);
                            lane_slot.normal_notes = lane_slot.normal_notes.saturating_add(1);
                        }
                        total_countdown -= 1;
                        add_bpm_note_count(&mut bpm_note_counts, bpm_at(chart, note.time.0), 1);
                    }
                    NoteKind::LongStart => {
                        add_long_head(slot, lane);
                        add_long_body(slot, lane, -1);
                        lane_slot.long_notes = lane_slot.long_notes.saturating_add(1);
                        total_countdown -= 1;
                        add_bpm_note_count(&mut bpm_note_counts, bpm_at(chart, note.time.0), 1);
                    }
                    NoteKind::LongEnd
                        if long_end_modes.get(&note.id).is_some_and(|mode| {
                            matches!(mode, LongNoteMode::Cn | LongNoteMode::Hcn)
                        }) =>
                    {
                        add_long_head(slot, lane);
                        lane_slot.long_notes = lane_slot.long_notes.saturating_add(1);
                        total_countdown -= 1;
                        add_bpm_note_count(&mut bpm_note_counts, bpm_at(chart, note.time.0), 1);
                    }
                    NoteKind::LongEnd => {}
                    NoteKind::Invisible => {}
                    NoteKind::Mine => {
                        slot.mines = slot.mines.saturating_add(1);
                        lane_slot.mines = lane_slot.mines.saturating_add(1);
                    }
                }
                if total_countdown == 0 {
                    border_sec = sec;
                }
            }
        }

        let peak_density =
            distribution.iter().map(|second| second.playable_notes()).max().unwrap_or(0) as f64;
        let threshold = canonical_total_notes as usize / distribution.len().max(1) / 4;
        let mut density_sum = 0u32;
        let mut density_count = 0u32;
        for notes in distribution.iter().map(|second| second.playable_notes()) {
            if notes as usize >= threshold {
                density_sum = density_sum.saturating_add(notes);
                density_count = density_count.saturating_add(1);
            }
        }
        let density: f64 = if density_count == 0 {
            0.0_f64
        } else {
            f64::from(density_sum) / f64::from(density_count)
        };

        let end_window = 5usize.min(distribution.len().saturating_sub(border_sec + 1));
        let mut end_density: f64 = 0.0;
        if end_window > 0 {
            for start in border_sec..distribution.len().saturating_sub(end_window) {
                let notes = (0..end_window)
                    .map(|offset| distribution[start + offset].playable_notes())
                    .sum::<u32>();
                end_density = end_density.max(f64::from(notes) / end_window as f64);
            }
        }

        let main_bpm: f64 = bpm_note_counts
            .into_iter()
            .max_by_key(|(_, count)| *count)
            .map(|(bpm, _)| bpm)
            .unwrap_or(chart.metadata.initial_bpm);

        let normal_notes = lane_notes.iter().map(|lane| lane.normal_notes).sum();
        let long_notes = lane_notes.iter().map(|lane| lane.long_notes).sum();
        let scratch_notes = lane_notes
            .iter()
            .filter(|lane| {
                lane.lane_index == Lane::Scratch.index() as u8
                    || lane.lane_index == Lane::Scratch2.index() as u8
            })
            .map(|lane| lane.normal_notes)
            .sum();
        let long_scratch_notes = lane_notes
            .iter()
            .filter(|lane| {
                lane.lane_index == Lane::Scratch.index() as u8
                    || lane.lane_index == Lane::Scratch2.index() as u8
            })
            .map(|lane| lane.long_notes)
            .sum();

        trim_trailing_empty_distribution(&mut distribution);

        Self {
            normal_notes,
            long_notes,
            scratch_notes,
            long_scratch_notes,
            density,
            peak_density,
            end_density,
            total_gauge: gauge_total_for_chart(chart.metadata.total, canonical_total_notes),
            main_bpm,
            distribution,
            speed_changes: chart_speed_changes(chart),
            lane_notes,
        }
    }
}

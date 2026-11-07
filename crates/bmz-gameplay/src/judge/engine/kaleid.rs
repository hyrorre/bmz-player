use super::*;

impl JudgeEngine {
    pub(super) fn mark_free_zone_input(&mut self, chart: &PlayableChart, input: InputEvent) {
        if input.kind == InputKind::Press {
            for zone in &chart.metadata.free_zones {
                if zone.lane == input.lane
                    && input.time >= zone.start_time
                    && input.time <= zone.end_time
                {
                    self.free_zone_scratched.insert(zone.note_id);
                }
            }
        }
    }

    pub(super) fn filter_free_zone_outcome(
        &mut self,
        chart: &PlayableChart,
        outcome: &mut JudgeOutcome,
    ) {
        outcome.events.retain_mut(|event| {
            let inside = chart.metadata.free_zones.iter().any(|zone| {
                zone.lane == event.lane
                    && event.time >= zone.start_time
                    && event.time <= zone.end_time
            });
            let inner = event.note_id.is_some_and(|id| chart.is_free_zone_inner(id));
            if event.judge == Judge::EmptyPoor && (inside || inner) {
                return false;
            }
            if inner {
                event.affects_score = false;
                if matches!(event.judge, Judge::PGreat | Judge::Great | Judge::Good | Judge::Bad) {
                    for zone in &chart.metadata.free_zones {
                        if event.note_id.is_some_and(|id| zone.inner_notes.contains(&id)) {
                            self.free_zone_scratched.insert(zone.note_id);
                        }
                    }
                }
                // Judge state is retained for the final zone grade, but the inner
                // note must not reach score/gauge, bombs, ghost or result graphs.
                return false;
            }
            true
        });
        outcome.keysounds.retain(|sound| {
            if sound.trigger == KeySoundTrigger::Miss && chart.is_free_zone_inner(sound.note_id) {
                return false;
            }
            sound.trigger != KeySoundTrigger::Fallback
                || chart.note_by_id(sound.note_id).is_none_or(|note| {
                    !chart.metadata.free_zones.iter().any(|zone| {
                        zone.lane == note.lane
                            && sound.time >= zone.start_time
                            && sound.time <= zone.end_time
                    })
                })
        });
    }

    pub(super) fn settle_free_zones(
        &mut self,
        chart: &PlayableChart,
        now: TimeUs,
        outcome: &mut JudgeOutcome,
    ) {
        for zone in &chart.metadata.free_zones {
            if now < zone.end_time || self.judged_notes.contains_key(&zone.note_id) {
                continue;
            }
            let mut grades = Vec::new();
            let mut pending = false;
            for &id in &zone.inner_notes {
                let Some(note) = chart.note_by_id(id) else { continue };
                if note.kind == NoteKind::LongEnd {
                    let pair = chart.long_notes.iter().find(|pair| pair.end_note_id == id);
                    if pair.is_some_and(|pair| {
                        matches!(
                            pair.mode.unwrap_or(chart.metadata.long_note_mode),
                            LongNoteMode::Ln | LongNoteMode::Hln
                        )
                    }) {
                        continue;
                    }
                }
                if self.lanes[zone.lane.index()]
                    .active_long
                    .is_some_and(|ln| ln.start_note_id == id)
                    || self.hln.pending_note(chart, id)
                {
                    pending = true;
                }
                match self.judged_notes.get(&id) {
                    Some(judge) => grades.push(*judge),
                    None => pending = true,
                }
            }
            if pending {
                continue;
            }
            let auto = self.free_zone_autoplay[zone.lane.index()];
            let scratched = auto || self.free_zone_scratched.contains(&zone.note_id);
            let judge = if (grades.is_empty() && auto)
                || (!grades.is_empty() && grades.iter().all(|j| *j == Judge::PGreat))
            {
                Judge::PGreat
            } else if !grades.is_empty()
                && grades.iter().all(|j| matches!(j, Judge::PGreat | Judge::Great))
            {
                Judge::Great
            } else if scratched {
                Judge::Good
            } else {
                Judge::Poor
            };
            self.judged_notes.insert(zone.note_id, judge);
            outcome.events.push(JudgementEvent {
                note_id: Some(zone.note_id),
                lane: zone.lane,
                judge,
                side: TimingSide::Slow,
                delta: TimeUs(0),
                time: now,
                affects_score: true,
            });
        }
    }
}

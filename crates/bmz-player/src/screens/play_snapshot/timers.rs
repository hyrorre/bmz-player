use super::*;

/// Detached event origins. Elapsed values are evaluated by the consumer, never advanced in place.
#[derive(Clone, Copy)]
pub(super) struct PlaySkinTimers {
    keyon: [Option<TimeUs>; LANE_COUNT],
    keyoff: [Option<TimeUs>; LANE_COUNT],
    hold: [Option<TimeUs>; LANE_COUNT],
    hcn_active: [Option<TimeUs>; LANE_COUNT],
    hcn_damage: [Option<TimeUs>; LANE_COUNT],
    full_combo: Option<TimeUs>,
    gauge_increase: Option<TimeUs>,
    gauge_max: Option<TimeUs>,
    opponent: Option<OpponentSkinTimers>,
}

#[derive(Clone, Copy)]
struct OpponentSkinTimers {
    full_combo: Option<TimeUs>,
    allow_future_full_combo: bool,
    gauge_increase: Option<TimeUs>,
    gauge_max: Option<TimeUs>,
}

impl PlaySkinTimers {
    pub(super) fn from_session(session: &GameSession) -> Self {
        Self {
            keyon: session.lane_keyon_started_at,
            keyoff: session.lane_keyoff_started_at,
            hold: session.judge.lanes.map(|lane| lane.active_long.map(|active| active.started_at)),
            hcn_active: session
                .lane_hcn_timer
                .map(|timer| timer.filter(|timer| timer.inclease).map(|timer| timer.since)),
            hcn_damage: session
                .lane_hcn_timer
                .map(|timer| timer.filter(|timer| !timer.inclease).map(|timer| timer.since)),
            full_combo: session.full_combo_started_at,
            gauge_increase: session.gauge_increase_started_at,
            gauge_max: session.gauge_max_started_at,
            opponent: session.opponent.display_state(session.scored_total_notes).map(|opponent| {
                OpponentSkinTimers {
                    full_combo: session
                        .opponent
                        .independent
                        .as_ref()
                        .map_or(session.opponent.full_combo_started_at, |opponent| {
                            opponent.full_combo_started_at
                        }),
                    allow_future_full_combo: session.opponent.independent.is_some(),
                    gauge_increase: opponent.gauge_increase_started_at,
                    gauge_max: opponent.gauge_max_started_at,
                }
            }),
        }
    }

    pub(super) fn apply(&self, snapshot: &mut RenderSnapshot) {
        let now = snapshot.time;
        snapshot.keyon_started_at = self.keyon;
        snapshot.keyoff_started_at = self.keyoff;
        snapshot.keyon_ms =
            self.keyon.map(|start| lane_key_timer_ms(start, now, snapshot.play_elapsed_time));
        snapshot.keyoff_ms =
            self.keyoff.map(|start| lane_key_timer_ms(start, now, snapshot.play_elapsed_time));
        snapshot.hold_ms = self.hold.map(|start| signed_elapsed_ms(now, start));
        snapshot.hcn_active_ms = self.hcn_active.map(|start| signed_elapsed_ms(now, start));
        snapshot.hcn_damage_ms = self.hcn_damage.map(|start| signed_elapsed_ms(now, start));
        snapshot.full_combo_elapsed_ms = full_combo_elapsed_ms(now, self.full_combo, false);
        snapshot.gauge_increase_elapsed_ms =
            optional_skin_timer_elapsed_ms(now, self.gauge_increase);
        snapshot.gauge_max_elapsed_ms = optional_skin_timer_elapsed_ms(now, self.gauge_max);
        if let Some((snapshot, timers)) = snapshot.opponent.as_mut().zip(self.opponent) {
            snapshot.full_combo_elapsed_ms =
                full_combo_elapsed_ms(now, timers.full_combo, timers.allow_future_full_combo);
            snapshot.gauge_increase_elapsed_ms =
                optional_skin_timer_elapsed_ms(now, timers.gauge_increase);
            snapshot.gauge_max_elapsed_ms = optional_skin_timer_elapsed_ms(now, timers.gauge_max);
        }
    }
}

fn signed_elapsed_ms(now: TimeUs, start: Option<TimeUs>) -> Option<i32> {
    start.map(|start| ((now.0 - start.0) / 1_000).clamp(i32::MIN as i64, i32::MAX as i64) as i32)
}

fn full_combo_elapsed_ms(now: TimeUs, start: Option<TimeUs>, allow_future: bool) -> Option<i32> {
    start
        .filter(|start| allow_future || now >= *start)
        .map(|start| skin_timer_elapsed_ms(now, start))
}

pub(super) fn refresh_chart_skin_timers(
    snapshot: &mut RenderSnapshot,
    timing_map: &TimingMap,
    bar_lines: &[BarLine],
    end_of_note_time: TimeUs,
) {
    snapshot.rhythm_timer_elapsed_ms =
        rhythm_timer_elapsed_ms(timing_map, bar_lines, snapshot.time);
    snapshot.quarter_note_elapsed_ms =
        quarter_note_elapsed_ms(timing_map, bar_lines, snapshot.time);
    snapshot.end_of_note_elapsed_ms = end_of_note_elapsed_ms(snapshot.time, end_of_note_time);
    if let Some(opponent) = &mut snapshot.opponent {
        opponent.end_of_note_elapsed_ms = snapshot.end_of_note_elapsed_ms;
    }
}

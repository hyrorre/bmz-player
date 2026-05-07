use bmz_core::judge::{Judge, TimingSide};
use bmz_core::lane::Lane;
use bmz_core::time::TimeUs;
use bmz_gameplay::judge::model::JudgementEvent;
use bmz_gameplay::session::GameSession;
use bmz_render::snapshot::{DisplayJudgement, RenderSnapshot, VisibleBarLine, VisibleNote};

pub const DEFAULT_LOOKAHEAD_US: i64 = 2_000_000;
pub const DEFAULT_LANE_HEIGHT: f32 = 720.0;

pub fn build_render_snapshot(
    session: &GameSession,
    render_now: TimeUs,
    recent_judgements: &[JudgementEvent],
) -> RenderSnapshot {
    let mut snapshot = RenderSnapshot {
        time: render_now,
        combo: session.score.combo,
        max_combo: session.score.max_combo,
        ex_score: session.score.ex_score(),
        total_notes: session.chart.total_notes,
        past_notes: session.score.past_notes,
        gauge: session.gauge.current().value,
        visible_notes: std::array::from_fn(|_| Vec::new()),
        recent_judgements: recent_judgements.iter().map(display_judgement).collect(),
        bar_lines: Vec::new(),
    };

    for lane in Lane::ALL {
        let next_note_index = session.judge.lanes[lane.index()].next_note_index;
        for note in session.chart.notes_for_lane(lane).iter().skip(next_note_index) {
            if let Some(y) = note_y(note.time, render_now) {
                snapshot.visible_notes[lane.index()].push(VisibleNote { lane, time: note.time, y });
            }
        }
    }

    for bar in &session.chart.bar_lines {
        if let Some(y) = note_y(bar.time, render_now) {
            snapshot.bar_lines.push(VisibleBarLine { time: bar.time, y });
        }
    }

    snapshot
}

fn note_y(note_time: TimeUs, render_now: TimeUs) -> Option<f32> {
    let delta = note_time.0 - render_now.0;
    if !(0..=DEFAULT_LOOKAHEAD_US).contains(&delta) {
        return None;
    }

    let progress = delta as f32 / DEFAULT_LOOKAHEAD_US as f32;
    Some(progress * DEFAULT_LANE_HEIGHT)
}

fn display_judgement(event: &JudgementEvent) -> DisplayJudgement {
    DisplayJudgement {
        text: format!("{}{}", judge_text(event.judge), side_suffix(event.side)),
        delta_us: event.delta.0,
        time: event.time,
    }
}

fn judge_text(judge: Judge) -> &'static str {
    match judge {
        Judge::PGreat => "PGREAT",
        Judge::Great => "GREAT",
        Judge::Good => "GOOD",
        Judge::Bad => "BAD",
        Judge::Poor => "POOR",
        Judge::EmptyPoor => "EMPTY POOR",
    }
}

fn side_suffix(side: TimingSide) -> &'static str {
    match side {
        TimingSide::Fast => " FAST",
        TimingSide::Slow => " SLOW",
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use bmz_chart::hash::compute_chart_identity;
    use bmz_chart::model::{ChartMetadata, NoteEvent, NoteKind, PlayableChart};
    use bmz_core::ids::NoteId;
    use bmz_core::judge::{Judge, TimingSide};
    use bmz_core::lane::Lane;
    use bmz_core::time::{ChartTick, TimeUs};
    use bmz_gameplay::judge::model::JudgementEvent;

    use crate::config::profile_config::ProfileConfig;
    use crate::screens::play_session::{PlaySessionOptions, build_game_session};

    use super::*;

    #[test]
    fn build_render_snapshot_filters_visible_notes_and_formats_judgements() {
        let profile = ProfileConfig::new_default("default", "Default", 1);
        let session =
            build_game_session(Arc::new(chart()), &profile, PlaySessionOptions::default());
        let judgements = vec![JudgementEvent {
            note_id: Some(NoteId(1)),
            lane: Lane::Key1,
            judge: Judge::EmptyPoor,
            side: TimingSide::Slow,
            delta: TimeUs(5_000),
            time: TimeUs(1_000),
        }];

        let snapshot = build_render_snapshot(&session, TimeUs(0), &judgements);

        assert_eq!(snapshot.combo, 0);
        assert_eq!(snapshot.max_combo, 0);
        assert_eq!(snapshot.ex_score, 0);
        assert_eq!(snapshot.total_notes, 1);
        assert_eq!(snapshot.past_notes, 0);
        assert_eq!(snapshot.visible_notes[Lane::Key1.index()].len(), 1);
        assert_eq!(snapshot.visible_notes[Lane::Key1.index()][0].y, 360.0);
        assert_eq!(snapshot.recent_judgements[0].text, "EMPTY POOR SLOW");
        assert_eq!(snapshot.recent_judgements[0].delta_us, 5_000);
    }

    #[test]
    fn build_render_snapshot_hides_consumed_notes() {
        let profile = ProfileConfig::new_default("default", "Default", 1);
        let mut session =
            build_game_session(Arc::new(chart()), &profile, PlaySessionOptions::default());
        session.judge.lanes[Lane::Key1.index()].next_note_index = 1;

        let snapshot = build_render_snapshot(&session, TimeUs(0), &[]);

        assert!(snapshot.visible_notes[Lane::Key1.index()].is_empty());
    }

    fn chart() -> PlayableChart {
        let note = NoteEvent {
            id: NoteId(1),
            lane: Lane::Key1,
            kind: NoteKind::Tap,
            tick: ChartTick(0),
            time: TimeUs(1_000_000),
            sound: None,
        };
        let mut lane_notes = std::array::from_fn(|_| Vec::new());
        lane_notes[Lane::Key1.index()].push(note);

        PlayableChart {
            identity: compute_chart_identity(b"snapshot"),
            metadata: ChartMetadata {
                title: "snapshot".to_string(),
                initial_bpm: 120.0,
                total: Some(160.0),
                ..Default::default()
            },
            lane_notes,
            long_notes: Vec::new(),
            bgm_events: Vec::new(),
            timing_events: Vec::new(),
            bar_lines: Vec::new(),
            sounds: Vec::new(),
            total_notes: 1,
            end_time: TimeUs(1_000_000),
        }
    }
}

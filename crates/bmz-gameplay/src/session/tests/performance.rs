use super::*;
use std::{hint::black_box, time::Instant};

fn measure(label: &str, mut run: impl FnMut()) {
    let mut samples = Vec::new();
    for i in 0..1010 {
        let start = Instant::now();
        run();
        if i >= 10 {
            samples.push(start.elapsed().as_secs_f64() * 1e6);
        }
    }
    samples.sort_by(f64::total_cmp);
    eprintln!(
        "{label} median_us={:.3} p95_us={:.3} max_us={:.3}",
        samples[500], samples[950], samples[999]
    );
}

#[test]
#[ignore = "release CPU probe; run alone with --ignored --nocapture"]
fn profile_gameplay_searches() {
    assert!(!black_box(cfg!(debug_assertions)), "run with --release");
    for count in [10_000u32, 50_000] {
        let mut chart = chart_with_keysound();
        let template = chart.lane_notes[Lane::Key1.index()][0].clone();
        chart.lane_notes[Lane::Key1.index()] = (0..count)
            .map(|i| NoteEvent {
                id: NoteId(i + 1),
                time: TimeUs(i64::from(i) * 100_000),
                tick: ChartTick(u64::from(i) * 96),
                ..template.clone()
            })
            .collect();
        chart.total_notes = count;
        chart.end_time = TimeUs(i64::from(count) * 100_000);
        let mut session = session_with_autoplay(chart);
        let now = TimeUs(i64::from(count - 10) * 100_000);
        measure(&format!("press candidates={count}"), || {
            black_box(session.judge.process_input(&session.chart, human_press(now)));
        });
        measure(&format!("note_id notes={count}"), || {
            black_box(session.judge.note_by_id(&session.chart, black_box(NoteId(count - 10))));
        });
        for mode in [LongNoteMode::Ln, LongNoteMode::Hcn] {
            let mut chart = chart_with_hcn_long_note();
            let pair = chart.long_notes[0].clone();
            let head = chart.lane_notes[Lane::Key1.index()][0].clone();
            let tail = chart.lane_notes[Lane::Key1.index()][1].clone();
            chart.long_notes.clear();
            chart.lane_notes[Lane::Key1.index()].clear();
            for i in 0..count {
                let start = TimeUs(i64::from(i) * 200_000);
                let end = TimeUs(start.0 + 100_000);
                let start_id = NoteId(i * 2 + 1);
                let end_id = NoteId(i * 2 + 2);
                chart.long_notes.push(bmz_chart::model::LongNotePair {
                    start_note_id: start_id,
                    end_note_id: end_id,
                    mode: Some(mode),
                    start_time: start,
                    end_time: end,
                    ..pair.clone()
                });
                chart.lane_notes[Lane::Key1.index()].extend([
                    NoteEvent { id: start_id, time: start, ..head.clone() },
                    NoteEvent { id: end_id, time: end, ..tail.clone() },
                ]);
            }
            let mut session = session_with_autoplay(chart);
            let now = TimeUs(i64::from(count - 10) * 200_000 + 50_000);
            measure(&format!("hcn pairs={count} mode={mode:?}"), || {
                update_hcn_lane_timers(black_box(&mut session), now);
            });
        }
    }
}

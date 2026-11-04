use bmz_core::clear::{ClearType, GaugeType};
use bmz_core::judge::{Judge, TimingSide};
use bmz_core::lane::Lane;
use bmz_core::time::{ChartTick, TimeUs};
use bmz_gameplay::gauge::GaugeState;
use bmz_gameplay::judge::model::JudgementEvent;
use bmz_gameplay::score::ScoreState;
use bmz_gameplay::session::PlayState;
use bmz_render::chart_graph::BpmGraphSegment;
use bmz_render::snapshot::HitErrorRingSnapshot;

use bmz_chart::model::{NoteEvent, NoteKind};

use super::*;

#[test]
fn result_summary_uses_play_and_storage_values() {
    let result = play_result();
    let stored = stored_result();
    let chart = chart();

    let summary = ResultSummary::from_play_result(&result, &stored, &chart);

    assert_eq!(summary.arrange, "NORMAL");
    assert_eq!(summary.arrange_2p, "NORMAL");
    assert_eq!(summary.title, "Test");
    assert_eq!(summary.duration_ms, 90_000);
    assert_eq!(summary.initial_bpm, 128.0);
    assert_eq!(summary.clear_type, ClearType::Normal);
    assert_eq!(summary.gauge_type, GaugeType::Normal);
    assert_eq!(summary.max_combo, 12);
    assert_eq!(summary.bp, 18);
    assert_eq!(summary.cb, 11);
    assert_eq!(summary.gauge_value, 82.0);
    assert!(!summary.has_long_notes);
    assert_eq!(summary.long_note_mode, LongNoteMode::Ln);
    assert_eq!(summary.score_history_id, 9);
    assert_eq!(summary.replay_path, "replay/test.toml");
    assert_eq!(
        summary.judge_counts,
        ResultJudgeCounts { pgreat: 2, great: 3, good: 4, bad: 5, poor: 6, empty_poor: 7 }
    );
}

#[test]
fn failed_result_summary_uses_record_bp_and_cb() {
    let mut result = play_result();
    result.clear_type = ClearType::Failed;
    result.score = bmz_gameplay::score::ScoreState::default();
    let stored = stored_result();
    let chart = chart();

    let summary = ResultSummary::from_play_result(&result, &stored, &chart);

    assert_eq!(summary.bp, chart.total_notes);
    assert_eq!(summary.cb, chart.total_notes);
}

#[test]
fn result_summary_keeps_effective_long_note_state() {
    let result = play_result();
    let stored = stored_result();
    let mut chart = chart();
    chart.metadata.long_note_mode = LongNoteMode::Hcn;
    chart.long_notes.push(bmz_chart::model::LongNotePair {
        lane: Lane::Key1,
        style: bmz_chart::model::LongNoteStyle::ChannelPair,
        mode: Some(LongNoteMode::Hcn),
        start_note_id: bmz_core::ids::NoteId(1),
        end_note_id: bmz_core::ids::NoteId(2),
        start_tick: bmz_core::time::ChartTick(0),
        end_tick: bmz_core::time::ChartTick(1),
        start_time: TimeUs(0),
        end_time: TimeUs(1_000_000),
        sound: None,
    });

    let summary = ResultSummary::from_play_result(&result, &stored, &chart);

    assert!(summary.has_long_notes);
    assert_eq!(summary.long_note_mode, LongNoteMode::Hcn);
}

fn play_result() -> PlayResult {
    let score = ScoreState {
        max_combo: 12,
        judges: bmz_gameplay::score::JudgeCounts {
            fast_pgreat: 2,
            slow_great: 3,
            fast_good: 4,
            slow_bad: 5,
            fast_poor: 6,
            slow_empty_poor: 7,
            ..Default::default()
        },
        ..Default::default()
    };
    PlayResult {
        chart_sha256: [1; 32],
        clear_type: ClearType::Normal,
        gauge_type: GaugeType::Normal,
        gauge_value: 82.0,
        total_notes: 20,
        score,
        autoplay: false,
    }
}

fn stored_result() -> StoredPlayResult {
    StoredPlayResult {
        score_history_id: 9,
        played_at: 0,
        replay_path: "replay/test.toml".to_string(),
        replay_sha256: None,
        slot_paths: [None, None, None, None],
        device_type: bmz_core::input::InputDeviceKind::Keyboard,
    }
}

fn chart() -> bmz_chart::model::PlayableChart {
    bmz_chart::model::PlayableChart {
        identity: bmz_core::chart::ChartIdentity { file_md5: [0; 16], file_sha256: [0; 32] },
        metadata: bmz_chart::model::ChartMetadata {
            title: "Test".to_string(),
            initial_bpm: 128.0,
            ..bmz_chart::model::ChartMetadata::default()
        },
        lane_notes: std::array::from_fn(|_| Vec::new()),
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
        bga_asset_by_bmp_key: std::collections::HashMap::new(),
        bar_lines: Vec::new(),
        sounds: Vec::new(),
        bga_assets: Vec::new(),
        total_notes: 20,
        end_time: TimeUs(90_000_000),
    }
}

#[test]
fn result_graph_collector_carries_frame_graph_data() {
    let hit_error_ring = HitErrorRingSnapshot { index: 7, ..HitErrorRingSnapshot::default() };
    let mut render_snapshot = RenderSnapshot {
        time: TimeUs(1_234_000),
        play_elapsed_time: TimeUs(0),
        gauge: 72.5,
        gauge_type: GaugeType::Hard as i32,
        gauge_border: 30.0,
        judge_graph_density: vec![1, 3, 2].into(),
        bpm_graph_segments: vec![BpmGraphSegment {
            start_ratio: 0.0,
            end_ratio: 1.0,
            bpm: 180.0,
            is_stop: false,
        }]
        .into(),
        hit_error_ring,
        ..RenderSnapshot::default()
    };
    let frame = FrameOutput {
        render_snapshot: render_snapshot.clone(),
        judgements: vec![JudgementEvent {
            note_id: None,
            lane: Lane::Key1,
            judge: Judge::Great,
            side: TimingSide::Fast,
            delta: TimeUs(-12_000),
            time: TimeUs(1_200_000),
            affects_score: true,
        }],
        mine_hits: Vec::new(),
        keysound_volumes: Vec::new(),
        skin_events: Vec::new(),
        state: PlayState::Playing,
    };
    let mut collector = ResultGraphCollector::default();
    collector.record_frame(&frame);

    render_snapshot.time = TimeUs(1_234_500);
    render_snapshot.play_elapsed_time = TimeUs(500_000);
    render_snapshot.gauge = 74.0;
    collector.record_frame(&FrameOutput {
        render_snapshot,
        judgements: Vec::new(),
        mine_hits: Vec::new(),
        keysound_volumes: Vec::new(),
        skin_events: Vec::new(),
        state: PlayState::Playing,
    });

    let graph = collector.snapshot();
    assert_eq!(
        graph.gauge_points.iter().map(|point| (point.time_ms, point.value)).collect::<Vec<_>>(),
        vec![(0, 72.5), (500, 74.0)]
    );
    assert_eq!(graph.timing_points.len(), 1);
    assert_eq!(graph.timing_points[0].delta_us, 12_000);
    assert_eq!(graph.timing_distribution.counts[(150 + 12) as usize], 1);
    assert_eq!(graph.judge_graph_density, vec![1, 3, 2]);
    assert_eq!(graph.bpm_graph_segments.len(), 1);
    assert_eq!(graph.hit_error_ring.index, 7);
}

#[test]
fn result_graph_collector_samples_gauge_every_500ms_without_compression() {
    fn record(collector: &mut ResultGraphCollector, time_us: i64, gauge: f32) {
        collector.record_frame(&FrameOutput {
            render_snapshot: RenderSnapshot {
                time: TimeUs(time_us),
                play_elapsed_time: TimeUs(time_us),
                gauge,
                gauge_type: GaugeType::Normal as i32,
                gauge_border: 80.0,
                ..RenderSnapshot::default()
            },
            judgements: Vec::new(),
            mine_hits: Vec::new(),
            keysound_volumes: Vec::new(),
            skin_events: Vec::new(),
            state: PlayState::Playing,
        });
    }

    let mut collector = ResultGraphCollector::default();
    record(&mut collector, 0, 20.0);
    record(&mut collector, 500_000, 20.0);
    record(&mut collector, 1_000_000, 35.0);
    record(&mut collector, 1_500_000, 35.0);
    record(&mut collector, 2_000_000, 35.0);
    record(&mut collector, 2_500_000, 42.0);

    let graph = collector.snapshot();
    assert_eq!(
        graph.gauge_points.iter().map(|point| (point.time_ms, point.value)).collect::<Vec<_>>(),
        vec![(0, 20.0), (500, 20.0), (1000, 35.0), (1500, 35.0), (2000, 35.0), (2500, 42.0),]
    );
}

#[test]
fn result_graph_collector_records_each_gauge_type() {
    fn record(collector: &mut ResultGraphCollector, time_us: i64, normal: f32, easy: f32) {
        collector.record_frame(&FrameOutput {
            render_snapshot: RenderSnapshot {
                time: TimeUs(time_us),
                play_elapsed_time: TimeUs(time_us),
                gauge_graph_points: vec![
                    ResultGaugeGraphPoint {
                        time_ms: 0,
                        value: normal,
                        max: 100.0,
                        border: 80.0,
                        gauge_type: GaugeType::Normal as i32,
                        course_section_start: false,
                    },
                    ResultGaugeGraphPoint {
                        time_ms: 0,
                        value: easy,
                        max: 100.0,
                        border: 80.0,
                        gauge_type: GaugeType::Easy as i32,
                        course_section_start: false,
                    },
                ],
                ..RenderSnapshot::default()
            },
            judgements: Vec::new(),
            mine_hits: Vec::new(),
            keysound_volumes: Vec::new(),
            skin_events: Vec::new(),
            state: PlayState::Playing,
        });
    }

    let mut collector = ResultGraphCollector::default();
    record(&mut collector, 0, 20.0, 20.0);
    record(&mut collector, 500_000, 70.0, 90.0);

    let graph = collector.snapshot();
    let normal = graph
        .gauge_points
        .iter()
        .filter(|point| point.gauge_type == GaugeType::Normal as i32)
        .map(|point| (point.time_ms, point.value))
        .collect::<Vec<_>>();
    let easy = graph
        .gauge_points
        .iter()
        .filter(|point| point.gauge_type == GaugeType::Easy as i32)
        .map(|point| (point.time_ms, point.value))
        .collect::<Vec<_>>();

    assert_eq!(normal, vec![(0, 20.0), (500, 70.0)]);
    assert_eq!(easy, vec![(0, 20.0), (500, 90.0)]);
}

#[test]
fn result_graph_failed_tail_appends_zero_samples_until_chart_end() {
    let mut graph = ResultGraphSnapshot::default();
    let gauge = GaugeState::new(GaugeType::Hard, 160.0, 1000);

    fill_failed_gauge_tail(&mut graph, &gauge, 500, 1500);

    let hard = graph
        .gauge_points
        .iter()
        .filter(|point| point.gauge_type == GaugeType::Hard as i32)
        .map(|point| (point.time_ms, point.value))
        .collect::<Vec<_>>();
    let normal = graph
        .gauge_points
        .iter()
        .filter(|point| point.gauge_type == GaugeType::Normal as i32)
        .map(|point| (point.time_ms, point.value))
        .collect::<Vec<_>>();

    assert_eq!(hard, vec![(500, 0.0), (1000, 0.0)]);
    assert_eq!(normal, vec![(500, 0.0), (1000, 0.0)]);
}

fn detail(judge: Judge, delta_us: i64) -> bmz_gameplay::session::ResultJudgementDetail {
    bmz_gameplay::session::ResultJudgementDetail {
        judge,
        side: TimingSide::Slow,
        delta: TimeUs(delta_us),
        time: TimeUs(0),
    }
}

fn judged(id: u32) -> JudgementEvent {
    JudgementEvent {
        note_id: Some(bmz_core::ids::NoteId(id)),
        lane: Lane::Key1,
        judge: Judge::Great,
        side: TimingSide::Slow,
        delta: TimeUs(0),
        time: TimeUs(0),
        affects_score: true,
    }
}

#[test]
fn play_judge_graph_tracker_publishes_at_most_every_750_ms_like_the_result_buckets() {
    let mut chart = chart();
    chart.end_time = TimeUs(2_000_000);
    chart.lane_notes[Lane::Key1.index()] = vec![note(1, 0), note(2, 1_000_000), note(3, 1_000_000)];
    let cache = crate::screens::play_snapshot::PlayRenderSnapshotCache::from_chart(&chart);
    let mut graphs = cache.play_note_graphs.clone();
    assert_eq!(graphs.judge.iter().map(|bucket| bucket.values[0]).collect::<Vec<_>>(), [1, 2, 0]);

    let mut judgements = std::collections::HashMap::new();
    let mut tracker = PlayJudgeGraphTracker::new(&chart, &judgements, &graphs);
    let start = Duration::from_secs(10);
    tracker.publish(start, &mut graphs);
    assert_eq!(graphs, cache.play_note_graphs, "nothing to publish before a judgement");

    judgements.insert(bmz_core::ids::NoteId(1), detail(Judge::Great, -5_000));
    tracker.record(&judgements, &[judged(1)]);
    tracker.publish(start, &mut graphs);
    assert_eq!(graphs.judge[0].values, [0, 0, 1, 0, 0, 0]);
    assert_eq!(graphs.early_late[0].values, [0, 0, 1, 0, 0, 0, 0, 0, 0, 0]);
    assert_ne!(graphs.judge_revision, cache.play_note_graphs.judge_revision);
    assert_eq!(graphs.notes_revision, cache.play_note_graphs.notes_revision);

    // A late Bad waits for the interval, and the next entry moves the same note.
    judgements.insert(bmz_core::ids::NoteId(2), detail(Judge::Bad, 40_000));
    judgements.insert(bmz_core::ids::NoteId(1), detail(Judge::PGreat, 0));
    tracker.record(&judgements, &[judged(2), judged(1)]);
    tracker.publish(start + Duration::from_millis(749), &mut graphs);
    assert_eq!(graphs.judge[1].values, [2, 0, 0, 0, 0, 0]);
    tracker.publish(start + Duration::from_millis(750), &mut graphs);
    assert_eq!(graphs.judge[0].values, [0, 1, 0, 0, 0, 0]);
    assert_eq!(graphs.judge[1].values, [1, 0, 0, 0, 1, 0]);
    assert_eq!(graphs.early_late[1].values, [1, 0, 0, 0, 0, 0, 0, 0, 1, 0]);

    // The same buckets as the Result graph once every note is judged.
    judgements.insert(bmz_core::ids::NoteId(3), detail(Judge::Good, -30_000));
    tracker.record(&judgements, &[judged(3)]);
    tracker.publish(start + Duration::from_secs(2), &mut graphs);
    let result =
        ResultGraphCollector::default().snapshot_for_result_parts(&chart, &judgements, None);
    assert_eq!(graphs.judge.as_ref(), result.judge_graph_buckets.as_slice());
    assert_eq!(graphs.early_late.as_ref(), result.early_late_graph_buckets.as_slice());
    assert_eq!(graphs.notes.as_ref(), result.note_graph_buckets.as_slice());
}

#[test]
fn play_judge_graph_tracker_starts_from_notes_judged_before_the_first_frame() {
    let mut chart = chart();
    chart.end_time = TimeUs(1_000_000);
    chart.lane_notes[Lane::Key1.index()] = vec![note(1, 0), note(2, 0)];
    let cache = crate::screens::play_snapshot::PlayRenderSnapshotCache::from_chart(&chart);
    let judgements =
        std::collections::HashMap::from([(bmz_core::ids::NoteId(1), detail(Judge::PGreat, 0))]);
    let mut graphs = cache.play_note_graphs.clone();
    PlayJudgeGraphTracker::new(&chart, &judgements, &graphs).publish(Duration::ZERO, &mut graphs);
    assert_eq!(graphs.judge[0].values, [1, 1, 0, 0, 0, 0]);
}

#[test]
fn result_graph_collector_builds_beatoraja_result_buckets_from_session_judgements() {
    let mut chart = chart();
    chart.end_time = TimeUs(2_000_000);
    chart.total_notes = 4;
    chart.lane_notes[Lane::Key1.index()] = vec![note(1, 0), note(2, 1_000_000), note(3, 1_000_000)];
    chart.lane_notes[Lane::Scratch.index()].push(NoteEvent {
        id: NoteId(4),
        lane: Lane::Scratch,
        kind: NoteKind::Tap,
        tick: ChartTick(0),
        time: TimeUs(1_000_000),
        sound: None,
        layered_sounds: Vec::new(),
        damage: None,
    });
    let mut judgements = std::collections::HashMap::new();
    judgements.insert(
        bmz_core::ids::NoteId(1),
        bmz_gameplay::session::ResultJudgementDetail {
            judge: Judge::Great,
            side: TimingSide::Fast,
            delta: TimeUs(-12_000),
            time: TimeUs(12_000),
        },
    );
    judgements.insert(
        bmz_core::ids::NoteId(2),
        bmz_gameplay::session::ResultJudgementDetail {
            judge: Judge::Bad,
            side: TimingSide::Slow,
            delta: TimeUs(45_000),
            time: TimeUs(1_045_000),
        },
    );

    let mut graph = ResultGraphSnapshot::default();
    populate_result_note_graphs(&mut graph, &chart, &judgements);

    assert_eq!(graph.judge_graph_buckets[0].values[2], 1);
    assert_eq!(graph.note_graph_buckets[0].values[5], 1);
    assert_eq!(graph.note_graph_buckets[1].values[2], 1);
    assert_eq!(graph.early_late_graph_buckets[0].values[2], 1);
    assert_eq!(graph.judge_graph_buckets[1].values[4], 1);
    assert_eq!(graph.judge_graph_buckets[1].values[0], 2);
    assert_eq!(graph.early_late_graph_buckets[1].values[8], 1);
    assert_eq!(graph.early_late_graph_buckets[1].values[0], 2);
    assert_eq!(
        graph.timing_points.iter().map(|point| point.delta_us).collect::<Vec<_>>(),
        vec![12_000, -45_000]
    );
    assert_eq!(graph.timing_distribution.counts[(150 + 12) as usize], 1);
    assert_eq!(graph.timing_distribution.counts[(150 - 45) as usize], 1);
}

fn note(id: u32, time_us: i64) -> bmz_chart::model::NoteEvent {
    bmz_chart::model::NoteEvent {
        id: bmz_core::ids::NoteId(id),
        lane: Lane::Key1,
        kind: bmz_chart::model::NoteKind::Tap,
        tick: bmz_core::time::ChartTick(0),
        time: TimeUs(time_us),
        sound: None,
        layered_sounds: Vec::new(),
        damage: None,
    }
}

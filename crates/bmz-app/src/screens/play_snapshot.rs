use std::collections::HashMap;

use bmz_chart::model::{BgaAssetId, BgaEventKind, PlayableChart, TimingEventKind};
use bmz_core::judge::{Judge, TimingSide};
use bmz_core::lane::Lane;
use bmz_core::time::TimeUs;
use bmz_gameplay::judge::model::JudgementEvent;
use bmz_gameplay::session::GameSession;
use bmz_render::plan::CHART_BGA_TEXTURE_BASE;
use bmz_render::skin_offset::{SkinOffsetValue, SkinOffsetValues};
use bmz_render::snapshot::{
    DisplayBgaFrame, DisplayInput, DisplayJudgeCounts, DisplayJudgement, RenderSnapshot,
    VisibleBarLine, VisibleLongNote, VisibleNote,
};

pub const DEFAULT_LOOKAHEAD_US: i64 = 2_000_000;
pub type BgaFrameCatalog = HashMap<BgaAssetId, DisplayBgaFrame>;

pub fn build_render_snapshot(
    session: &GameSession,
    render_now: TimeUs,
    recent_judgements: &[JudgementEvent],
    best_ex_score: Option<u32>,
) -> RenderSnapshot {
    build_render_snapshot_with_bga_frames(
        session,
        render_now,
        recent_judgements,
        best_ex_score,
        &BgaFrameCatalog::new(),
    )
}

pub fn build_render_snapshot_with_bga_frames(
    session: &GameSession,
    render_now: TimeUs,
    recent_judgements: &[JudgementEvent],
    best_ex_score: Option<u32>,
    bga_frames: &BgaFrameCatalog,
) -> RenderSnapshot {
    let mut snapshot = RenderSnapshot {
        time: render_now,
        duration: session.chart.end_time,
        title: session.chart.metadata.title.clone(),
        subtitle: session.chart.metadata.subtitle.clone(),
        artist: session.chart.metadata.artist.clone(),
        subartist: session.chart.metadata.subartist.clone(),
        genre: session.chart.metadata.genre.clone(),
        combo: session.score.combo,
        max_combo: session.score.max_combo,
        ex_score: session.score.ex_score(),
        total_notes: session.chart.total_notes,
        past_notes: session.score.past_notes,
        judge_counts: display_judge_counts(session),
        gauge: session.gauge.current().value,
        gauge_type: session.gauge.current().definition.gauge_type as i32,
        hispeed: session.hispeed,
        lift: session.lift,
        lane_cover: if session.lane_cover_visible { session.lane_cover } else { 0.0 },
        hidden_cover: session.hidden_cover,
        skin_offsets: skin_offsets_from_session(session),
        now_bpm: current_bpm(&session.chart, render_now) as f32,
        min_bpm: chart_min_bpm(&session.chart) as f32,
        max_bpm: chart_max_bpm(&session.chart) as f32,
        has_bga: session.chart.metadata.has_bga,
        bga_enabled: session.bga_enabled,
        bga_base: session
            .bga_enabled
            .then(|| current_bga_frame(&session.chart, render_now, BgaEventKind::Base, bga_frames))
            .flatten(),
        bga_layer: session
            .bga_enabled
            .then(|| current_bga_frame(&session.chart, render_now, BgaEventKind::Layer, bga_frames))
            .flatten(),
        bga_poor: session
            .bga_enabled
            .then(|| {
                current_poor_bga_frame(
                    &session.chart,
                    render_now,
                    recent_judgements,
                    bga_frames,
                    session.poor_bga_duration_us,
                )
            })
            .flatten(),
        bga_stretch: session.bga_stretch,
        best_ex_score,
        target_ex_score: None, // TODO: resolve from rival / target config
        judge_timing_offset_ms: (session.offsets.input_offset_us / 1_000) as i32,
        key_mode: session.chart.metadata.key_mode,
        visible_notes: std::array::from_fn(|_| Vec::new()),
        recent_inputs: session
            .recent_inputs
            .iter()
            .map(|input| DisplayInput { lane: input.lane, time: input.time })
            .collect(),
        recent_judgements: recent_judgements.iter().map(display_judgement).collect(),
        bar_lines: Vec::new(),
        visible_long_notes: Vec::new(),
    };

    // SUDDEN+（レーンカバー）はノーツの可視域を上端側から縮める。
    // beatoraja の currentduration = region * (1 - lanecover) と同じ。
    // 可視進捗の上限が visible_max になり、それより奥のノーツ・小節線は描画しない。
    let effective_lane_cover = if session.lane_cover_visible { session.lane_cover } else { 0.0 };
    let visible_max = (1.0 - effective_lane_cover).clamp(0.0, 1.0);

    for lane in Lane::ALL {
        let next_note_index = session.judge.lanes[lane.index()].next_note_index;
        for note in session.chart.notes_for_lane(lane).iter().skip(next_note_index) {
            if let Some(y) = note_y(note.time, render_now, session.hispeed, visible_max) {
                snapshot.visible_notes[lane.index()].push(VisibleNote { lane, time: note.time, y });
            }
        }
    }

    for bar in &session.chart.bar_lines {
        if let Some(y) = note_y(bar.time, render_now, session.hispeed, visible_max) {
            snapshot.bar_lines.push(VisibleBarLine { time: bar.time, y });
        }
    }

    for long in &session.chart.long_notes {
        let head = note_progress(long.start_time, render_now, session.hispeed);
        let tail = note_progress(long.end_time, render_now, session.hispeed);
        // 終端が判定ラインを過ぎた、または始端がカバー域より奥なら非表示。
        // ホールド中のLNは head < 0 になるが tail は可視域に残るので表示される。
        // 始端・終端ともレーンカバーの可視上限 visible_max でクランプする。
        if tail < 0.0 || head > visible_max {
            continue;
        }
        snapshot.visible_long_notes.push(VisibleLongNote {
            lane: long.lane,
            head_y: head.clamp(0.0, visible_max),
            tail_y: tail.clamp(0.0, visible_max),
        });
    }

    snapshot
}

fn current_poor_bga_frame(
    chart: &PlayableChart,
    render_now: TimeUs,
    recent_judgements: &[JudgementEvent],
    bga_frames: &BgaFrameCatalog,
    duration_us: i64,
) -> Option<DisplayBgaFrame> {
    if duration_us <= 0 {
        return None;
    }

    let judgement = recent_judgements.iter().rev().find(|event| {
        matches!(event.judge, Judge::Bad | Judge::Poor)
            && render_now.0 >= event.time.0
            && render_now.0 < event.time.0 + duration_us
    })?;
    current_bga_frame(chart, judgement.time, BgaEventKind::Poor, bga_frames)
}

fn current_bga_frame(
    chart: &PlayableChart,
    render_now: TimeUs,
    kind: BgaEventKind,
    bga_frames: &BgaFrameCatalog,
) -> Option<DisplayBgaFrame> {
    let event = chart
        .bga_events
        .iter()
        .rev()
        .find(|event| event.time <= render_now && event.kind == kind)?;
    bga_frames.get(&event.asset).copied()
}

pub fn display_bga_frame(id: BgaAssetId, width: u32, height: u32) -> DisplayBgaFrame {
    DisplayBgaFrame {
        texture_id: bga_texture_id(id),
        width: width.max(1) as f32,
        height: height.max(1) as f32,
    }
}

pub fn bga_texture_id(id: BgaAssetId) -> u32 {
    CHART_BGA_TEXTURE_BASE + id.0
}

fn skin_offsets_from_session(session: &GameSession) -> SkinOffsetValues {
    let mut values = SkinOffsetValues::default();
    for offset in &session.skin_offsets {
        values.set(
            offset.id,
            SkinOffsetValue {
                x: offset.x,
                y: offset.y,
                w: offset.w,
                h: offset.h,
                r: offset.r,
                a: offset.a,
            },
        );
    }
    values
}

/// ノートの正規化進捗（0.0=判定ライン, 1.0=可視域最奥）を返す。
/// 判定ラインを過ぎた（delta<0）か、`visible_max` を超えるノートは `None`。
/// `visible_max` はレーンカバーで縮んだ可視上限（カバー無しなら 1.0）。
fn note_y(note_time: TimeUs, render_now: TimeUs, hispeed: f32, visible_max: f32) -> Option<f32> {
    let delta = note_time.0 - render_now.0;
    if delta < 0 {
        return None;
    }

    let progress = delta as f32 * hispeed / DEFAULT_LOOKAHEAD_US as f32;
    (progress <= visible_max).then_some(progress)
}

/// `note_y` と同じ正規化進捗を返すが、可視判定・クランプをしない生の値。
/// 判定ラインを過ぎたノートは負値、可視域より奥は 1.0 超になる。
/// ロングノートの始端・終端位置の算出に使う。
fn note_progress(note_time: TimeUs, render_now: TimeUs, hispeed: f32) -> f32 {
    let delta = note_time.0 - render_now.0;
    delta as f32 * hispeed / DEFAULT_LOOKAHEAD_US as f32
}

fn display_judge_counts(session: &GameSession) -> DisplayJudgeCounts {
    let judges = &session.score.judges;
    DisplayJudgeCounts {
        pgreat: judges.fast_pgreat + judges.slow_pgreat,
        great: judges.fast_great + judges.slow_great,
        good: judges.fast_good + judges.slow_good,
        bad: judges.fast_bad + judges.slow_bad,
        poor: judges.fast_poor + judges.slow_poor,
        empty_poor: judges.fast_empty_poor + judges.slow_empty_poor,
    }
}

fn display_judgement(event: &JudgementEvent) -> DisplayJudgement {
    DisplayJudgement {
        lane: event.lane,
        text: format!("{}{}", judge_text(event.judge), side_suffix(event.side)),
        delta_us: event.delta.0,
        time: event.time,
        is_miss: event.judge == Judge::Poor,
    }
}

/// `render_now` の時点で有効な BPM を返す。
fn current_bpm(chart: &bmz_chart::model::PlayableChart, render_now: TimeUs) -> f64 {
    let mut bpm = chart.metadata.initial_bpm;
    for event in &chart.timing_events {
        if event.time > render_now {
            break;
        }
        if let TimingEventKind::BpmChange { bpm: b } = event.kind {
            bpm = b;
        }
    }
    bpm
}

fn chart_min_bpm(chart: &bmz_chart::model::PlayableChart) -> f64 {
    chart
        .timing_events
        .iter()
        .filter_map(
            |e| if let TimingEventKind::BpmChange { bpm } = e.kind { Some(bpm) } else { None },
        )
        .fold(chart.metadata.initial_bpm, f64::min)
}

fn chart_max_bpm(chart: &bmz_chart::model::PlayableChart) -> f64 {
    chart
        .timing_events
        .iter()
        .filter_map(
            |e| if let TimingEventKind::BpmChange { bpm } = e.kind { Some(bpm) } else { None },
        )
        .fold(chart.metadata.initial_bpm, f64::max)
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
        let mut session =
            build_game_session(Arc::new(chart()), &profile, PlaySessionOptions::default());
        session.hispeed = 1.0;
        let judgements = vec![JudgementEvent {
            note_id: Some(NoteId(1)),
            lane: Lane::Key1,
            judge: Judge::EmptyPoor,
            side: TimingSide::Slow,
            delta: TimeUs(5_000),
            time: TimeUs(1_000),
        }];

        let snapshot = build_render_snapshot(&session, TimeUs(0), &judgements, None);

        assert_eq!(snapshot.combo, 0);
        assert_eq!(snapshot.max_combo, 0);
        assert_eq!(snapshot.ex_score, 0);
        assert_eq!(snapshot.total_notes, 1);
        assert_eq!(snapshot.past_notes, 0);
        assert!(snapshot.recent_inputs.is_empty());
        assert_eq!(snapshot.visible_notes[Lane::Key1.index()].len(), 1);
        assert_eq!(snapshot.visible_notes[Lane::Key1.index()][0].y, 0.5);
        assert_eq!(snapshot.recent_judgements[0].lane, Lane::Key1);
        assert_eq!(snapshot.recent_judgements[0].text, "EMPTY POOR SLOW");
        assert_eq!(snapshot.recent_judgements[0].delta_us, 5_000);
    }

    #[test]
    fn build_render_snapshot_normalizes_note_y_to_visible_range() {
        let profile = ProfileConfig::new_default("default", "Default", 1);
        let mut session =
            build_game_session(Arc::new(chart()), &profile, PlaySessionOptions::default());
        session.hispeed = 1.0;

        let early = build_render_snapshot(&session, TimeUs(0), &[], None);
        let later = build_render_snapshot(&session, TimeUs(750_000), &[], None);

        assert_eq!(early.visible_notes[Lane::Key1.index()][0].y, 0.5);
        assert_eq!(later.visible_notes[Lane::Key1.index()][0].y, 0.125);
    }

    #[test]
    fn build_render_snapshot_culls_notes_under_lane_cover() {
        let profile = ProfileConfig::new_default("default", "Default", 1);
        let mut session =
            build_game_session(Arc::new(chart()), &profile, PlaySessionOptions::default());
        session.hispeed = 1.0;
        // Key1 のノートは render_now=0 で progress 0.5 (time 1_000_000 / lookahead 2_000_000)

        // lane_cover=0.3 → visible_max=0.7 → progress 0.5 は可視
        session.lane_cover = 0.3;
        let visible = build_render_snapshot(&session, TimeUs(0), &[], None);
        assert_eq!(visible.visible_notes[Lane::Key1.index()].len(), 1);

        // lane_cover=0.6 → visible_max=0.4 → progress 0.5 はカバー域なので除外
        session.lane_cover = 0.6;
        let culled = build_render_snapshot(&session, TimeUs(0), &[], None);
        assert!(culled.visible_notes[Lane::Key1.index()].is_empty());
    }

    #[test]
    fn build_render_snapshot_applies_hispeed_to_note_positions() {
        let profile = ProfileConfig::new_default("default", "Default", 1);
        let mut session =
            build_game_session(Arc::new(chart()), &profile, PlaySessionOptions::default());
        session.hispeed = 2.0;

        let snapshot = build_render_snapshot(&session, TimeUs(0), &[], None);

        assert_eq!(snapshot.hispeed, 2.0);
        assert_eq!(snapshot.visible_notes[Lane::Key1.index()][0].y, 1.0);
    }

    #[test]
    fn build_render_snapshot_hides_consumed_notes() {
        let profile = ProfileConfig::new_default("default", "Default", 1);
        let mut session =
            build_game_session(Arc::new(chart()), &profile, PlaySessionOptions::default());
        session.judge.lanes[Lane::Key1.index()].next_note_index = 1;

        let snapshot = build_render_snapshot(&session, TimeUs(0), &[], None);

        assert!(snapshot.visible_notes[Lane::Key1.index()].is_empty());
    }

    #[test]
    fn build_render_snapshot_copies_recent_inputs() {
        use bmz_core::input::{InputEvent, InputKind, InputSource};

        let profile = ProfileConfig::new_default("default", "Default", 1);
        let mut session =
            build_game_session(Arc::new(chart()), &profile, PlaySessionOptions::default());
        session.recent_inputs.push(InputEvent {
            lane: Lane::Key3,
            kind: InputKind::Press,
            time: TimeUs(42_000),
            source: InputSource::Human,
        });

        let snapshot = build_render_snapshot(&session, TimeUs(50_000), &[], None);

        assert_eq!(snapshot.recent_inputs.len(), 1);
        assert_eq!(snapshot.recent_inputs[0].lane, Lane::Key3);
        assert_eq!(snapshot.recent_inputs[0].time, TimeUs(42_000));
    }

    #[test]
    fn build_render_snapshot_sums_judge_counts() {
        let profile = ProfileConfig::new_default("default", "Default", 1);
        let mut session =
            build_game_session(Arc::new(chart()), &profile, PlaySessionOptions::default());
        session.score.apply(&JudgementEvent {
            note_id: Some(NoteId(1)),
            lane: Lane::Key1,
            judge: Judge::PGreat,
            side: TimingSide::Fast,
            delta: TimeUs(-1_000),
            time: TimeUs(1_000),
        });
        session.score.apply(&JudgementEvent {
            note_id: None,
            lane: Lane::Key1,
            judge: Judge::EmptyPoor,
            side: TimingSide::Slow,
            delta: TimeUs(40_000),
            time: TimeUs(2_000),
        });

        let snapshot = build_render_snapshot(&session, TimeUs(0), &[], None);

        assert_eq!(snapshot.judge_counts.pgreat, 1);
        assert_eq!(snapshot.judge_counts.empty_poor, 1);
    }

    #[test]
    fn build_render_snapshot_passes_best_ex_score() {
        let profile = ProfileConfig::new_default("default", "Default", 1);
        let session =
            build_game_session(Arc::new(chart()), &profile, PlaySessionOptions::default());

        let with_best = build_render_snapshot(&session, TimeUs(0), &[], Some(42));
        let without_best = build_render_snapshot(&session, TimeUs(0), &[], None);

        assert_eq!(with_best.best_ex_score, Some(42));
        assert_eq!(without_best.best_ex_score, None);
    }

    #[test]
    fn build_render_snapshot_derives_judge_timing_offset_from_session() {
        use bmz_gameplay::session::PlayOffsets;

        let profile = ProfileConfig::new_default("default", "Default", 1);
        let mut session =
            build_game_session(Arc::new(chart()), &profile, PlaySessionOptions::default());
        session.offsets = PlayOffsets { input_offset_us: 3_000, visual_offset_us: 0 };

        let snapshot = build_render_snapshot(&session, TimeUs(0), &[], None);

        assert_eq!(snapshot.judge_timing_offset_ms, 3);
    }

    #[test]
    fn build_render_snapshot_copies_skin_offsets() {
        let profile = ProfileConfig::new_default("default", "Default", 1);
        let mut session =
            build_game_session(Arc::new(chart()), &profile, PlaySessionOptions::default());
        session.skin_offsets.push(bmz_gameplay::session::PlaySkinOffset {
            id: 42,
            x: 1,
            y: 2,
            w: 3,
            h: 4,
            r: 5,
            a: -6,
        });

        let snapshot = build_render_snapshot(&session, TimeUs(0), &[], None);

        assert_eq!(
            snapshot.skin_offsets.get(42),
            Some(SkinOffsetValue { x: 1, y: 2, w: 3, h: 4, r: 5, a: -6 })
        );
    }

    #[test]
    fn build_render_snapshot_selects_current_bga_frames() {
        use bmz_chart::model::{BgaAssetKind, BgaAssetRef, BgaEvent};

        let profile = ProfileConfig::new_default("default", "Default", 1);
        let mut chart = chart();
        chart.metadata.has_bga = true;
        chart.bga_assets = vec![
            BgaAssetRef {
                id: BgaAssetId(0),
                path: "base-a.png".into(),
                kind: BgaAssetKind::Static,
            },
            BgaAssetRef {
                id: BgaAssetId(1),
                path: "base-b.png".into(),
                kind: BgaAssetKind::Static,
            },
            BgaAssetRef { id: BgaAssetId(2), path: "layer.png".into(), kind: BgaAssetKind::Static },
            BgaAssetRef { id: BgaAssetId(3), path: "poor.png".into(), kind: BgaAssetKind::Static },
        ];
        chart.bga_events = vec![
            BgaEvent {
                tick: ChartTick(0),
                time: TimeUs(0),
                asset: BgaAssetId(0),
                kind: BgaEventKind::Base,
            },
            BgaEvent {
                tick: ChartTick(0),
                time: TimeUs(500_000),
                asset: BgaAssetId(1),
                kind: BgaEventKind::Base,
            },
            BgaEvent {
                tick: ChartTick(0),
                time: TimeUs(250_000),
                asset: BgaAssetId(2),
                kind: BgaEventKind::Layer,
            },
            BgaEvent {
                tick: ChartTick(0),
                time: TimeUs(300_000),
                asset: BgaAssetId(3),
                kind: BgaEventKind::Poor,
            },
        ];
        let mut session =
            build_game_session(Arc::new(chart), &profile, PlaySessionOptions::default());
        session.poor_bga_duration_us = 250_000;
        let bga_frames = BgaFrameCatalog::from([
            (BgaAssetId(0), display_bga_frame(BgaAssetId(0), 256, 256)),
            (BgaAssetId(1), display_bga_frame(BgaAssetId(1), 640, 480)),
            (BgaAssetId(2), display_bga_frame(BgaAssetId(2), 1280, 720)),
            (BgaAssetId(3), display_bga_frame(BgaAssetId(3), 320, 240)),
        ]);
        let poor_judgements = [JudgementEvent {
            note_id: Some(NoteId(1)),
            lane: Lane::Key1,
            judge: Judge::Poor,
            side: TimingSide::Slow,
            delta: TimeUs(0),
            time: TimeUs(400_000),
        }];

        let early = build_render_snapshot_with_bga_frames(
            &session,
            TimeUs(100_000),
            &[],
            None,
            &bga_frames,
        );
        let late = build_render_snapshot_with_bga_frames(
            &session,
            TimeUs(600_000),
            &[],
            None,
            &bga_frames,
        );
        let poor_active = build_render_snapshot_with_bga_frames(
            &session,
            TimeUs(600_000),
            &poor_judgements,
            None,
            &bga_frames,
        );
        let poor_expired = build_render_snapshot_with_bga_frames(
            &session,
            TimeUs(651_000),
            &poor_judgements,
            None,
            &bga_frames,
        );

        assert_eq!(early.bga_base.unwrap().texture_id, bga_texture_id(BgaAssetId(0)));
        assert!(early.bga_layer.is_none());
        assert_eq!(
            late.bga_base.unwrap(),
            DisplayBgaFrame {
                texture_id: bga_texture_id(BgaAssetId(1)),
                width: 640.0,
                height: 480.0
            }
        );
        assert_eq!(late.bga_layer.unwrap().texture_id, bga_texture_id(BgaAssetId(2)));
        assert_eq!(
            poor_active.bga_poor.unwrap(),
            DisplayBgaFrame {
                texture_id: bga_texture_id(BgaAssetId(3)),
                width: 320.0,
                height: 240.0
            }
        );
        assert!(poor_expired.bga_poor.is_none());
    }

    #[test]
    fn current_bpm_returns_initial_bpm_before_first_change() {
        let chart = chart_with_bpm_changes();
        // At time 0, before any BPM change
        assert_eq!(current_bpm(&chart, TimeUs(0)), 120.0);
    }

    #[test]
    fn current_bpm_returns_changed_bpm_after_event() {
        let chart = chart_with_bpm_changes();
        // BPM changes to 180 at t=500_000 µs
        assert_eq!(current_bpm(&chart, TimeUs(500_000)), 180.0);
        // BPM changes to 90 at t=1_000_000 µs
        assert_eq!(current_bpm(&chart, TimeUs(1_000_000)), 90.0);
        // After last change
        assert_eq!(current_bpm(&chart, TimeUs(2_000_000)), 90.0);
    }

    #[test]
    fn chart_min_bpm_returns_minimum_across_all_events() {
        let chart = chart_with_bpm_changes();
        // initial=120, events: 180, 90 → min=90
        assert_eq!(chart_min_bpm(&chart), 90.0);
    }

    #[test]
    fn chart_max_bpm_returns_maximum_across_all_events() {
        let chart = chart_with_bpm_changes();
        // initial=120, events: 180, 90 → max=180
        assert_eq!(chart_max_bpm(&chart), 180.0);
    }

    #[test]
    fn bpm_helpers_use_initial_bpm_when_no_timing_events() {
        let chart = chart(); // no timing_events
        assert_eq!(current_bpm(&chart, TimeUs(0)), 120.0);
        assert_eq!(chart_min_bpm(&chart), 120.0);
        assert_eq!(chart_max_bpm(&chart), 120.0);
    }

    fn chart_with_bpm_changes() -> PlayableChart {
        use bmz_chart::model::{TimingEvent, TimingEventKind};
        PlayableChart {
            identity: compute_chart_identity(b"bpm-test"),
            metadata: ChartMetadata { initial_bpm: 120.0, ..Default::default() },
            lane_notes: std::array::from_fn(|_| Vec::new()),
            long_notes: Vec::new(),
            bgm_events: Vec::new(),
            bga_events: Vec::new(),
            timing_events: vec![
                TimingEvent {
                    tick: ChartTick(0),
                    time: TimeUs(500_000),
                    kind: TimingEventKind::BpmChange { bpm: 180.0 },
                },
                TimingEvent {
                    tick: ChartTick(0),
                    time: TimeUs(1_000_000),
                    kind: TimingEventKind::BpmChange { bpm: 90.0 },
                },
            ],
            bar_lines: Vec::new(),
            sounds: Vec::new(),
            bga_assets: Vec::new(),
            total_notes: 0,
            end_time: TimeUs(2_000_000),
        }
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
            bga_events: Vec::new(),
            timing_events: Vec::new(),
            bar_lines: Vec::new(),
            sounds: Vec::new(),
            bga_assets: Vec::new(),
            total_notes: 1,
            end_time: TimeUs(1_000_000),
        }
    }

    /// Key1 に start=500ms, end=1500ms のロングノートを1本持つ譜面。
    fn chart_with_long_note() -> PlayableChart {
        use bmz_chart::model::{LongNotePair, LongNoteStyle};

        let start = NoteEvent {
            id: NoteId(1),
            lane: Lane::Key1,
            kind: NoteKind::LongStart,
            tick: ChartTick(0),
            time: TimeUs(500_000),
            sound: None,
        };
        let end = NoteEvent {
            id: NoteId(2),
            lane: Lane::Key1,
            kind: NoteKind::LongEnd,
            tick: ChartTick(0),
            time: TimeUs(1_500_000),
            sound: None,
        };
        let mut lane_notes = std::array::from_fn(|_| Vec::new());
        lane_notes[Lane::Key1.index()].push(start);
        lane_notes[Lane::Key1.index()].push(end);

        PlayableChart {
            identity: compute_chart_identity(b"long-note"),
            metadata: ChartMetadata { initial_bpm: 120.0, ..Default::default() },
            lane_notes,
            long_notes: vec![LongNotePair {
                lane: Lane::Key1,
                style: LongNoteStyle::ChannelPair,
                start_note_id: NoteId(1),
                end_note_id: NoteId(2),
                start_tick: ChartTick(0),
                end_tick: ChartTick(0),
                start_time: TimeUs(500_000),
                end_time: TimeUs(1_500_000),
                sound: None,
            }],
            bgm_events: Vec::new(),
            bga_events: Vec::new(),
            timing_events: Vec::new(),
            bar_lines: Vec::new(),
            sounds: Vec::new(),
            bga_assets: Vec::new(),
            total_notes: 1,
            end_time: TimeUs(1_500_000),
        }
    }

    #[test]
    fn build_render_snapshot_emits_visible_long_note() {
        let profile = ProfileConfig::new_default("default", "Default", 1);
        let mut session = build_game_session(
            Arc::new(chart_with_long_note()),
            &profile,
            PlaySessionOptions::default(),
        );
        session.hispeed = 1.0;

        // render_now=0: start 500ms→0.25, end 1500ms→0.75 (lookahead 2s)
        let upcoming = build_render_snapshot(&session, TimeUs(0), &[], None);
        assert_eq!(upcoming.visible_long_notes.len(), 1);
        assert_eq!(upcoming.visible_long_notes[0].lane, Lane::Key1);
        assert_eq!(upcoming.visible_long_notes[0].head_y, 0.25);
        assert_eq!(upcoming.visible_long_notes[0].tail_y, 0.75);
    }

    #[test]
    fn build_render_snapshot_clamps_held_long_note_head_to_judge_line() {
        let profile = ProfileConfig::new_default("default", "Default", 1);
        let mut session = build_game_session(
            Arc::new(chart_with_long_note()),
            &profile,
            PlaySessionOptions::default(),
        );
        session.hispeed = 1.0;

        // render_now=1_000_000: 始端は判定ライン通過済み(負値→0.0)、終端は 0.25
        let held = build_render_snapshot(&session, TimeUs(1_000_000), &[], None);
        assert_eq!(held.visible_long_notes.len(), 1);
        assert_eq!(held.visible_long_notes[0].head_y, 0.0);
        assert_eq!(held.visible_long_notes[0].tail_y, 0.25);

        // 終端も通過したら非表示
        let passed = build_render_snapshot(&session, TimeUs(2_000_000), &[], None);
        assert!(passed.visible_long_notes.is_empty());
    }
}

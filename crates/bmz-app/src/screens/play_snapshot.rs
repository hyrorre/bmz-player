use std::collections::HashMap;

use bmz_chart::model::{BgaAssetId, BgaEventKind, NoteKind, PlayableChart, TimingEventKind};
use bmz_chart::timing::{TICKS_PER_BEAT, TimingMap};
use bmz_core::judge::{Judge, TimingSide};
use bmz_core::lane::Lane;
use bmz_core::time::TimeUs;
use bmz_gameplay::judge::model::JudgementEvent;
use bmz_gameplay::session::GameSession;
use bmz_render::plan::CHART_BGA_TEXTURE_BASE;
use bmz_render::skin_offset::{SkinOffsetValue, SkinOffsetValues};
use bmz_render::snapshot::{
    DisplayBgaFrame, DisplayInput, DisplayJudgeCounts, DisplayJudgement, OverlaySnapshot,
    RenderSnapshot, VisibleBarLine, VisibleLongNote, VisibleMine, VisibleNote,
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
        play_elapsed_time: render_now,
        ready_elapsed_time: None,
        duration: session.chart.end_time,
        title: session.chart.metadata.title.clone(),
        subtitle: session.chart.metadata.subtitle.clone(),
        artist: session.chart.metadata.artist.clone(),
        subartist: session.chart.metadata.subartist.clone(),
        genre: session.chart.metadata.genre.clone(),
        difficulty_name: session.chart.metadata.difficulty_name.clone(),
        judge_rank: session.chart.metadata.judge_rank,
        play_level: session.chart.metadata.play_level.clone(),
        combo: session.score.combo,
        max_combo: session.score.max_combo,
        ex_score: session.score.ex_score(),
        total_notes: session.chart.total_notes,
        past_notes: session.score.past_notes,
        judge_counts: display_judge_counts(session),
        fast_slow_counts: display_fast_slow_counts(session),
        gauge: session.gauge.current().value,
        gauge_type: session.gauge.current().definition.gauge_type as i32,
        hispeed: session.hispeed,
        lift: session.lift,
        lane_cover: if session.lane_cover_visible { session.lane_cover } else { 0.0 },
        lane_cover_changing: session.lane_cover_changing,
        lanecover_enabled: session.lanecover_enabled,
        lift_enabled: session.lift_enabled,
        hidden_enabled: session.hidden_enabled,
        note_display_duration_ms: note_display_duration_ms(session, render_now),
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
        autoplay: session.autoplay.is_some(),
        course_stage: None,
        key_mode: session.chart.metadata.key_mode,
        visible_notes: std::array::from_fn(|_| Vec::new()),
        visible_mines: std::array::from_fn(|_| Vec::new()),
        recent_inputs: session
            .recent_inputs
            .iter()
            .map(|input| DisplayInput { lane: input.lane, time: input.time })
            .collect(),
        recent_judgements: recent_judgements.iter().map(display_judgement).collect(),
        full_combo_elapsed_ms: session.full_combo_started_at.and_then(|started_at| {
            (render_now.0 >= started_at.0)
                .then_some(((render_now.0 - started_at.0) / 1_000).clamp(0, i32::MAX as i64) as i32)
        }),
        fadeout_elapsed_ms: None,
        failed_elapsed_ms: None,
        music_end_elapsed_ms: None,
        bar_lines: Vec::new(),
        visible_long_notes: Vec::new(),
        keyon_ms: std::array::from_fn(|lane_index| {
            session.lane_keyon_started_at[lane_index].map(|t| {
                ((render_now.0 - t.0) / 1_000).clamp(i32::MIN as i64, i32::MAX as i64) as i32
            })
        }),
        keyoff_ms: std::array::from_fn(|lane_index| {
            session.lane_keyoff_started_at[lane_index].map(|t| {
                ((render_now.0 - t.0) / 1_000).clamp(i32::MIN as i64, i32::MAX as i64) as i32
            })
        }),
        overlay: OverlaySnapshot::default(),
    };

    let scroll = ScrollContext::new(session);
    let cursor_tick = scroll.cursor_tick(render_now);

    for lane in Lane::ALL {
        let next_note_index = session.judge.lanes[lane.index()].next_note_index;
        for note in session.chart.notes_for_lane(lane).iter().skip(next_note_index) {
            match note.kind {
                NoteKind::Invisible => continue,
                NoteKind::Mine => {
                    if let Some(y) = scroll.note_y(note.time, cursor_tick) {
                        snapshot.visible_mines[lane.index()].push(VisibleMine {
                            lane,
                            time: note.time,
                            y,
                            damage: note.damage.unwrap_or(0),
                        });
                    }
                }
                NoteKind::Tap | NoteKind::LongStart | NoteKind::LongEnd => {
                    if let Some(y) = scroll.note_y(note.time, cursor_tick) {
                        snapshot.visible_notes[lane.index()].push(VisibleNote {
                            lane,
                            time: note.time,
                            y,
                        });
                    }
                }
            }
        }
    }

    for bar in &session.chart.bar_lines {
        if let Some(y) = scroll.note_y(bar.time, cursor_tick) {
            snapshot.bar_lines.push(VisibleBarLine { time: bar.time, y });
        }
    }

    for long in &session.chart.long_notes {
        let head = scroll.note_progress(long.start_time, cursor_tick);
        let tail = scroll.note_progress(long.end_time, cursor_tick);
        // 終端が判定ラインを過ぎた、または始端が画面上端より奥なら非表示。
        // lane cover は前面描画で隠すだけで、ノーツのカリング範囲は変えない。
        if tail < 0.0 || head > 1.0 {
            continue;
        }
        snapshot.visible_long_notes.push(VisibleLongNote {
            lane: long.lane,
            head_y: head.clamp(0.0, 1.0),
            tail_y: tail.clamp(0.0, 1.0),
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

fn note_display_duration_ms(session: &GameSession, render_now: TimeUs) -> i32 {
    let hispeed = session.hispeed.max(0.01);
    let lane_cover = if session.lane_cover_visible { session.lane_cover } else { 0.0 };
    let visible_max = (1.0 - lane_cover).clamp(0.0, 1.0);
    // BPM スクロールでは可視時間が現在 BPM に反比例する。譜面の基準 BPM (initial_bpm)
    // 比で補正することで、緑数字が今の流速に追従する。
    let initial_bpm = session.chart.metadata.initial_bpm.max(1.0);
    let now_bpm = current_bpm(&session.chart, render_now).max(1.0);
    let bpm_ratio = (initial_bpm / now_bpm) as f32;
    ((DEFAULT_LOOKAHEAD_US as f32 / hispeed * visible_max * bpm_ratio) / 1_000.0)
        .round()
        .clamp(0.0, i32::MAX as f32) as i32
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

/// BPM 変化と STOP に追従した tick ベースのスクロール計算ヘルパ。
///
/// 「lookahead = `DEFAULT_LOOKAHEAD_US` を譜面の `initial_bpm` で換算した tick 数」を
/// 基準にし、現在カーソル tick との差分でノートの y を出す。これにより BPM が
/// 上がれば見かけのスクロール速度も上がり、STOP 中はカーソル tick が停止する。
struct ScrollContext<'a> {
    timing_map: &'a TimingMap,
    hispeed: f32,
    lookahead_ticks: f64,
    /// SCROLL イベント (tick 昇順)。`(tick, factor)`。
    /// 区間ごとに factor を掛けて scroll 位置を畳む。空なら factor 1.0 固定。
    scroll_segments: Vec<(f64, f64)>,
    /// SPEED イベント (tick 昇順)。beatoraja は線形補間だが、まずは SCROLL と同じ
    /// 階段関数で扱い、note 位置時点での値を倍率として掛ける。
    speed_segments: Vec<(f64, f64)>,
}

impl<'a> ScrollContext<'a> {
    fn new(session: &'a GameSession) -> Self {
        let initial_bpm = session.chart.metadata.initial_bpm.max(1.0);
        let lookahead_ticks =
            initial_bpm * DEFAULT_LOOKAHEAD_US as f64 * TICKS_PER_BEAT as f64 / 60_000_000.0;
        let scroll_segments =
            session.chart.scroll_events.iter().map(|e| (e.tick.0 as f64, e.factor)).collect();
        let speed_segments =
            session.chart.speed_events.iter().map(|e| (e.tick.0 as f64, e.factor)).collect();
        Self {
            timing_map: &session.timing_map,
            hispeed: session.hispeed,
            lookahead_ticks,
            scroll_segments,
            speed_segments,
        }
    }

    fn cursor_tick(&self, render_now: TimeUs) -> f64 {
        self.timing_map.time_to_tick_f64(render_now)
    }

    /// ノートの正規化進捗（0.0=判定ライン, 1.0=画面上端）。判定ラインより手前 (delta<0)
    /// と画面上端より奥のノートは `None`。SCROLL / SPEED 倍率を畳み込む。
    fn note_y(&self, note_time: TimeUs, cursor_tick: f64) -> Option<f32> {
        let note_tick = self.timing_map.time_to_tick_f64(note_time);
        let delta = self.scroll_delta(cursor_tick, note_tick);
        if delta < 0.0 {
            return None;
        }
        let progress = (delta / self.lookahead_ticks) as f32 * self.hispeed;
        (progress <= 1.0).then_some(progress)
    }

    /// `note_y` と同じ進捗のクランプしない生値。ロングノートの始端/終端で使う。
    fn note_progress(&self, note_time: TimeUs, cursor_tick: f64) -> f32 {
        let note_tick = self.timing_map.time_to_tick_f64(note_time);
        let delta = self.scroll_delta(cursor_tick, note_tick);
        (delta / self.lookahead_ticks) as f32 * self.hispeed
    }

    /// `from..to` の tick 区間にわたって SCROLL の factor を畳み込み、note 位置の
    /// SPEED 倍率を掛けた「見かけの距離」を返す。factor が負だと delta も負になり、
    /// note_y は `None` に倒れる(= 逆スクロール時は画面外として描画対象外)。
    fn scroll_delta(&self, from_tick: f64, to_tick: f64) -> f64 {
        accumulate_scroll(&self.scroll_segments, from_tick, to_tick)
            * speed_at(&self.speed_segments, to_tick)
    }
}

/// `segments` を階段関数として `from..to` の区間積分を返す。factor は次のイベントまで
/// 一定。`from > to` の場合は対称に負値を返す。
fn accumulate_scroll(segments: &[(f64, f64)], from_tick: f64, to_tick: f64) -> f64 {
    if (from_tick - to_tick).abs() < f64::EPSILON {
        return 0.0;
    }
    let (lo, hi, sign) =
        if from_tick <= to_tick { (from_tick, to_tick, 1.0) } else { (to_tick, from_tick, -1.0) };
    let mut acc = 0.0;
    let mut prev = lo;
    let mut factor = factor_before(segments, lo);
    for &(tick, next_factor) in segments {
        if tick <= lo {
            continue;
        }
        if tick >= hi {
            break;
        }
        acc += (tick - prev) * factor;
        prev = tick;
        factor = next_factor;
    }
    acc += (hi - prev) * factor;
    acc * sign
}

/// 指定 tick 直前(同時刻も含む)の factor 値を返す(イベント未定義なら 1.0)。
fn factor_before(segments: &[(f64, f64)], tick: f64) -> f64 {
    let mut current = 1.0;
    for &(t, f) in segments {
        if t > tick {
            break;
        }
        current = f;
    }
    current
}

/// 指定 tick における SPEED の現在値を返す。beatoraja 仕様に合わせ、隣接イベント間は
/// 線形補間。最初のイベント前は 1.0、最後のイベント以降はその値で固定。
fn speed_at(segments: &[(f64, f64)], tick: f64) -> f64 {
    if segments.is_empty() {
        return 1.0;
    }
    // tick を挟む直前 (prev) / 直後 (next) のイベントを探す。
    let mut prev: Option<(f64, f64)> = None;
    let mut next: Option<(f64, f64)> = None;
    for &(t, f) in segments {
        if t <= tick {
            prev = Some((t, f));
        } else {
            next = Some((t, f));
            break;
        }
    }
    match (prev, next) {
        (None, _) => 1.0,
        (Some((_, f)), None) => f,
        (Some((t0, f0)), Some((t1, f1))) => {
            let span = t1 - t0;
            if span <= f64::EPSILON {
                return f1;
            }
            let ratio = ((tick - t0) / span).clamp(0.0, 1.0);
            f0 + (f1 - f0) * ratio
        }
    }
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

fn display_fast_slow_counts(session: &GameSession) -> bmz_render::snapshot::FastSlowJudgeCounts {
    let judges = &session.score.judges;
    bmz_render::snapshot::FastSlowJudgeCounts {
        fast_pgreat: judges.fast_pgreat,
        slow_pgreat: judges.slow_pgreat,
        fast_great: judges.fast_great,
        slow_great: judges.slow_great,
        fast_good: judges.fast_good,
        slow_good: judges.slow_good,
        fast_bad: judges.fast_bad,
        slow_bad: judges.slow_bad,
        fast_poor: judges.fast_poor,
        slow_poor: judges.slow_poor,
        fast_empty_poor: judges.fast_empty_poor,
        slow_empty_poor: judges.slow_empty_poor,
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
    fn bga_texture_ids_do_not_overlap_beatoraja_skin_ranges() {
        // skin_loader::SkinKind::first_texture_id と同じ割当。
        const SELECT_SKIN_BASE: u32 = 20_000;
        const RESULT_SKIN_BASE: u32 = 30_000;
        // result スキンが数千 PNG あっても BGA 帯に届かないこと。
        const MAX_RESULT_SKIN_TEXTURES: u32 = 10_000;

        assert!(CHART_BGA_TEXTURE_BASE >= RESULT_SKIN_BASE + MAX_RESULT_SKIN_TEXTURES);
        assert!(CHART_BGA_TEXTURE_BASE > SELECT_SKIN_BASE);
        assert_eq!(bga_texture_id(BgaAssetId(0)), CHART_BGA_TEXTURE_BASE);
    }

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
    fn build_render_snapshot_keeps_notes_under_lane_cover() {
        let profile = ProfileConfig::new_default("default", "Default", 1);
        let mut session =
            build_game_session(Arc::new(chart()), &profile, PlaySessionOptions::default());
        session.hispeed = 1.0;
        // Key1 のノートは render_now=0 で progress 0.5 (time 1_000_000 / lookahead 2_000_000)

        session.lane_cover = 0.3;
        let visible = build_render_snapshot(&session, TimeUs(0), &[], None);
        assert_eq!(visible.visible_notes[Lane::Key1.index()].len(), 1);

        // lane cover は描画で隠すだけなので、カバー域に入る progress でも snapshot には残す。
        session.lane_cover = 0.6;
        let covered = build_render_snapshot(&session, TimeUs(0), &[], None);
        assert_eq!(covered.visible_notes[Lane::Key1.index()].len(), 1);
        assert_eq!(covered.visible_notes[Lane::Key1.index()][0].y, 0.5);
    }

    #[test]
    fn build_render_snapshot_scroll_speed_tracks_bpm_change() {
        use bmz_chart::model::{TimingEvent, TimingEventKind};
        use bmz_chart::timing::TICKS_PER_BEAT;

        // 120 BPM の譜面で 4 拍経過時点(500ms)に 240 BPM へ変化。
        // ノートを変化点直後の 1 拍先 (= さらに 250ms) に置く。
        // hispeed=1.0, lookahead=2s, base BPM=120 → lookahead は 4 拍ぶん。
        // 240 BPM 区間では実時間で半分の速さでスクロールに見えるはずで、
        // ノートは「1 / 4 拍 = 0.25」の位置に来る。
        let mut c = chart();
        c.metadata.initial_bpm = 120.0;
        c.timing_events = vec![TimingEvent {
            tick: ChartTick(TICKS_PER_BEAT as u64 * 4),
            time: TimeUs(2_000_000),
            kind: TimingEventKind::BpmChange { bpm: 240.0 },
        }];
        // ノートを 4 拍 + 1 拍 = 5 拍位置に置く。
        // 0..4 拍 = 2s @ 120BPM, 4..5 拍 = 0.25s @ 240BPM → time = 2_250_000us
        c.lane_notes[Lane::Key1.index()][0].tick = ChartTick(TICKS_PER_BEAT as u64 * 5);
        c.lane_notes[Lane::Key1.index()][0].time = TimeUs(2_250_000);

        let profile = ProfileConfig::new_default("default", "Default", 1);
        let mut session = build_game_session(Arc::new(c), &profile, PlaySessionOptions::default());
        session.hispeed = 1.0;

        // render_now=2_000_000 (BPM 変化点ちょうど): ノートは 1 拍先 = 0.25 にいる。
        let snap = build_render_snapshot(&session, TimeUs(2_000_000), &[], None);
        let y = snap.visible_notes[Lane::Key1.index()][0].y;
        assert!((y - 0.25).abs() < 1e-3, "expected ~0.25, got {y}");
    }

    #[test]
    fn build_render_snapshot_scroll_freezes_during_stop() {
        use bmz_chart::model::{TimingEvent, TimingEventKind};
        use bmz_chart::timing::TICKS_PER_BEAT;

        // 120 BPM で 4 拍経過時点 (2s) に 1 秒の STOP。
        // ノートは 5 拍位置 (実時刻 3.5s — 2s + STOP1s + 0.5s)。
        let mut c = chart();
        c.metadata.initial_bpm = 120.0;
        c.timing_events = vec![TimingEvent {
            tick: ChartTick(TICKS_PER_BEAT as u64 * 4),
            time: TimeUs(0),
            kind: TimingEventKind::Stop { duration_us: 1_000_000 },
        }];
        c.lane_notes[Lane::Key1.index()][0].tick = ChartTick(TICKS_PER_BEAT as u64 * 5);
        c.lane_notes[Lane::Key1.index()][0].time = TimeUs(3_500_000);

        let profile = ProfileConfig::new_default("default", "Default", 1);
        let mut session = build_game_session(Arc::new(c), &profile, PlaySessionOptions::default());
        session.hispeed = 1.0;

        // STOP 直前 (just before tick 4 拍): カーソル tick=4, ノート tick=5 → 1 拍差 = 0.25
        let before = build_render_snapshot(&session, TimeUs(1_999_999), &[], None);
        let y_before = before.visible_notes[Lane::Key1.index()][0].y;
        assert!((y_before - 0.25).abs() < 1e-3, "before: expected ~0.25, got {y_before}");

        // STOP 中: カーソル tick が止まり、ノート位置も動かない。
        let mid = build_render_snapshot(&session, TimeUs(2_500_000), &[], None);
        let y_mid = mid.visible_notes[Lane::Key1.index()][0].y;
        assert!((y_mid - 0.25).abs() < 1e-3, "mid stop: expected ~0.25, got {y_mid}");
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
    fn build_render_snapshot_doubles_distance_with_scroll_factor_two() {
        use bmz_chart::model::ScrollEvent;
        let mut chart = chart();
        // tick 0 から factor=2.0 で全区間スクロール倍速。
        chart.scroll_events =
            vec![ScrollEvent { tick: ChartTick(0), time: TimeUs(0), factor: 2.0 }];
        let profile = ProfileConfig::new_default("default", "Default", 1);
        let mut session =
            build_game_session(Arc::new(chart), &profile, PlaySessionOptions::default());
        session.hispeed = 1.0;

        // chart() のノートは TimeUs(1_000_000)、lookahead=2_000_000 で 1/2 進捗。
        // SCROLL 2.0 が乗ると見かけ進捗 1.0 (画面上端) になる。
        let snapshot = build_render_snapshot(&session, TimeUs(0), &[], None);
        let y = snapshot.visible_notes[Lane::Key1.index()][0].y;
        assert!((y - 1.0).abs() < 1e-3, "expected ~1.0 with SCROLL 2.0, got {y}");
    }

    #[test]
    fn speed_at_interpolates_linearly_between_events() {
        let segments = [(0.0, 1.0), (3840.0, 2.0)];
        // 区間内の中央は中間値 1.5。
        assert!((super::speed_at(&segments, 1920.0) - 1.5).abs() < 1e-6);
        // 1/4 地点。
        assert!((super::speed_at(&segments, 960.0) - 1.25).abs() < 1e-6);
        // 境界の値そのもの。
        assert!((super::speed_at(&segments, 0.0) - 1.0).abs() < 1e-6);
        assert!((super::speed_at(&segments, 3840.0) - 2.0).abs() < 1e-6);
        // 最後のイベント以降はその factor で固定 (補間されない)。
        assert!((super::speed_at(&segments, 5000.0) - 2.0).abs() < 1e-6);
    }

    #[test]
    fn speed_at_returns_one_before_first_event() {
        let segments = [(1000.0, 2.0)];
        assert!((super::speed_at(&segments, 500.0) - 1.0).abs() < 1e-6);
        assert!((super::speed_at(&segments, 1000.0) - 2.0).abs() < 1e-6);
        assert!((super::speed_at(&segments, 2000.0) - 2.0).abs() < 1e-6);
    }

    #[test]
    fn build_render_snapshot_applies_speed_factor() {
        use bmz_chart::model::SpeedEvent;
        let mut chart = chart();
        chart.speed_events = vec![SpeedEvent { tick: ChartTick(0), time: TimeUs(0), factor: 2.0 }];
        let profile = ProfileConfig::new_default("default", "Default", 1);
        let mut session =
            build_game_session(Arc::new(chart), &profile, PlaySessionOptions::default());
        session.hispeed = 1.0;

        let snapshot = build_render_snapshot(&session, TimeUs(0), &[], None);
        let y = snapshot.visible_notes[Lane::Key1.index()][0].y;
        assert!((y - 1.0).abs() < 1e-3, "expected ~1.0 with SPEED 2.0, got {y}");
    }

    #[test]
    fn build_render_snapshot_interpolates_speed_between_events() {
        use bmz_chart::model::SpeedEvent;
        let mut chart = chart();
        // BPM 120 / 4 拍 = 3840 ticks。SPEED を tick=0..3840 で 1.0→3.0 へ補間。
        // chart() のノートは TimeUs(1_000_000) = 1920 ticks (中央) なので、
        // 補間値は 2.0 になる。base 進捗 0.5 × SPEED 2.0 = 1.0 (画面上端)。
        chart.speed_events = vec![
            SpeedEvent { tick: ChartTick(0), time: TimeUs(0), factor: 1.0 },
            SpeedEvent { tick: ChartTick(3840), time: TimeUs(2_000_000), factor: 3.0 },
        ];
        let profile = ProfileConfig::new_default("default", "Default", 1);
        let mut session =
            build_game_session(Arc::new(chart), &profile, PlaySessionOptions::default());
        session.hispeed = 1.0;

        let snapshot = build_render_snapshot(&session, TimeUs(0), &[], None);
        let y = snapshot.visible_notes[Lane::Key1.index()][0].y;
        assert!(
            (y - 1.0).abs() < 1e-3,
            "expected ~1.0 from linear interpolation (0.5 base × 2.0 mid speed), got {y}"
        );
    }

    #[test]
    fn build_render_snapshot_compresses_distance_with_scroll_factor_half() {
        use bmz_chart::model::ScrollEvent;
        let mut chart = chart();
        chart.scroll_events =
            vec![ScrollEvent { tick: ChartTick(0), time: TimeUs(0), factor: 0.5 }];
        let profile = ProfileConfig::new_default("default", "Default", 1);
        let mut session =
            build_game_session(Arc::new(chart), &profile, PlaySessionOptions::default());
        session.hispeed = 1.0;

        // 1/2 進捗 × SCROLL 0.5 = 1/4 進捗。
        let snapshot = build_render_snapshot(&session, TimeUs(0), &[], None);
        let y = snapshot.visible_notes[Lane::Key1.index()][0].y;
        assert!((y - 0.25).abs() < 1e-3, "expected ~0.25 with SCROLL 0.5, got {y}");
    }

    #[test]
    fn build_render_snapshot_hides_note_with_negative_scroll() {
        use bmz_chart::model::ScrollEvent;
        let mut chart = chart();
        // factor < 0 は逆スクロール。delta が負になり描画対象外。
        chart.scroll_events =
            vec![ScrollEvent { tick: ChartTick(0), time: TimeUs(0), factor: -1.0 }];
        let profile = ProfileConfig::new_default("default", "Default", 1);
        let mut session =
            build_game_session(Arc::new(chart), &profile, PlaySessionOptions::default());
        session.hispeed = 1.0;

        let snapshot = build_render_snapshot(&session, TimeUs(0), &[], None);
        assert!(snapshot.visible_notes[Lane::Key1.index()].is_empty());
    }

    #[test]
    fn build_render_snapshot_reports_lane_cover_changing_and_note_display_duration() {
        let profile = ProfileConfig::new_default("default", "Default", 1);
        let mut session =
            build_game_session(Arc::new(chart()), &profile, PlaySessionOptions::default());
        session.hispeed = 2.0;
        session.lane_cover = 0.25;
        session.lane_cover_changing = true;

        let snapshot = build_render_snapshot(&session, TimeUs(0), &[], None);

        assert!(snapshot.lane_cover_changing);
        assert_eq!(snapshot.note_display_duration_ms, 750);
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
    fn build_render_snapshot_routes_invisible_and_mine_correctly() {
        let mut chart = chart();
        chart.lane_notes[Lane::Key2.index()].push(NoteEvent {
            id: NoteId(2),
            lane: Lane::Key2,
            kind: NoteKind::Invisible,
            tick: ChartTick(0),
            time: TimeUs(1_000_000),
            sound: None,
            damage: None,
        });
        chart.lane_notes[Lane::Key3.index()].push(NoteEvent {
            id: NoteId(3),
            lane: Lane::Key3,
            kind: NoteKind::Mine,
            tick: ChartTick(0),
            time: TimeUs(1_000_000),
            sound: None,
            damage: Some(8),
        });
        let profile = ProfileConfig::new_default("default", "Default", 1);
        let session = build_game_session(Arc::new(chart), &profile, PlaySessionOptions::default());

        let snapshot = build_render_snapshot(&session, TimeUs(0), &[], None);

        assert_eq!(snapshot.visible_notes[Lane::Key1.index()].len(), 1);
        assert!(snapshot.visible_notes[Lane::Key2.index()].is_empty());
        assert!(snapshot.visible_notes[Lane::Key3.index()].is_empty());
        // Mine は visible_mines 側に振り分けられる。damage も保持。
        assert_eq!(snapshot.visible_mines[Lane::Key3.index()].len(), 1);
        assert_eq!(snapshot.visible_mines[Lane::Key3.index()][0].damage, 8);
        assert!(snapshot.visible_mines[Lane::Key1.index()].is_empty());
        assert!(snapshot.visible_mines[Lane::Key2.index()].is_empty());
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
        assert_eq!(snapshot.fast_slow_counts.fast_pgreat, 1);
        assert_eq!(snapshot.fast_slow_counts.slow_empty_poor, 1);
    }

    #[test]
    fn build_render_snapshot_passes_judge_rank() {
        let profile = ProfileConfig::new_default("default", "Default", 1);
        let mut chart = chart();
        chart.metadata.judge_rank = Some(0);
        let session = build_game_session(Arc::new(chart), &profile, PlaySessionOptions::default());

        let snapshot = build_render_snapshot(&session, TimeUs(0), &[], None);

        assert_eq!(snapshot.judge_rank, Some(0));
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
            scroll_events: Vec::new(),
            speed_events: Vec::new(),
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
            damage: None,
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

            scroll_events: Vec::new(),

            speed_events: Vec::new(),
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
            damage: None,
        };
        let end = NoteEvent {
            id: NoteId(2),
            lane: Lane::Key1,
            kind: NoteKind::LongEnd,
            tick: ChartTick(0),
            time: TimeUs(1_500_000),
            sound: None,
            damage: None,
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

            scroll_events: Vec::new(),

            speed_events: Vec::new(),
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

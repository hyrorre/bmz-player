//! beatoraja `BMSModelUtils.setStartNoteTime` 相当の先頭ノーツ余白。

use bmz_core::time::{ChartTick, TimeUs};

use crate::model::{NoteKind, PlayableChart};
use crate::timing::us_to_ticks;

/// beatoraja と同じく、先頭判定ノーツを少なくともこの時刻へ押し出す (ms)。
pub const START_NOTE_MARGIN_MS: i64 = 1_000;

/// 先頭の判定対象ノーツが [`START_NOTE_MARGIN_MS`] 未満なら、譜面全体を後ろへずらす。
///
/// beatoraja `setStartNoteTime(model, 1000)` 相当:
/// - トリガー: `Tap` / `LongStart` / `LongEnd` / `Mine`（`Invisible` は除外）
/// - 全イベントの `tick` / `time` を同一量だけ加算
/// - 適用した余白 (µs) を返す。不要なら `TimeUs(0)`
pub fn apply_start_note_margin(chart: &mut PlayableChart) -> TimeUs {
    apply_start_note_margin_ms(chart, START_NOTE_MARGIN_MS)
}

/// [`apply_start_note_margin`] の閾値指定版 (テスト用)。
pub fn apply_start_note_margin_ms(chart: &mut PlayableChart, margin_ms: i64) -> TimeUs {
    let margin_target_us = margin_ms.saturating_mul(1_000).max(0);
    let Some(first_note_us) = first_trigger_note_time_us(chart) else {
        return TimeUs(0);
    };
    if first_note_us >= margin_target_us {
        return TimeUs(0);
    }

    let margin_us = margin_target_us - first_note_us;
    let margin_ticks = us_to_ticks(margin_us, chart.metadata.initial_bpm);
    shift_chart(chart, margin_ticks, margin_us);
    ensure_leading_bar_line(chart);
    chart.end_time = TimeUs(chart.end_time.0.saturating_add(margin_us));
    TimeUs(margin_us)
}

fn first_trigger_note_time_us(chart: &PlayableChart) -> Option<i64> {
    chart
        .lane_notes
        .iter()
        .flat_map(|notes| notes.iter())
        .filter(|note| is_trigger_kind(note.kind))
        .map(|note| note.time.0)
        .min()
}

fn is_trigger_kind(kind: NoteKind) -> bool {
    matches!(kind, NoteKind::Tap | NoteKind::LongStart | NoteKind::LongEnd | NoteKind::Mine)
}

fn shift_chart(chart: &mut PlayableChart, margin_ticks: u64, margin_us: i64) {
    for lane_notes in &mut chart.lane_notes {
        for note in lane_notes.iter_mut() {
            note.tick = ChartTick(note.tick.0.saturating_add(margin_ticks));
            note.time = TimeUs(note.time.0.saturating_add(margin_us));
        }
    }

    for pair in &mut chart.long_notes {
        pair.start_tick = ChartTick(pair.start_tick.0.saturating_add(margin_ticks));
        pair.end_tick = ChartTick(pair.end_tick.0.saturating_add(margin_ticks));
        pair.start_time = TimeUs(pair.start_time.0.saturating_add(margin_us));
        pair.end_time = TimeUs(pair.end_time.0.saturating_add(margin_us));
    }

    for event in &mut chart.bgm_events {
        event.tick = ChartTick(event.tick.0.saturating_add(margin_ticks));
        event.time = TimeUs(event.time.0.saturating_add(margin_us));
    }
    for event in &mut chart.bga_events {
        event.tick = ChartTick(event.tick.0.saturating_add(margin_ticks));
        event.time = TimeUs(event.time.0.saturating_add(margin_us));
    }
    for event in &mut chart.timing_events {
        event.tick = ChartTick(event.tick.0.saturating_add(margin_ticks));
        event.time = TimeUs(event.time.0.saturating_add(margin_us));
    }
    for event in &mut chart.scroll_events {
        event.tick = ChartTick(event.tick.0.saturating_add(margin_ticks));
        event.time = TimeUs(event.time.0.saturating_add(margin_us));
    }
    for event in &mut chart.speed_events {
        event.tick = ChartTick(event.tick.0.saturating_add(margin_ticks));
        event.time = TimeUs(event.time.0.saturating_add(margin_us));
    }
    for event in &mut chart.judge_rank_events {
        event.tick = ChartTick(event.tick.0.saturating_add(margin_ticks));
        event.time = TimeUs(event.time.0.saturating_add(margin_us));
    }
    for event in &mut chart.bgm_volume_events {
        event.tick = ChartTick(event.tick.0.saturating_add(margin_ticks));
        event.time = TimeUs(event.time.0.saturating_add(margin_us));
    }
    for event in &mut chart.key_volume_events {
        event.tick = ChartTick(event.tick.0.saturating_add(margin_ticks));
        event.time = TimeUs(event.time.0.saturating_add(margin_us));
    }
    for event in &mut chart.text_events {
        event.tick = ChartTick(event.tick.0.saturating_add(margin_ticks));
        event.time = TimeUs(event.time.0.saturating_add(margin_us));
    }
    for event in &mut chart.bga_opacity_events {
        event.tick = ChartTick(event.tick.0.saturating_add(margin_ticks));
        event.time = TimeUs(event.time.0.saturating_add(margin_us));
    }
    for event in &mut chart.bga_argb_events {
        event.tick = ChartTick(event.tick.0.saturating_add(margin_ticks));
        event.time = TimeUs(event.time.0.saturating_add(margin_us));
    }
    for event in &mut chart.bga_keybound_events {
        event.tick = ChartTick(event.tick.0.saturating_add(margin_ticks));
        event.time = TimeUs(event.time.0.saturating_add(margin_us));
    }
    for bar in &mut chart.bar_lines {
        bar.tick = ChartTick(bar.tick.0.saturating_add(margin_ticks));
        bar.time = TimeUs(bar.time.0.saturating_add(margin_us));
        // beatoraja は先頭に空 TimeLine を挿む。小節番号も 1 つずらす。
        bar.measure = bar.measure.saturating_add(1);
    }
}

/// シフト後に measure 0 / tick 0 の小節線が無ければ挿入する。
fn ensure_leading_bar_line(chart: &mut PlayableChart) {
    let has_origin = chart.bar_lines.iter().any(|bar| bar.tick.0 == 0 && bar.time.0 == 0);
    if has_origin {
        return;
    }
    chart
        .bar_lines
        .insert(0, crate::model::BarLine { measure: 0, tick: ChartTick(0), time: TimeUs(0) });
}

/// beatoraja の `marginSection = marginTime * BPM / 240000` を tick 換算した値。
///
/// テストとドキュメント用。実装は [`us_to_ticks`] に委譲する。
pub fn margin_ticks_for(margin_us: i64, bpm: f64) -> u64 {
    us_to_ticks(margin_us, bpm)
}

#[cfg(test)]
mod tests {
    use bmz_core::chart::ChartIdentity;
    use bmz_core::ids::{NoteId, SoundId};
    use bmz_core::lane::Lane;
    use bmz_core::time::{ChartTick, TimeUs};

    use super::*;
    use crate::model::{
        BarLine, ChartMetadata, NoteEvent, NoteKind, PlayableChart, SoundEvent, TimingEvent,
        TimingEventKind,
    };
    use crate::timing::TICKS_PER_MEASURE;

    fn empty_chart(bpm: f64) -> PlayableChart {
        PlayableChart {
            identity: ChartIdentity { file_md5: [0; 16], file_sha256: [0; 32] },
            metadata: ChartMetadata { initial_bpm: bpm, ..ChartMetadata::default() },
            lane_notes: Default::default(),
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
            bar_lines: vec![BarLine { measure: 0, tick: ChartTick(0), time: TimeUs(0) }],
            sounds: Vec::new(),
            bga_assets: Vec::new(),
            total_notes: 0,
            end_time: TimeUs(0),
        }
    }

    fn tap(id: u32, lane: Lane, time_us: i64, tick: u64) -> NoteEvent {
        NoteEvent {
            id: NoteId(id),
            lane,
            kind: NoteKind::Tap,
            tick: ChartTick(tick),
            time: TimeUs(time_us),
            sound: None,
            damage: None,
        }
    }

    #[test]
    fn no_notes_is_noop() {
        let mut chart = empty_chart(150.0);
        assert_eq!(apply_start_note_margin(&mut chart), TimeUs(0));
        assert_eq!(chart.bar_lines[0].measure, 0);
    }

    #[test]
    fn first_note_at_or_after_margin_is_noop() {
        let mut chart = empty_chart(150.0);
        chart.lane_notes[Lane::Key1.index()].push(tap(1, Lane::Key1, 1_000_000, 2400));
        chart.total_notes = 1;
        chart.end_time = TimeUs(1_000_000);
        assert_eq!(apply_start_note_margin(&mut chart), TimeUs(0));
        assert_eq!(chart.lane_notes[Lane::Key1.index()][0].time.0, 1_000_000);
    }

    #[test]
    fn note_at_zero_shifts_by_full_margin() {
        let mut chart = empty_chart(150.0);
        chart.lane_notes[Lane::Scratch.index()].push(tap(1, Lane::Scratch, 0, 0));
        chart.bgm_events.push(SoundEvent {
            tick: ChartTick(7680),
            time: TimeUs(3_200_000),
            sound: SoundId(1),
        });
        chart.total_notes = 1;
        chart.end_time = TimeUs(3_200_000);

        let margin = apply_start_note_margin(&mut chart);
        assert_eq!(margin, TimeUs(1_000_000));
        assert_eq!(chart.lane_notes[Lane::Scratch.index()][0].time.0, 1_000_000);
        assert_eq!(chart.bgm_events[0].time.0, 4_200_000);
        assert_eq!(chart.end_time.0, 4_200_000);
        // bar line 0 が先頭にあり、元の measure 0 は 1 へ
        assert_eq!(chart.bar_lines[0].measure, 0);
        assert_eq!(chart.bar_lines[0].tick.0, 0);
        assert_eq!(chart.bar_lines[1].measure, 1);
        let expected_ticks = us_to_ticks(1_000_000, 150.0);
        assert_eq!(chart.lane_notes[Lane::Scratch.index()][0].tick.0, expected_ticks);
        assert_eq!(chart.bgm_events[0].tick.0, 7680 + expected_ticks);
    }

    #[test]
    fn note_at_500ms_shifts_by_remaining_margin() {
        let mut chart = empty_chart(120.0);
        chart.lane_notes[Lane::Key1.index()].push(tap(1, Lane::Key1, 500_000, 960));
        chart.total_notes = 1;
        chart.end_time = TimeUs(500_000);

        let margin = apply_start_note_margin(&mut chart);
        assert_eq!(margin, TimeUs(500_000));
        assert_eq!(chart.lane_notes[Lane::Key1.index()][0].time.0, 1_000_000);
    }

    #[test]
    fn invisible_alone_does_not_trigger() {
        let mut chart = empty_chart(150.0);
        chart.lane_notes[Lane::Key1.index()].push(NoteEvent {
            id: NoteId(1),
            lane: Lane::Key1,
            kind: NoteKind::Invisible,
            tick: ChartTick(0),
            time: TimeUs(0),
            sound: None,
            damage: None,
        });
        chart.lane_notes[Lane::Key2.index()].push(tap(2, Lane::Key2, 2_000_000, 4800));
        chart.total_notes = 1;
        assert_eq!(apply_start_note_margin(&mut chart), TimeUs(0));
        assert_eq!(chart.lane_notes[Lane::Key2.index()][0].time.0, 2_000_000);
    }

    #[test]
    fn mine_at_zero_triggers_margin() {
        let mut chart = empty_chart(150.0);
        chart.lane_notes[Lane::Key1.index()].push(NoteEvent {
            id: NoteId(1),
            lane: Lane::Key1,
            kind: NoteKind::Mine,
            tick: ChartTick(0),
            time: TimeUs(0),
            sound: None,
            damage: Some(10),
        });
        chart.end_time = TimeUs(0);
        assert_eq!(apply_start_note_margin(&mut chart), TimeUs(1_000_000));
        assert_eq!(chart.lane_notes[Lane::Key1.index()][0].time.0, 1_000_000);
    }

    #[test]
    fn timing_events_shift_with_notes() {
        let mut chart = empty_chart(150.0);
        chart.lane_notes[Lane::Key1.index()].push(tap(1, Lane::Key1, 0, 0));
        chart.timing_events.push(TimingEvent {
            tick: ChartTick(3840),
            time: TimeUs(1_600_000),
            kind: TimingEventKind::BpmChange { bpm: 180.0 },
        });
        chart.end_time = TimeUs(1_600_000);

        apply_start_note_margin(&mut chart);
        let expected_ticks = us_to_ticks(1_000_000, 150.0);
        assert_eq!(chart.timing_events[0].time.0, 2_600_000);
        assert_eq!(chart.timing_events[0].tick.0, 3840 + expected_ticks);
    }

    #[test]
    fn margin_ticks_matches_beatoraja_section_formula() {
        // marginSection = marginTime_ms * BPM / 240000
        // ticks = marginSection * TICKS_PER_MEASURE
        let margin_us = 1_000_000i64;
        let bpm = 150.0;
        let section = (margin_us as f64 / 1000.0) * bpm / 240_000.0;
        let expected = (section * f64::from(TICKS_PER_MEASURE)).floor() as u64;
        assert_eq!(margin_ticks_for(margin_us, bpm), expected);
    }
}

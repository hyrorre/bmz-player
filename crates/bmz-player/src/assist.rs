//! beatoraja `PatternModifier` / assist button 301..307 compatibility.

use std::collections::{BTreeMap, BTreeSet};

use bmz_chart::model::{
    LongNoteMode, LongNotePair, LongNoteStyle, NoteEvent, NoteKind, PlayableChart, ScrollEvent,
    SoundEvent, TimingEventKind,
};
use bmz_chart::timing::TimingMap;
use bmz_core::ids::NoteId;
use bmz_core::lane::Lane;
use bmz_core::time::{ChartTick, TimeUs};
use bmz_gameplay::judge::model::{JudgeWindow, JudgeWindows};
use bmz_gameplay::session::{AssistLevel, AssistRuntime};

use crate::config::profile_config::{
    AssistLongNoteMode, AssistMineMode, AssistOptionConfig, AssistScrollMode,
};
use crate::random_option_seed::JavaRandom;
use crate::select_options::ArrangeOption;

pub const EXPAND_JUDGE_MASK: u32 = 1 << 0;
pub const CONSTANT_MASK: u32 = 1 << 1;
pub const JUDGE_AREA_MASK: u32 = 1 << 2;
pub const LEGACY_NOTE_MASK: u32 = 1 << 3;
pub const MARK_NOTE_MASK: u32 = 1 << 4;
pub const BPM_GUIDE_MASK: u32 = 1 << 5;
pub const NO_MINE_MASK: u32 = 1 << 6;
pub const EXTRA_NOTE_MASK: u32 = 1 << 7;

pub fn configured_mask(config: AssistOptionConfig) -> u32 {
    config.flags().into_iter().enumerate().fold(
        0,
        |mask, (index, enabled)| {
            if enabled { mask | (1 << index) } else { mask }
        },
    ) | if config.extra_note_depth > 0 { EXTRA_NOTE_MASK } else { 0 }
}

pub fn mask_flags(mask: u32) -> [bool; 7] {
    std::array::from_fn(|index| mask & (1 << index) != 0)
}

/// beatoraja で Light Assist 扱いになる配置を、実効アシストへ合成する。
///
/// 配置は assist button 301..307 の bit mask には含めず、ランプ・score/replay/IR の
/// 保存可否に使う level だけを更新する。scratch のない mode などで NORMAL へ
/// fallback した場合を除外できるよう、呼び出し側は適用後の配置を渡す。
pub fn merge_arrange_assist_level(
    runtime: &mut AssistRuntime,
    arrange_1p: ArrangeOption,
    arrange_2p: ArrangeOption,
) {
    if [arrange_1p, arrange_2p].into_iter().any(|arrange| {
        matches!(
            arrange,
            ArrangeOption::Spiral
                | ArrangeOption::HRandom
                | ArrangeOption::AllScratch
                | ArrangeOption::RandomEx
                | ArrangeOption::SRandomEx
        )
    }) {
        runtime.level = runtime.level.max(AssistLevel::LightAssist);
    }
}

/// AUTO SCRATCH を実効アシストへ合成する。
///
/// LR2oraja Endless Dream の `AutoplayModifier` と同じく、自動演奏するレーンに
/// 採点対象ノーツが無ければアシスト扱いにしない。呼び出し側は全譜面オプション
/// 適用後の譜面と、プレイヤーが判定されるスクラッチレーンを渡す。
pub fn merge_auto_scratch_assist_level(
    runtime: &mut AssistRuntime,
    chart: &PlayableChart,
    scratch_lanes: &[Lane],
) {
    let has_scratch_notes = scratch_lanes.iter().any(|lane| {
        chart.lane_notes.get(lane.index()).is_some_and(|notes| {
            notes.iter().any(|note| matches!(note.kind, NoteKind::Tap | NoteKind::LongStart))
        })
    });
    if has_scratch_notes {
        runtime.level = runtime.level.max(AssistLevel::Assist);
    }
}

/// 譜面依存の有効性を判定しながら、beatoraja と同じ modifier 順で適用する。
///
/// 呼び出し位置は LN policy の後、DOUBLE / lane arrange の前とする。
pub fn apply_chart_assists(
    chart: &mut PlayableChart,
    config: AssistOptionConfig,
    seed: i64,
) -> AssistRuntime {
    let configured_mask = configured_mask(config);
    let variable_bpm = chart.timing_events.iter().any(|event| {
        matches!(event.kind, TimingEventKind::BpmChange { bpm } if (bpm - chart.metadata.initial_bpm).abs() > f64::EPSILON)
    });
    let mut runtime = AssistRuntime {
        configured_mask,
        effective_mask: 0,
        level: AssistLevel::None,
        judge_area: config.judge_area,
        mark_note: config.mark_note,
        bpm_guide: config.bpm_guide,
        extra_note_depth: config.extra_note_depth,
        mine_mode: config.mine_mode as i64,
        scroll_mode: config.scroll_mode as i64,
        long_note_mode: config.long_note_mode as i64,
    };
    let mut rng = JavaRandom::new(seed);

    if config.expand_judge
        && [
            config.key_pgreat_rate,
            config.key_great_rate,
            config.key_good_rate,
            config.scratch_pgreat_rate,
            config.scratch_great_rate,
            config.scratch_good_rate,
            config.long_note_margin_rate,
        ]
        .into_iter()
        .any(|rate| rate > 100)
    {
        mark_effect(&mut runtime, EXPAND_JUDGE_MASK, AssistLevel::Assist);
    }

    match config.scroll_mode {
        AssistScrollMode::Off => {}
        AssistScrollMode::Remove => {
            if remove_scroll_changes(chart) {
                mark_effect(&mut runtime, CONSTANT_MASK, AssistLevel::LightAssist);
            }
        }
        AssistScrollMode::Add => add_scroll_changes(chart, config, &mut rng),
    }

    match config.long_note_mode {
        AssistLongNoteMode::Off => {}
        AssistLongNoteMode::Remove => {
            if remove_long_notes(chart, config.long_note_rate, &mut rng) {
                mark_effect(&mut runtime, LEGACY_NOTE_MASK, AssistLevel::Assist);
            }
        }
        mode => {
            let (_, assist_added) = add_long_notes(chart, mode, config.long_note_rate, &mut rng);
            if assist_added {
                mark_effect(&mut runtime, LEGACY_NOTE_MASK, AssistLevel::Assist);
            }
        }
    }

    match config.mine_mode {
        AssistMineMode::Off => {}
        AssistMineMode::Remove => {
            if remove_mines(chart) {
                mark_effect(&mut runtime, NO_MINE_MASK, AssistLevel::LightAssist);
            }
        }
        mode => add_mines(chart, mode, &mut rng),
    }

    if config.extra_note_depth > 0
        && add_extra_notes(chart, config.extra_note_depth, config.extra_note_scratch)
    {
        mark_effect(&mut runtime, EXTRA_NOTE_MASK, AssistLevel::Assist);
    }

    if config.bpm_guide && variable_bpm {
        mark_effect(&mut runtime, BPM_GUIDE_MASK, AssistLevel::LightAssist);
    }

    chart.total_notes = chart
        .lane_notes
        .iter()
        .flatten()
        .filter(|note| matches!(note.kind, NoteKind::Tap | NoteKind::LongStart))
        .count()
        .min(u32::MAX as usize) as u32;
    runtime
}

pub fn apply_custom_judge_windows(
    windows: JudgeWindows,
    config: AssistOptionConfig,
) -> JudgeWindows {
    if !config.expand_judge {
        return windows;
    }
    JudgeWindows {
        note: scale_window(
            windows.note,
            config.key_pgreat_rate,
            config.key_great_rate,
            config.key_good_rate,
        ),
        scratch: scale_window(
            windows.scratch,
            config.scratch_pgreat_rate,
            config.scratch_great_rate,
            config.scratch_good_rate,
        ),
        long_note_end: scale_window(
            windows.long_note_end,
            config.key_pgreat_rate,
            config.key_great_rate,
            config.key_good_rate,
        ),
        long_scratch_end: scale_window(
            windows.long_scratch_end,
            config.scratch_pgreat_rate,
            config.scratch_great_rate,
            config.scratch_good_rate,
        ),
        long_note_release_margin_us: scale_us(
            windows.long_note_release_margin_us,
            config.long_note_margin_rate,
        ),
        long_scratch_release_margin_us: windows.long_scratch_release_margin_us,
    }
}

fn scale_window(window: JudgeWindow, pgreat: u16, great: u16, good: u16) -> JudgeWindow {
    let bad_cap = window.bad_fast_us.min(window.bad_slow_us);
    let pgreat_us = scale_us(window.pgreat_us, pgreat).min(bad_cap);
    let great_us = scale_us(window.great_us, great).clamp(pgreat_us, bad_cap);
    let good_us = scale_us(window.good_us, good).clamp(great_us, bad_cap);
    JudgeWindow { pgreat_us, great_us, good_us, ..window }
}

fn scale_us(value: i64, rate: u16) -> i64 {
    value.saturating_mul(i64::from(rate)) / 100
}

fn mark_effect(runtime: &mut AssistRuntime, mask: u32, level: AssistLevel) {
    runtime.effective_mask |= mask;
    runtime.level = runtime.level.max(level);
}

fn remove_scroll_changes(chart: &mut PlayableChart) -> bool {
    let start_scroll = initial_scroll_factor(chart);
    let changed_timing = chart.timing_events.iter().any(|event| match event.kind {
        TimingEventKind::BpmChange { bpm } => {
            (bpm - chart.metadata.initial_bpm).abs() > f64::EPSILON
        }
        TimingEventKind::Stop { duration_us } => duration_us != 0,
    });
    let changed_scroll =
        chart.scroll_events.iter().any(|event| (event.factor - start_scroll).abs() > f64::EPSILON);
    chart.timing_events.clear();
    chart.scroll_events.clear();
    if (start_scroll - 1.0).abs() > f64::EPSILON {
        chart.scroll_events.push(ScrollEvent {
            tick: ChartTick(0),
            time: TimeUs(0),
            factor: start_scroll,
        });
    }
    retime_chart_to_constant_bpm(chart);
    changed_timing || changed_scroll
}

fn add_scroll_changes(chart: &mut PlayableChart, config: AssistOptionConfig, rng: &mut JavaRandom) {
    let section = usize::from(config.scroll_section.max(1));
    let base = initial_scroll_factor(chart);
    let mut events = Vec::new();
    for (index, line) in chart.bar_lines.iter().enumerate() {
        if (index + 1) % section == 0 {
            let unit = f64::from(rng.next_int_bound(1_000_001)) / 1_000_000.0;
            let factor = base * (1.0 + (unit * 2.0 - 1.0) * config.scroll_rate);
            events.push(ScrollEvent { tick: line.tick, time: line.time, factor });
        }
    }
    if events.is_empty() {
        return;
    }
    chart.scroll_events.clear();
    chart.scroll_events.push(ScrollEvent { tick: ChartTick(0), time: TimeUs(0), factor: base });
    chart.scroll_events.extend(events);
}

fn initial_scroll_factor(chart: &PlayableChart) -> f64 {
    chart
        .scroll_events
        .iter()
        .take_while(|event| event.time.0 <= 0)
        .last()
        .map_or(1.0, |event| event.factor)
}

/// beatoraja の CONSTANT は各 timeline の section も開始 BPM と実時刻から
/// 再計算する。公開 chart model では同じ効果になるよう、時刻を持つ全要素の tick を
/// 固定 BPM の TimingMap で張り直す。
fn retime_chart_to_constant_bpm(chart: &mut PlayableChart) {
    let timing = TimingMap::from_chart_timing_events(chart.metadata.initial_bpm, &[]);
    let tick_at = |time| timing.time_to_tick(time);
    for note in chart.lane_notes.iter_mut().flatten() {
        note.tick = tick_at(note.time);
    }
    for pair in &mut chart.long_notes {
        pair.start_tick = tick_at(pair.start_time);
        pair.end_tick = tick_at(pair.end_time);
    }
    for event in &mut chart.bgm_events {
        event.tick = tick_at(event.time);
    }
    for event in &mut chart.bga_events {
        event.tick = tick_at(event.time);
    }
    for event in &mut chart.scroll_events {
        event.tick = tick_at(event.time);
    }
    for event in &mut chart.speed_events {
        event.tick = tick_at(event.time);
    }
    for event in &mut chart.judge_rank_events {
        event.tick = tick_at(event.time);
    }
    for event in chart.bgm_volume_events.iter_mut().chain(&mut chart.key_volume_events) {
        event.tick = tick_at(event.time);
    }
    for event in &mut chart.text_events {
        event.tick = tick_at(event.time);
    }
    for event in &mut chart.bga_opacity_events {
        event.tick = tick_at(event.time);
    }
    for event in &mut chart.bga_argb_events {
        event.tick = tick_at(event.time);
    }
    for event in &mut chart.bga_keybound_events {
        event.tick = tick_at(event.time);
    }
    for line in &mut chart.bar_lines {
        line.tick = tick_at(line.time);
    }
}

fn remove_long_notes(chart: &mut PlayableChart, rate: f64, rng: &mut JavaRandom) -> bool {
    let mut removed = BTreeSet::new();
    let mut starts = BTreeSet::new();
    chart.long_notes.retain(|pair| {
        if random_unit(rng) < rate {
            starts.insert(pair.start_note_id);
            removed.insert(pair.end_note_id);
            false
        } else {
            true
        }
    });
    if removed.is_empty() {
        return false;
    }
    for notes in &mut chart.lane_notes {
        notes.retain(|note| !removed.contains(&note.id));
        for note in notes {
            if starts.contains(&note.id) {
                note.kind = NoteKind::Tap;
            }
        }
    }
    true
}

fn add_long_notes(
    chart: &mut PlayableChart,
    mode: AssistLongNoteMode,
    rate: f64,
    rng: &mut JavaRandom,
) -> (bool, bool) {
    let timeline = chart_timeline(chart);
    if timeline.len() < 2 {
        return (false, false);
    }
    let mut next_id = next_note_id(chart);
    let mut additions = Vec::new();
    let mut pairs = Vec::new();
    let mut assist_added = false;
    for &lane in chart.metadata.key_mode.active_lanes() {
        let lane_index = lane.index();
        let occupied: BTreeSet<TimeUs> =
            chart.lane_notes[lane_index].iter().map(|note| note.time).collect();
        for note in &mut chart.lane_notes[lane_index] {
            if note.kind != NoteKind::Tap || random_unit(rng) >= rate {
                continue;
            }
            let Ok(index) = timeline.binary_search_by_key(&note.time, |entry| entry.0) else {
                continue;
            };
            let Some(&(end_time, end_tick)) = timeline.get(index + 1) else {
                continue;
            };
            if occupied.contains(&end_time) {
                continue;
            }
            let pair_mode = match mode {
                AssistLongNoteMode::AddLn => LongNoteMode::Ln,
                AssistLongNoteMode::AddCn => LongNoteMode::Cn,
                AssistLongNoteMode::AddHcn => LongNoteMode::Hcn,
                AssistLongNoteMode::AddAll => match rng.next_int_bound(3) {
                    0 => LongNoteMode::Ln,
                    1 => LongNoteMode::Cn,
                    _ => LongNoteMode::Hcn,
                },
                AssistLongNoteMode::Off | AssistLongNoteMode::Remove => continue,
            };
            assist_added |= pair_mode != LongNoteMode::Ln;
            let end_id = next_id;
            next_id.0 = next_id.0.saturating_add(1);
            note.kind = NoteKind::LongStart;
            additions.push(NoteEvent {
                id: end_id,
                lane,
                kind: NoteKind::LongEnd,
                tick: end_tick,
                time: end_time,
                sound: None,
                layered_sounds: Vec::new(),
                damage: None,
            });
            pairs.push(LongNotePair {
                lane,
                style: LongNoteStyle::ChannelPair,
                mode: Some(pair_mode),
                start_note_id: note.id,
                end_note_id: end_id,
                start_tick: note.tick,
                end_tick,
                start_time: note.time,
                end_time,
                sound: note.sound,
            });
        }
        chart.lane_notes[lane_index].append(&mut additions);
        chart.lane_notes[lane_index].sort_by_key(|note| (note.time, note.id));
    }
    let added = !pairs.is_empty();
    chart.long_notes.extend(pairs);
    if added {
        chart.metadata.long_note_mode_defined = true;
    }
    (added, added && assist_added)
}

fn remove_mines(chart: &mut PlayableChart) -> bool {
    let mut removed = false;
    for notes in &mut chart.lane_notes {
        let before = notes.len();
        notes.retain(|note| note.kind != NoteKind::Mine);
        removed |= notes.len() != before;
    }
    removed
}

fn add_mines(chart: &mut PlayableChart, mode: AssistMineMode, rng: &mut JavaRandom) {
    let timeline = chart_timeline(chart);
    let active_lanes = chart.metadata.key_mode.active_lanes();
    let mut next_id = next_note_id(chart);
    for &(time, tick) in &timeline {
        let blank: Vec<bool> =
            active_lanes.iter().map(|&lane| lane_is_blank_at(chart, lane, time)).collect();
        for (position, &lane) in active_lanes.iter().enumerate() {
            if !blank[position] {
                continue;
            }
            let add = match mode {
                AssistMineMode::AddRandom => rng.next_int_bound(10) == 0,
                AssistMineMode::AddNear => {
                    position.checked_sub(1).is_some_and(|index| !blank[index])
                        || blank.get(position + 1).is_some_and(|blank| !blank)
                }
                AssistMineMode::AddBlank => true,
                AssistMineMode::Off | AssistMineMode::Remove => false,
            };
            if add {
                chart.lane_notes[lane.index()].push(NoteEvent {
                    id: next_id,
                    lane,
                    kind: NoteKind::Mine,
                    tick,
                    time,
                    sound: None,
                    layered_sounds: Vec::new(),
                    damage: Some(10.0),
                });
                next_id.0 = next_id.0.saturating_add(1);
            }
        }
    }
    for notes in &mut chart.lane_notes {
        notes.sort_by_key(|note| (note.time, note.id));
    }
}

fn add_extra_notes(chart: &mut PlayableChart, depth: u8, scratch: bool) -> bool {
    let mut grouped: BTreeMap<(TimeUs, ChartTick), Vec<SoundEvent>> = BTreeMap::new();
    for event in chart.bgm_events.drain(..) {
        grouped.entry((event.time, event.tick)).or_default().push(event);
    }
    let mut next_id = next_note_id(chart);
    let mut moved = false;
    let mut last_offset = 0;
    let active_lanes = chart.metadata.key_mode.active_lanes();
    for ((time, tick), mut events) in grouped {
        for _ in 0..depth {
            let Some(sound) = events.first().map(|event| event.sound) else {
                break;
            };
            let target = (0..active_lanes.len()).find_map(|step| {
                let position = (last_offset + step) % active_lanes.len();
                let lane = active_lanes[position];
                let is_scratch = matches!(lane, Lane::Scratch | Lane::Scratch2);
                (lane_is_blank_at(chart, lane, time) && (scratch || !is_scratch))
                    .then_some((position, lane))
            });
            let Some((position, lane)) = target else {
                break;
            };
            last_offset = position;
            chart.lane_notes[lane.index()].push(NoteEvent {
                id: next_id,
                lane,
                kind: NoteKind::Tap,
                tick,
                time,
                sound: Some(sound),
                layered_sounds: Vec::new(),
                damage: None,
            });
            next_id.0 = next_id.0.saturating_add(1);
            events.remove(0);
            moved = true;
        }
        chart.bgm_events.extend(events);
    }
    chart.bgm_events.sort_by_key(|event| event.time);
    for notes in &mut chart.lane_notes {
        notes.sort_by_key(|note| (note.time, note.id));
    }
    moved
}

fn chart_timeline(chart: &PlayableChart) -> Vec<(TimeUs, ChartTick)> {
    let mut timeline = BTreeMap::new();
    for note in chart.lane_notes.iter().flatten() {
        timeline.entry(note.time).or_insert(note.tick);
    }
    for event in &chart.bgm_events {
        timeline.entry(event.time).or_insert(event.tick);
    }
    for event in &chart.timing_events {
        timeline.entry(event.time).or_insert(event.tick);
    }
    for line in &chart.bar_lines {
        timeline.entry(line.time).or_insert(line.tick);
    }
    timeline.into_iter().collect()
}

fn lane_is_blank_at(chart: &PlayableChart, lane: Lane, time: TimeUs) -> bool {
    !chart.lane_notes[lane.index()].iter().any(|note| note.time == time)
        && !chart
            .long_notes
            .iter()
            .any(|pair| pair.lane == lane && pair.start_time <= time && time < pair.end_time)
}

fn next_note_id(chart: &PlayableChart) -> NoteId {
    NoteId(
        chart
            .lane_notes
            .iter()
            .flatten()
            .map(|note| note.id.0)
            .chain(
                chart.long_notes.iter().flat_map(|pair| [pair.start_note_id.0, pair.end_note_id.0]),
            )
            .max()
            .unwrap_or(0)
            .saturating_add(1),
    )
}

fn random_unit(rng: &mut JavaRandom) -> f64 {
    f64::from(rng.next_int_bound(1_000_000)) / 1_000_000.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use bmz_chart::hash::compute_chart_identity;
    use bmz_chart::model::{BarLine, ChartMetadata, TimingEvent};
    use bmz_core::ids::SoundId;

    fn chart() -> PlayableChart {
        let start = NoteEvent {
            id: NoteId(1),
            lane: Lane::Key1,
            kind: NoteKind::LongStart,
            tick: ChartTick(960),
            time: TimeUs(1_000_000),
            sound: Some(SoundId(1)),
            layered_sounds: Vec::new(),
            damage: None,
        };
        let end = NoteEvent {
            id: NoteId(2),
            lane: Lane::Key1,
            kind: NoteKind::LongEnd,
            tick: ChartTick(1_920),
            time: TimeUs(2_000_000),
            sound: None,
            layered_sounds: Vec::new(),
            damage: None,
        };
        let mine = NoteEvent {
            id: NoteId(3),
            lane: Lane::Key2,
            kind: NoteKind::Mine,
            tick: ChartTick(1_920),
            time: TimeUs(2_000_000),
            sound: None,
            layered_sounds: Vec::new(),
            damage: Some(10.0),
        };
        let mut lane_notes = std::array::from_fn(|_| Vec::new());
        lane_notes[Lane::Key1.index()] = vec![start, end];
        lane_notes[Lane::Key2.index()] = vec![mine];
        PlayableChart {
            identity: compute_chart_identity(b"assist"),
            metadata: ChartMetadata {
                key_mode: bmz_core::lane::KeyMode::K7,
                initial_bpm: 120.0,
                ..Default::default()
            },
            lane_notes,
            long_notes: vec![LongNotePair {
                lane: Lane::Key1,
                style: LongNoteStyle::ChannelPair,
                mode: Some(LongNoteMode::Ln),
                start_note_id: NoteId(1),
                end_note_id: NoteId(2),
                start_tick: ChartTick(960),
                end_tick: ChartTick(1_920),
                start_time: TimeUs(1_000_000),
                end_time: TimeUs(2_000_000),
                sound: Some(SoundId(1)),
            }],
            bgm_events: Vec::new(),
            bga_events: Vec::new(),
            timing_events: vec![TimingEvent {
                tick: ChartTick(960),
                time: TimeUs(1_000_000),
                kind: TimingEventKind::BpmChange { bpm: 180.0 },
            }],
            scroll_events: vec![
                ScrollEvent { tick: ChartTick(0), time: TimeUs(0), factor: 1.0 },
                ScrollEvent { tick: ChartTick(960), time: TimeUs(1_000_000), factor: 0.5 },
            ],
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
            bar_lines: vec![
                BarLine { measure: 0, tick: ChartTick(0), time: TimeUs(0) },
                BarLine { measure: 1, tick: ChartTick(3_840), time: TimeUs(3_000_000) },
            ],
            sounds: Vec::new(),
            bga_assets: Vec::new(),
            total_notes: 1,
            end_time: TimeUs(3_000_000),
        }
    }

    #[test]
    fn mask_maps_beatoraja_buttons_in_order() {
        let config = AssistOptionConfig {
            expand_judge: true,
            scroll_mode: AssistScrollMode::Remove,
            judge_area: true,
            long_note_mode: AssistLongNoteMode::Remove,
            mark_note: true,
            bpm_guide: true,
            mine_mode: AssistMineMode::Remove,
            ..Default::default()
        };
        assert_eq!(configured_mask(config), 0x7f);
        assert_eq!(mask_flags(0x7f), [true; 7]);
    }

    #[test]
    fn custom_judge_expands_nested_windows_but_not_past_bad() {
        let window =
            JudgeWindow::symmetric(20_000, 40_000, 100_000, 120_000, 150_000, 150_000, 16_000);
        let windows = JudgeWindows {
            note: window,
            scratch: window,
            long_note_end: window,
            long_scratch_end: window,
            long_note_release_margin_us: 50_000,
            long_scratch_release_margin_us: 60_000,
        };
        let scaled = apply_custom_judge_windows(
            windows,
            AssistOptionConfig {
                expand_judge: true,
                key_pgreat_rate: 400,
                key_great_rate: 400,
                key_good_rate: 100,
                long_note_margin_rate: 200,
                ..Default::default()
            },
        );
        assert_eq!(
            (scaled.note.pgreat_us, scaled.note.great_us, scaled.note.good_us),
            (80_000, 120_000, 120_000)
        );
        assert_eq!(scaled.long_note_release_margin_us, 100_000);
        assert_eq!(scaled.long_scratch_release_margin_us, 60_000);
    }

    #[test]
    fn core_chart_assists_apply_and_escalate_to_assist() {
        let mut chart = chart();
        let runtime = apply_chart_assists(
            &mut chart,
            AssistOptionConfig {
                scroll_mode: AssistScrollMode::Remove,
                long_note_mode: AssistLongNoteMode::Remove,
                mine_mode: AssistMineMode::Remove,
                bpm_guide: true,
                ..Default::default()
            },
            1,
        );

        assert_eq!(runtime.level, AssistLevel::Assist);
        assert_eq!(
            runtime.effective_mask,
            CONSTANT_MASK | LEGACY_NOTE_MASK | BPM_GUIDE_MASK | NO_MINE_MASK
        );
        assert!(chart.timing_events.is_empty());
        assert!(chart.scroll_events.is_empty());
        assert!(chart.long_notes.is_empty());
        assert_eq!(chart.lane_notes[Lane::Key1.index()].len(), 1);
        assert_eq!(chart.lane_notes[Lane::Key1.index()][0].kind, NoteKind::Tap);
        assert_eq!(chart.lane_notes[Lane::Key1.index()][0].tick, ChartTick(1_920));
        assert_eq!(chart.bar_lines[1].tick, ChartTick(5_760));
        assert!(chart.lane_notes.iter().flatten().all(|note| note.kind != NoteKind::Mine));
    }

    #[test]
    fn visual_assists_do_not_disable_score_on_fixed_bpm_chart() {
        let mut chart = chart();
        chart.timing_events.clear();
        let runtime = apply_chart_assists(
            &mut chart,
            AssistOptionConfig {
                judge_area: true,
                mark_note: true,
                bpm_guide: true,
                ..Default::default()
            },
            1,
        );
        assert_eq!(runtime.level, AssistLevel::None);
        assert!(runtime.score_update_enabled());
        assert!(runtime.judge_area && runtime.mark_note && runtime.bpm_guide);
    }

    #[test]
    fn beatoraja_light_assist_arranges_disable_score_on_either_side() {
        for arrange in [
            ArrangeOption::Spiral,
            ArrangeOption::HRandom,
            ArrangeOption::AllScratch,
            ArrangeOption::RandomEx,
            ArrangeOption::SRandomEx,
        ] {
            let mut p1 = AssistRuntime::default();
            merge_arrange_assist_level(&mut p1, arrange, ArrangeOption::Normal);
            assert_eq!(p1.level, AssistLevel::LightAssist, "1P {arrange:?}");
            assert!(!p1.score_update_enabled(), "1P {arrange:?}");

            let mut p2 = AssistRuntime::default();
            merge_arrange_assist_level(&mut p2, ArrangeOption::Normal, arrange);
            assert_eq!(p2.level, AssistLevel::LightAssist, "2P {arrange:?}");
            assert!(!p2.score_update_enabled(), "2P {arrange:?}");
        }
    }

    #[test]
    fn scoreable_arranges_do_not_raise_assist_level() {
        for arrange in [
            ArrangeOption::Normal,
            ArrangeOption::Mirror,
            ArrangeOption::Random,
            ArrangeOption::RRandom,
            ArrangeOption::SRandom,
            ArrangeOption::FRandom,
            ArrangeOption::MFRandom,
        ] {
            let mut runtime = AssistRuntime::default();
            merge_arrange_assist_level(&mut runtime, arrange, arrange);
            assert_eq!(runtime.level, AssistLevel::None, "{arrange:?}");
            assert!(runtime.score_update_enabled(), "{arrange:?}");
        }
    }

    #[test]
    fn added_ln_and_mines_match_beatoraja_assist_levels() {
        let mut base = chart();
        remove_long_notes(&mut base, 1.0, &mut JavaRandom::new(1));
        base.lane_notes[Lane::Key2.index()].clear();

        let mut ln = base.clone();
        let ln_runtime = apply_chart_assists(
            &mut ln,
            AssistOptionConfig {
                long_note_mode: AssistLongNoteMode::AddLn,
                long_note_rate: 1.0,
                mine_mode: AssistMineMode::AddBlank,
                ..Default::default()
            },
            2,
        );
        assert_eq!(ln_runtime.level, AssistLevel::None);
        assert!(!ln.long_notes.is_empty());
        assert!(ln.lane_notes.iter().flatten().any(|note| note.kind == NoteKind::Mine));

        let mut cn = base;
        let cn_runtime = apply_chart_assists(
            &mut cn,
            AssistOptionConfig {
                long_note_mode: AssistLongNoteMode::AddCn,
                long_note_rate: 1.0,
                ..Default::default()
            },
            2,
        );
        assert_eq!(cn_runtime.level, AssistLevel::Assist);
    }
}

use super::*;

pub(super) fn analysis_distribution_seconds(chart: &PlayableChart) -> usize {
    let last_note_us = chart
        .lane_notes
        .iter()
        .flat_map(|notes| notes.iter().map(|note| note.time.0))
        .max()
        .unwrap_or(0);
    let last_long_us = chart.long_notes.iter().map(|pair| pair.end_time.0).max().unwrap_or(0);
    let last_us = last_note_us.max(last_long_us).max(0);
    ((last_us / 1_000_000) as usize + 2).clamp(1, MAX_ANALYSIS_DISTRIBUTION_SECONDS)
}

pub(super) fn trim_trailing_empty_distribution(distribution: &mut Vec<ChartDistributionSecond>) {
    while distribution.len() > 1
        && distribution.last().copied().is_some_and(ChartDistributionSecond::is_empty)
    {
        distribution.pop();
    }
}

pub(super) fn encode_distribution_compact(distribution: &[ChartDistributionSecond]) -> String {
    let mut out = String::with_capacity(1 + distribution.len() * 14);
    out.push('#');
    for second in distribution {
        for value in [
            second.scratch_long_heads,
            second.scratch_long_bodies,
            second.scratch_taps,
            second.key_long_heads,
            second.key_long_bodies,
            second.key_taps,
            second.mines,
        ] {
            push_base36_2(&mut out, value);
        }
    }
    out
}

pub(super) fn decode_distribution(value: &str) -> Vec<ChartDistributionSecond> {
    if let Some(compact) = value.strip_prefix('#') {
        return decode_distribution_compact(compact).unwrap_or_default();
    }
    serde_json::from_str(value).unwrap_or_default()
}

pub(super) fn decode_distribution_compact(value: &str) -> Option<Vec<ChartDistributionSecond>> {
    if !value.len().is_multiple_of(14) || !value.is_ascii() {
        return None;
    }
    let mut out = Vec::with_capacity(value.len() / 14);
    for chunk in value.as_bytes().as_chunks::<14>().0 {
        out.push(ChartDistributionSecond {
            scratch_long_heads: parse_base36_2(&chunk[0..2])?,
            scratch_long_bodies: parse_base36_2(&chunk[2..4])?,
            scratch_taps: parse_base36_2(&chunk[4..6])?,
            key_long_heads: parse_base36_2(&chunk[6..8])?,
            key_long_bodies: parse_base36_2(&chunk[8..10])?,
            key_taps: parse_base36_2(&chunk[10..12])?,
            mines: parse_base36_2(&chunk[12..14])?,
        });
    }
    Some(out)
}

pub(super) fn push_base36_2(out: &mut String, value: u16) {
    let value = value.min(36 * 36 - 1);
    out.push(base36_digit((value / 36) as u8));
    out.push(base36_digit((value % 36) as u8));
}

pub(super) fn parse_base36_2(bytes: &[u8]) -> Option<u16> {
    Some(u16::from(parse_base36_digit(bytes[0])?) * 36 + u16::from(parse_base36_digit(bytes[1])?))
}

pub(super) fn base36_digit(value: u8) -> char {
    match value {
        0..=9 => (b'0' + value) as char,
        10..=35 => (b'a' + value - 10) as char,
        _ => unreachable!("base36 digit out of range"),
    }
}

pub(super) fn parse_base36_digit(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'z' => Some(value - b'a' + 10),
        b'A'..=b'Z' => Some(value - b'A' + 10),
        _ => None,
    }
}

pub(super) fn chart_analysis_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ChartAnalysis> {
    chart_analysis_from_row_with_offset(row, 0)
}

pub(super) fn chart_analysis_from_row_with_offset(
    row: &rusqlite::Row<'_>,
    offset: usize,
) -> rusqlite::Result<ChartAnalysis> {
    let distribution_json: String = row.get(offset + 9)?;
    let speed_changes_json: String = row.get(offset + 10)?;
    let lane_notes_json: String = row.get(offset + 11)?;
    Ok(ChartAnalysis {
        normal_notes: row.get(offset)?,
        long_notes: row.get(offset + 1)?,
        scratch_notes: row.get(offset + 2)?,
        long_scratch_notes: row.get(offset + 3)?,
        density: row.get(offset + 4)?,
        peak_density: row.get(offset + 5)?,
        end_density: row.get(offset + 6)?,
        total_gauge: row.get(offset + 7)?,
        main_bpm: row.get(offset + 8)?,
        distribution: decode_distribution(&distribution_json),
        speed_changes: serde_json::from_str(&speed_changes_json).unwrap_or_default(),
        lane_notes: serde_json::from_str(&lane_notes_json).unwrap_or_default(),
    })
}

pub(super) fn chart_analysis_summary_from_row_with_offset(
    row: &rusqlite::Row<'_>,
    offset: usize,
) -> rusqlite::Result<ChartAnalysisSummary> {
    Ok(ChartAnalysisSummary {
        normal_notes: row.get(offset)?,
        long_notes: row.get(offset + 1)?,
        scratch_notes: row.get(offset + 2)?,
        long_scratch_notes: row.get(offset + 3)?,
        density: row.get(offset + 4)?,
        peak_density: row.get(offset + 5)?,
        end_density: row.get(offset + 6)?,
        main_bpm: row.get(offset + 7)?,
    })
}

pub(super) fn second_index(time_us: i64) -> usize {
    (time_us / 1_000_000).max(0) as usize
}

pub(super) fn is_scratch_lane(lane: Lane) -> bool {
    matches!(lane, Lane::Scratch | Lane::Scratch2)
}

pub(super) fn add_long_head(second: &mut ChartDistributionSecond, lane: Lane) {
    if is_scratch_lane(lane) {
        second.scratch_long_heads = second.scratch_long_heads.saturating_add(1);
    } else {
        second.key_long_heads = second.key_long_heads.saturating_add(1);
    }
}

pub(super) fn add_long_body(second: &mut ChartDistributionSecond, lane: Lane, amount: i16) {
    let value = if is_scratch_lane(lane) {
        &mut second.scratch_long_bodies
    } else {
        &mut second.key_long_bodies
    };
    if amount >= 0 {
        *value = value.saturating_add(amount as u16);
    } else {
        *value = value.saturating_sub((-amount) as u16);
    }
}

pub(super) fn gauge_border_note_count(total: Option<f64>, total_notes: u32) -> i64 {
    let total = total.unwrap_or(0.0);
    if total <= 0.0 {
        return 0;
    }
    (f64::from(total_notes) * (1.0 - 100.0 / total)) as i64
}

pub(super) fn add_bpm_note_count(counts: &mut Vec<(f64, u32)>, bpm: f64, notes: u32) {
    if let Some((_, count)) =
        counts.iter_mut().find(|(candidate, _)| (*candidate - bpm).abs() < 0.0001)
    {
        *count = count.saturating_add(notes);
    } else {
        counts.push((bpm, notes));
    }
}

pub(super) fn bpm_at(chart: &PlayableChart, time_us: i64) -> f64 {
    let mut bpm = chart.metadata.initial_bpm;
    for event in &chart.timing_events {
        if event.time.0 > time_us {
            break;
        }
        if let TimingEventKind::BpmChange { bpm: event_bpm } = event.kind {
            bpm = event_bpm;
        }
    }
    bpm
}

pub(super) fn chart_speed_changes(chart: &PlayableChart) -> Vec<ChartSpeedChange> {
    let mut out = vec![ChartSpeedChange { speed: chart.metadata.initial_bpm, time_ms: 0 }];
    let mut current = chart.metadata.initial_bpm;
    // STOP 区間の終了時刻。Some の間は STOP 区間内とみなし、BPM 変化のみ先読みする。
    let mut stop_end_us: Option<i64> = None;
    for event in &chart.timing_events {
        let event_us = event.time.0;
        // STOP 区間を抜けたら resume エントリを出力してペンディングを解除する。
        if let Some(end_us) = stop_end_us {
            if event_us >= end_us {
                if end_us < chart.end_time.0 {
                    out.push(ChartSpeedChange { speed: current, time_ms: end_us / 1_000 });
                }
                stop_end_us = None;
                // fall through: このイベントを通常通り処理する
            } else {
                // STOP 区間内: BPM 変化だけ current に反映して次へ
                if let TimingEventKind::BpmChange { bpm } = event.kind {
                    current = bpm;
                }
                continue;
            }
        }
        match event.kind {
            TimingEventKind::Stop { duration_us } => {
                out.push(ChartSpeedChange { speed: 0.0, time_ms: event_us / 1_000 });
                // current は STOP 前の BPM のまま保持し、
                // STOP 区間内の BPM 変化を先読みするため stop_end_us をセットする。
                stop_end_us = Some(event_us + duration_us);
            }
            TimingEventKind::BpmChange { bpm } => {
                if (bpm - current).abs() > f64::EPSILON {
                    out.push(ChartSpeedChange { speed: bpm, time_ms: event_us / 1_000 });
                    current = bpm;
                }
            }
        }
    }
    // ループ終了後も STOP がペンディングなら resume エントリを出力する。
    if let Some(end_us) = stop_end_us
        && end_us < chart.end_time.0
    {
        out.push(ChartSpeedChange { speed: current, time_ms: end_us / 1_000 });
    }
    if out.last().is_some_and(|last| last.time_ms != chart.end_time.0 / 1_000) {
        out.push(ChartSpeedChange { speed: current, time_ms: chart.end_time.0 / 1_000 });
    }
    out
}

#[derive(Debug, Clone, Copy)]
pub(super) struct ChartStats {
    pub(super) min_bpm: f64,
    pub(super) max_bpm: f64,
    pub(super) ln_type: &'static str,
    pub(super) has_long_notes: bool,
    pub(super) has_mines: bool,
    pub(super) ln_profile: ChartLnProfile,
    pub(super) ln_counts: ChartLnCounts,
}

impl ChartStats {
    pub(super) fn from_chart(chart: &PlayableChart) -> Self {
        let mut min_bpm: f64 = chart.metadata.initial_bpm;
        let mut max_bpm: f64 = chart.metadata.initial_bpm;
        for event in &chart.timing_events {
            if let TimingEventKind::BpmChange { bpm } = event.kind {
                min_bpm = min_bpm.min(bpm);
                max_bpm = max_bpm.max(bpm);
            }
        }

        let has_mines = chart
            .lane_notes
            .iter()
            .flat_map(|notes| notes.iter())
            .any(|note| note.kind == NoteKind::Mine);
        let ln_counts = ChartLnCounts::from_chart(chart);
        let ln_profile = ln_counts.profile();

        Self {
            min_bpm,
            max_bpm,
            ln_type: if chart.long_notes.is_empty() {
                ""
            } else if chart.metadata.long_note_mode_defined {
                match chart.metadata.long_note_mode {
                    LongNoteMode::Ln => "Ln",
                    LongNoteMode::Cn => "Cn",
                    LongNoteMode::Hcn => "Hcn",
                }
            } else {
                "LongNote"
            },
            has_long_notes: !chart.long_notes.is_empty(),
            has_mines,
            ln_profile,
            ln_counts,
        }
    }
}

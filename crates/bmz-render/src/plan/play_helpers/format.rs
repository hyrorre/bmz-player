use super::*;

pub(super) fn format_delta_ms(delta_us: i64) -> String {
    let sign = if delta_us < 0 { "-" } else { "+" };
    format!("{}{}MS", sign, delta_us.abs() / 1_000)
}

pub(super) fn format_percent(rate: f32) -> String {
    format!("{}%", (rate.clamp(0.0, 1.0) * 100.0).round() as u32)
}

pub(super) fn format_time(time: TimeUs) -> String {
    let seconds = (time.0.max(0) / 1_000_000) as u32;
    format!("{:02}:{:02}", seconds / 60, seconds % 60)
}

pub(super) fn start_overlay_label(time: TimeUs) -> Option<&'static str> {
    match time.0 {
        ..=999_999 => Some("READY"),
        1_000_000..=1_599_999 => Some("GO"),
        _ => None,
    }
}

pub(super) fn lane_flash_color(snapshot: &RenderSnapshot, lane: Lane) -> Option<Color> {
    if let Some(judgement_color) = judgement_lane_flash_color(snapshot, lane) {
        return Some(judgement_color);
    }

    input_lane_flash_color(snapshot, lane)
}

pub(super) fn long_note_body_color(mode: LongNoteMode) -> Color {
    match mode {
        LongNoteMode::Ln => LONG_NOTE_BODY_COLOR,
        LongNoteMode::Cn => CN_BODY_COLOR,
        LongNoteMode::Hcn | LongNoteMode::Hln => HCN_BODY_COLOR,
    }
}

pub(super) fn judgement_lane_flash_color(snapshot: &RenderSnapshot, lane: Lane) -> Option<Color> {
    let judgement = snapshot.recent_judgements.iter().rev().find(|judgement| {
        judgement.lane == lane
            && !judgement.is_miss
            && (0..=220_000).contains(&(snapshot.time.0 - judgement.time.0))
    })?;
    let age_us = (snapshot.time.0 - judgement.time.0).max(0) as f32;
    let alpha = (1.0 - age_us / 220_000.0).clamp(0.0, 1.0) * 0.55;
    Some(judge_flash_color(&judgement.text, alpha))
}

pub(super) fn input_lane_flash_color(snapshot: &RenderSnapshot, lane: Lane) -> Option<Color> {
    let input = snapshot.recent_inputs.iter().rev().find(|input| {
        input.lane == lane && (0..=140_000).contains(&(snapshot.time.0 - input.time.0))
    })?;
    let age_us = (snapshot.time.0 - input.time.0).max(0) as f32;
    let alpha = (1.0 - age_us / 140_000.0).clamp(0.0, 1.0) * 0.32;
    Some(Color::rgba(0.95, 0.98, 1.0, alpha))
}

pub(super) fn judge_flash_color(text: &str, alpha: f32) -> Color {
    if text.starts_with("PGREAT") || text.starts_with("GREAT") {
        Color::rgba(0.55, 0.9, 1.0, alpha)
    } else if text.starts_with("GOOD") {
        Color::rgba(0.85, 0.9, 0.45, alpha)
    } else {
        Color::rgba(1.0, 0.28, 0.32, alpha)
    }
}

pub(super) fn judgement_history_label(judgement: &crate::snapshot::DisplayJudgement) -> String {
    format!("{} {}", judge_short_label(&judgement.text), side_short_label(&judgement.text))
}

pub(super) fn judge_short_label(text: &str) -> &'static str {
    if text.starts_with("PGREAT") {
        "PG"
    } else if text.starts_with("GREAT") {
        "GR"
    } else if text.starts_with("GOOD") {
        "GD"
    } else if text.starts_with("BAD") {
        "BD"
    } else if text.starts_with("EMPTY POOR") {
        "EP"
    } else if text.starts_with("POOR") {
        "PR"
    } else {
        "??"
    }
}

pub(super) fn side_short_label(text: &str) -> &'static str {
    if text.ends_with("FAST") {
        "F"
    } else if text.ends_with("SLOW") {
        "S"
    } else {
        "-"
    }
}

pub(super) fn judgement_history_color(text: &str) -> Color {
    if text.starts_with("PGREAT") || text.starts_with("GREAT") {
        Color::rgb(0.64, 0.9, 0.98)
    } else if text.starts_with("GOOD") {
        Color::rgb(0.84, 0.88, 0.48)
    } else {
        Color::rgb(0.96, 0.4, 0.44)
    }
}

pub(super) fn display_title(title: &str) -> String {
    display_label(title, 24)
}

pub(super) fn display_label(text: &str, max_chars: usize) -> String {
    let ascii: String = text
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, ' ' | '-' | '.' | '/' | ':') {
                ch
            } else {
                '?'
            }
        })
        .take(max_chars)
        .collect();
    if ascii.is_empty() { "NO TITLE".to_string() } else { ascii }
}

pub(super) fn skin_image_tint(_lane: Lane) -> Color {
    Color::rgb(1.0, 1.0, 1.0)
}

pub(super) fn lane_label(lane: Lane) -> &'static str {
    match lane {
        Lane::Scratch => "SC",
        Lane::Key1 => "1",
        Lane::Key2 => "2",
        Lane::Key3 => "3",
        Lane::Key4 => "4",
        Lane::Key5 => "5",
        Lane::Key6 => "6",
        Lane::Key7 => "7",
        Lane::Key8 => "1'",
        Lane::Key9 => "2'",
        Lane::Key10 => "3'",
        Lane::Key11 => "4'",
        Lane::Key12 => "5'",
        Lane::Key13 => "6'",
        Lane::Key14 => "7'",
        Lane::Scratch2 => "S2",
    }
}

pub(super) fn lane_key_label(lane: Lane) -> &'static str {
    match lane {
        Lane::Scratch => "LS",
        Lane::Key1 => "Z",
        Lane::Key2 => "S",
        Lane::Key3 => "X",
        Lane::Key4 => "D",
        Lane::Key5 => "C",
        Lane::Key6 => "F",
        Lane::Key7 => "V",
        Lane::Key8 => "Z",
        Lane::Key9 => "S",
        Lane::Key10 => "X",
        Lane::Key11 => "D",
        Lane::Key12 => "C",
        Lane::Key13 => "F",
        Lane::Key14 => "V",
        Lane::Scratch2 => "LS",
    }
}

pub(super) fn label_width(label: &str, cell: f32) -> f32 {
    let chars = label.chars().count() as f32;
    if chars == 0.0 { 0.0 } else { (chars * 3.0 + (chars - 1.0)) * cell }
}

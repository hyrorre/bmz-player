use super::*;

pub(super) fn decode_external_ghost(encoded: &str, total_notes: u32) -> Vec<u8> {
    if encoded.is_empty() {
        return Vec::new();
    }
    match decode_beatoraja_ghost(encoded, total_notes) {
        Ok(ghost) => ghost,
        Err(error) => {
            tracing::warn!(%error, "failed to decode imported score ghost");
            Vec::new()
        }
    }
}

/// Decodes LR2's `score.ghost` column into bmz's per-note judge array.
///
/// The LR2 format is a run-length encoding of per-note judge symbols
/// `@ A B C D E` (= judge codes 0..=5), wrapped in two layers of bigram dictionary compression.  We reverse the
/// dictionaries (layer 2 then layer 1, as LR2 does), expand the run-length runs,
/// then map LR2 judge codes to bmz's (`5 - code`): E/5=PGreat→0, D/4=Great→1,
/// C/3=Good→2, B/2=Bad→3, A/1=Poor→4.  Code 0 (`@`) is an empty poor not tied to a
/// scoreable note and is dropped.  The result is padded with Poor / truncated to
/// `total_notes`, mirroring [`decode_beatoraja_ghost`].
pub(super) fn decode_lr2_ghost(encoded: &str, total_notes: u32) -> Vec<u8> {
    if encoded.is_empty() {
        return Vec::new();
    }

    let mut layer2 = String::with_capacity(encoded.len() * 2);
    for c in encoded.chars() {
        match lr2_ghost_layer2_symbol(c) {
            Some(replacement) => layer2.push_str(replacement),
            None => layer2.push(c),
        }
    }
    let mut expanded = String::with_capacity(layer2.len() * 2);
    for c in layer2.chars() {
        match lr2_ghost_layer1_symbol(c) {
            Some(replacement) => expanded.push_str(replacement),
            None => expanded.push(c),
        }
    }

    let mut ghost: Vec<u8> = Vec::with_capacity(total_notes as usize);
    let mut current: Option<u8> = None;
    let mut rep: i64 = -1;
    for c in expanded.chars() {
        let o = c as u32;
        if (0x40..=0x45).contains(&o) {
            if let Some(code) = current {
                push_lr2_run(&mut ghost, code, if rep == 0 { 1 } else { rep });
            }
            rep = 0;
            current = Some((o - 0x40) as u8);
        } else if c.is_ascii_digit() {
            let digit = (o - 0x30) as i64;
            rep = if rep == 0 { digit } else { rep * 10 + digit };
        }
    }
    if let Some(code) = current {
        push_lr2_run(&mut ghost, code, if rep == 0 { 1 } else { rep });
    }

    let expected = total_notes as usize;
    if expected > 0 {
        if ghost.len() < expected {
            ghost.resize(expected, 4);
        } else {
            ghost.truncate(expected);
        }
    }
    ghost
}

/// Appends `count` copies of an LR2 judge `code` (0..=5) to a bmz ghost, mapping
/// LR2 codes to bmz judge codes via `5 - code`.  Code 0 (empty poor) is not a
/// scoreable note and is skipped.
pub(super) fn push_lr2_run(ghost: &mut Vec<u8>, code: u8, count: i64) {
    if (1..=5).contains(&code) {
        let bmz_code = 5 - code;
        for _ in 0..count.max(1) {
            ghost.push(bmz_code);
        }
    }
}

/// LR2 ghost layer-2 dictionary (`q`..`z`), reversed on decode before layer 1.
pub(super) fn lr2_ghost_layer2_symbol(c: char) -> Option<&'static str> {
    Some(match c {
        'q' => "XX",
        'r' => "X1",
        's' => "X2",
        't' => "X3",
        'u' => "X4",
        'v' => "X5",
        'w' => "X6",
        'x' => "X7",
        'y' => "X8",
        'z' => "X9",
        _ => return None,
    })
}

/// LR2 ghost layer-1 dictionary (`F`..`p`), reversed after layer 2 on decode.
pub(super) fn lr2_ghost_layer1_symbol(c: char) -> Option<&'static str> {
    Some(match c {
        'F' => "E1",
        'G' => "E2",
        'H' => "E3",
        'I' => "E4",
        'J' => "E5",
        'K' => "E6",
        'L' => "E7",
        'M' => "E8",
        'N' => "E9",
        'P' => "EC",
        'Q' => "EB",
        'R' => "EA",
        'S' => "D2",
        'T' => "D3",
        'U' => "D4",
        'V' => "D5",
        'W' => "D6",
        'X' => "DE",
        'Y' => "DC",
        'a' => "DB",
        'b' => "DA",
        'c' => "C2",
        'd' => "C3",
        'e' => "C4",
        'f' => "C5",
        'g' => "CE",
        'h' => "CD",
        'i' => "CB",
        'j' => "CA",
        'k' => "AB",
        'l' => "AC",
        'm' => "AD",
        'n' => "AE",
        'o' => "A2",
        'p' => "A3",
        _ => return None,
    })
}

pub(super) fn normalize_imported_played_at(value: i64) -> Option<i64> {
    if value <= 0 {
        None
    } else if value >= 100_000_000_000 {
        Some(value / 1000)
    } else {
        Some(value)
    }
}

pub(super) fn lr2_clear_type(clear: i64) -> ClearType {
    match clear {
        0 => ClearType::NoPlay,
        1 => ClearType::Failed,
        2 => ClearType::Easy,
        3 => ClearType::Normal,
        4 => ClearType::Hard,
        5 => ClearType::FullCombo,
        6 => ClearType::Perfect,
        _ => ClearType::NoPlay,
    }
}

pub(super) fn beatoraja_clear_type(clear: i64) -> ClearType {
    match clear {
        0 => ClearType::NoPlay,
        1 => ClearType::Failed,
        2 => ClearType::AssistEasy,
        3 => ClearType::LightAssistEasy,
        4 => ClearType::Easy,
        5 => ClearType::Normal,
        6 => ClearType::Hard,
        7 => ClearType::ExHard,
        8 => ClearType::FullCombo,
        9 => ClearType::Perfect,
        10 => ClearType::Max,
        _ => ClearType::NoPlay,
    }
}

pub(super) fn gauge_type_for_clear(clear_type: ClearType) -> Option<GaugeType> {
    match clear_type {
        ClearType::AssistEasy | ClearType::LightAssistEasy => Some(GaugeType::AssistEasy),
        ClearType::Easy => Some(GaugeType::Easy),
        ClearType::Normal | ClearType::FullCombo | ClearType::Perfect | ClearType::Max => {
            Some(GaugeType::Normal)
        }
        ClearType::Hard => Some(GaugeType::Hard),
        ClearType::ExHard => Some(GaugeType::ExHard),
        ClearType::NoPlay | ClearType::Failed => None,
    }
}

pub(super) fn gauge_value_for_clear(clear_type: ClearType) -> f32 {
    match clear_type {
        ClearType::NoPlay | ClearType::Failed => 0.0,
        _ => 100.0,
    }
}

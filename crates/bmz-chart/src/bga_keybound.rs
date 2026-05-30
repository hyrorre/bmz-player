use bmz_core::lane::Lane;
use bmz_core::time::TimeUs;

use crate::model::{BgaAssetId, PlayableChart};

/// SWBGA `line` (BMS 可視キー通道の下2桁) からレーンへ。
pub fn swbga_line_to_lane(line: u8) -> Option<Lane> {
    match line {
        11 => Some(Lane::Key1),
        12 => Some(Lane::Key2),
        13 => Some(Lane::Key3),
        14 => Some(Lane::Key4),
        15 => Some(Lane::Key5),
        16 => Some(Lane::Scratch),
        17 => Some(Lane::Key6),
        18 => Some(Lane::Key7),
        21 => Some(Lane::Key8),
        22 => Some(Lane::Key9),
        23 => Some(Lane::Key10),
        24 => Some(Lane::Key11),
        25 => Some(Lane::Key12),
        26 => Some(Lane::Scratch2),
        27 => Some(Lane::Key13),
        28 => Some(Lane::Key14),
        _ => None,
    }
}

/// `#SWBGA` pattern 文字列 (2桁オブジェクト ID 列) を BMP キー列へ。
///
/// `base62_obj_ids` が `true` のとき beatoraja / bms-rs の base62 換算、
/// それ以外は base36 換算を使う。
pub fn parse_swbga_pattern(pattern: &str, base62_obj_ids: bool) -> Vec<u16> {
    let trimmed = pattern.trim();
    if trimmed.is_empty() {
        return Vec::new();
    }
    trimmed
        .as_bytes()
        .chunks(2)
        .filter_map(|chunk| parse_obj_id_pair(chunk, base62_obj_ids))
        .collect()
}

fn parse_obj_id_pair(chars: &[u8], base62_obj_ids: bool) -> Option<u16> {
    let [c1, c2] = chars else {
        return None;
    };
    if base62_obj_ids { parse_int62(*c1, *c2) } else { parse_int36(*c1, *c2) }
}

fn base62_digit(c: u8) -> Option<u8> {
    match c {
        b'0'..=b'9' => Some(c - b'0'),
        b'A'..=b'Z' => Some(c - b'A' + 10),
        b'a'..=b'z' => Some(c - b'a' + 36),
        _ => None,
    }
}

fn parse_int62(c1: u8, c2: u8) -> Option<u16> {
    let hi = base62_digit(c1)?;
    let lo = base62_digit(c2)?;
    Some(u16::from(hi) * 62 + u16::from(lo))
}

fn parse_int36(c1: u8, c2: u8) -> Option<u16> {
    let digit = |c: u8| -> Option<u8> {
        match c {
            b'0'..=b'9' => Some(c - b'0'),
            b'a'..=b'z' => Some(c - b'a' + 10),
            b'A'..=b'Z' => Some(c - b'A' + 10),
            _ => None,
        }
    };
    let hi = digit(c1)?;
    let lo = digit(c2)?;
    Some(u16::from(hi) * 36 + u16::from(lo))
}

/// キー押下中の keybound BGA フレーム (BMP キー) を返す。
pub fn keybound_bmp_key_at_time(
    chart: &PlayableChart,
    render_now: TimeUs,
    lane_keyon_started_at: [Option<TimeUs>; bmz_core::lane::LANE_COUNT],
) -> Option<u16> {
    let event = chart.bga_keybound_events.iter().rev().find(|event| event.time <= render_now)?;
    let definition = chart.swbga_definitions.iter().find(|def| def.id == event.swbga_id)?;
    let lane = swbga_line_to_lane(definition.line)?;
    let keyon = lane_keyon_started_at[lane.index()]?;
    let elapsed_ms = (render_now.0.saturating_sub(keyon.0) / 1_000) as u32;
    if definition.total_time_ms > 0 && elapsed_ms > definition.total_time_ms {
        return None;
    }
    if definition.pattern_bmp_keys.is_empty() {
        return None;
    }
    let frame_rate = definition.frame_rate_ms.max(1);
    let mut frame_index = (elapsed_ms / frame_rate) as usize;
    if definition.loop_mode {
        frame_index %= definition.pattern_bmp_keys.len();
    } else {
        frame_index = frame_index.min(definition.pattern_bmp_keys.len() - 1);
    }
    Some(definition.pattern_bmp_keys[frame_index])
}

pub fn keybound_bga_asset_at_time(
    chart: &PlayableChart,
    render_now: TimeUs,
    lane_keyon_started_at: [Option<TimeUs>; bmz_core::lane::LANE_COUNT],
) -> Option<BgaAssetId> {
    let bmp_key = keybound_bmp_key_at_time(chart, render_now, lane_keyon_started_at)?;
    chart.bga_asset_by_bmp_key.get(&bmp_key).copied()
}

#[cfg(test)]
mod tests {
    use bmz_core::time::ChartTick;

    use super::*;
    use crate::model::{BgaKeyboundEvent, SwBgaDefinition};

    #[test]
    fn parse_swbga_pattern_reads_base36_pairs() {
        assert_eq!(parse_swbga_pattern("0102ZZ", false), vec![1, 2, 35 * 36 + 35]);
    }

    #[test]
    fn parse_swbga_pattern_reads_base62_pairs() {
        assert_eq!(parse_swbga_pattern("0102ZZ", true), vec![1, 2, 35 * 62 + 35]);
        // aa=36*62+36, AA=10*62+10
        assert_eq!(parse_swbga_pattern("aaAA", true), vec![36 * 62 + 36, 10 * 62 + 10]);
        // "10" は base36=36, base62=62
        assert_eq!(parse_swbga_pattern("10", false), vec![36]);
        assert_eq!(parse_swbga_pattern("10", true), vec![62]);
    }

    #[test]
    fn keybound_frame_advances_with_key_hold() {
        let chart = PlayableChart {
            swbga_definitions: vec![SwBgaDefinition {
                id: 1,
                frame_rate_ms: 100,
                total_time_ms: 0,
                line: 11,
                loop_mode: false,
                chroma_alpha: 255,
                chroma_red: 0,
                chroma_green: 0,
                chroma_blue: 0,
                pattern_bmp_keys: vec![10, 20, 30],
            }],
            bga_keybound_events: vec![BgaKeyboundEvent {
                tick: ChartTick(0),
                time: TimeUs(0),
                swbga_id: 1,
            }],
            bga_asset_by_bmp_key: [(10, BgaAssetId(0)), (20, BgaAssetId(1)), (30, BgaAssetId(2))]
                .into_iter()
                .collect(),
            ..test_chart()
        };
        let mut keyon = [None; bmz_core::lane::LANE_COUNT];
        keyon[Lane::Key1.index()] = Some(TimeUs(0));

        assert_eq!(keybound_bga_asset_at_time(&chart, TimeUs(50_000), keyon), Some(BgaAssetId(0)));
        assert_eq!(keybound_bga_asset_at_time(&chart, TimeUs(150_000), keyon), Some(BgaAssetId(1)));
        assert_eq!(keybound_bga_asset_at_time(&chart, TimeUs(250_000), keyon), Some(BgaAssetId(2)));
    }

    fn test_chart() -> PlayableChart {
        use bmz_core::chart::ChartIdentity;
        use bmz_core::lane::KeyMode;

        use crate::model::ChartMetadata;

        PlayableChart {
            identity: ChartIdentity { file_md5: [0; 16], file_sha256: [0; 32] },
            metadata: ChartMetadata { key_mode: KeyMode::K7, ..ChartMetadata::default() },
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
            total_notes: 0,
            end_time: TimeUs(0),
        }
    }
}

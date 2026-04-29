use std::path::Path;

use bmz_core::time::ChartTick;

use crate::hash::compute_chart_identity;
use crate::timing::TICKS_PER_MEASURE;

use super::decode::decode_bms_text;
use super::error::{ImportError, ImportWarning};
use super::intermediate::{
    BmpDef, BpmDef, IntermediateChart, IntermediateMetadata, IntermediateObject,
    IntermediateObjectKind, IntermediateResources, MeasureInfo, StopDef, WavDef,
};

pub fn import_bms_to_intermediate(
    source_path: &Path,
    _random_seed: Option<u64>,
    warnings: &mut Vec<ImportWarning>,
) -> Result<IntermediateChart, ImportError> {
    let bytes = read_source_bytes(source_path)?;
    let identity = compute_chart_identity(&bytes);
    let text = decode_bms_text(&bytes, warnings);
    let mut intermediate = parse_bms_text(&text, warnings)?;
    intermediate.identity = identity;

    Ok(intermediate)
}

fn read_source_bytes(path: &Path) -> Result<Vec<u8>, ImportError> {
    std::fs::read(path).map_err(|source| ImportError::Io { path: path.to_path_buf(), source })
}

fn parse_bms_text(
    text: &str,
    warnings: &mut Vec<ImportWarning>,
) -> Result<IntermediateChart, ImportError> {
    let mut metadata = IntermediateMetadata { initial_bpm: 130.0, ..Default::default() };
    let mut resources = IntermediateResources::default();
    let mut objects = Vec::new();
    let mut measure_lengths = Vec::<(u32, u32, u32)>::new();
    let mut max_measure = 0_u32;
    let mut lnobj_wav_key = None;

    for raw_line in text.lines() {
        let line = raw_line.trim();
        if !line.starts_with('#') || line.len() == 1 {
            continue;
        }

        let body = &line[1..];
        if parse_channel_line(body, &mut objects, &mut measure_lengths, &mut max_measure, warnings)?
        {
            continue;
        }

        parse_command_line(body, &mut metadata, &mut resources, &mut lnobj_wav_key, warnings)?;
    }

    let measures = build_measures(max_measure, &measure_lengths);

    Ok(IntermediateChart {
        identity: compute_chart_identity(&[]),
        metadata,
        resources,
        measures,
        objects,
        lnobj_wav_key,
    })
}

fn parse_command_line(
    body: &str,
    metadata: &mut IntermediateMetadata,
    resources: &mut IntermediateResources,
    lnobj_wav_key: &mut Option<u16>,
    warnings: &mut Vec<ImportWarning>,
) -> Result<(), ImportError> {
    let Some((command, value)) = split_command_value(body) else {
        return Ok(());
    };
    let command_upper = command.to_ascii_uppercase();
    let value = value.trim();

    match command_upper.as_str() {
        "TITLE" => metadata.title = value.to_string(),
        "SUBTITLE" => metadata.subtitle = value.to_string(),
        "ARTIST" => metadata.artist = value.to_string(),
        "SUBARTIST" => metadata.subartist = value.to_string(),
        "GENRE" => metadata.genre = value.to_string(),
        "PLAYLEVEL" => metadata.play_level = value.to_string(),
        "DIFFICULTY" => metadata.difficulty_name = value.to_string(),
        "STAGEFILE" => metadata.stage_file = value.to_string(),
        "PREVIEW" => metadata.preview_file = value.to_string(),
        "TOTAL" => metadata.total = value.parse::<f64>().ok(),
        "BPM" => {
            if let Ok(bpm) = value.parse::<f64>() {
                metadata.initial_bpm = bpm;
            }
        }
        "LNOBJ" => *lnobj_wav_key = parse_base36_key(value),
        _ if command_upper.starts_with("WAV") && command_upper.len() == 5 => {
            if let Some(key) = parse_base36_key(&command_upper[3..]) {
                resources.wavs.push(WavDef { key, path: value.into() });
            }
        }
        _ if command_upper.starts_with("BMP") && command_upper.len() == 5 => {
            if let Some(key) = parse_base36_key(&command_upper[3..]) {
                resources.bmps.push(BmpDef { key, path: value.into() });
            }
        }
        _ if command_upper.starts_with("BPM") && command_upper.len() == 5 => {
            if let (Some(key), Ok(bpm)) =
                (parse_base36_key(&command_upper[3..]), value.parse::<f64>())
            {
                resources.bpm_table.push(BpmDef { key, bpm });
            }
        }
        _ if command_upper.starts_with("STOP") && command_upper.len() == 6 => {
            if let (Some(key), Ok(value)) =
                (parse_base36_key(&command_upper[4..]), value.parse::<u64>())
            {
                resources.stop_table.push(StopDef { key, value });
            }
        }
        _ => warnings.push(ImportWarning::UnsupportedCommand { command: command.to_string() }),
    }

    Ok(())
}

fn parse_channel_line(
    body: &str,
    objects: &mut Vec<IntermediateObject>,
    measure_lengths: &mut Vec<(u32, u32, u32)>,
    max_measure: &mut u32,
    warnings: &mut Vec<ImportWarning>,
) -> Result<bool, ImportError> {
    if body.len() < 7 || body.as_bytes()[5] != b':' {
        return Ok(false);
    }

    let Some(measure) = parse_fixed_decimal_u32(&body[0..3]) else {
        return Ok(false);
    };
    let Some(channel) = parse_fixed_decimal_u16(&body[3..5]) else {
        return Ok(false);
    };

    *max_measure = (*max_measure).max(measure);
    let data = body[6..].trim();

    if channel == 2 {
        if let Some((num, den)) = parse_measure_length(data) {
            measure_lengths.push((measure, num, den));
        } else {
            warnings.push(ImportWarning::SuspiciousMeasureLength { measure });
        }
        return Ok(true);
    }

    if data.len() % 2 != 0 {
        return Err(ImportError::Parse {
            path: Default::default(),
            message: format!(
                "channel data length must be even: measure {measure}, channel {channel}"
            ),
        });
    }

    let object_count = (data.len() / 2) as u32;
    if object_count == 0 {
        return Ok(true);
    }

    for (index, chunk) in data.as_bytes().chunks_exact(2).enumerate() {
        if chunk == b"00" {
            continue;
        }

        let token = std::str::from_utf8(chunk).map_err(|_| ImportError::Parse {
            path: Default::default(),
            message: "channel data contains non-UTF-8 token".to_string(),
        })?;

        let Some(kind) = object_kind_from_channel(channel, token, warnings) else {
            continue;
        };

        objects.push(IntermediateObject {
            measure,
            position_num: index as u32,
            position_den: object_count,
            kind,
        });
    }

    Ok(true)
}

fn object_kind_from_channel(
    channel: u16,
    token: &str,
    warnings: &mut Vec<ImportWarning>,
) -> Option<IntermediateObjectKind> {
    let key = parse_base36_key(token)?;
    match channel {
        1 => Some(IntermediateObjectKind::Bgm { wav_key: key }),
        3 => Some(IntermediateObjectKind::SetBpm { bpm: key as f64 }),
        8 => Some(IntermediateObjectKind::SetExtendedBpm { bpm_key: key }),
        9 => Some(IntermediateObjectKind::Stop { stop_key: key }),
        11 | 12 | 13 | 14 | 15 | 16 | 18 | 19 => visible_lane(channel)
            .map(|lane| IntermediateObjectKind::VisibleNote { lane, wav_key: Some(key) }),
        31 | 32 | 33 | 34 | 35 | 36 | 38 | 39 => visible_lane(channel - 20)
            .map(|lane| IntermediateObjectKind::InvisibleNote { lane, wav_key: Some(key) }),
        51 | 52 | 53 | 54 | 55 | 56 | 58 | 59 => visible_lane(channel - 40)
            .map(|lane| IntermediateObjectKind::LongChannelNote { lane, wav_key: Some(key) }),
        _ => {
            warnings.push(ImportWarning::UnsupportedChannel { channel });
            None
        }
    }
}

fn visible_lane(channel: u16) -> Option<bmz_core::lane::Lane> {
    match channel {
        16 => Some(bmz_core::lane::Lane::Scratch),
        11 => Some(bmz_core::lane::Lane::Key1),
        12 => Some(bmz_core::lane::Lane::Key2),
        13 => Some(bmz_core::lane::Lane::Key3),
        14 => Some(bmz_core::lane::Lane::Key4),
        15 => Some(bmz_core::lane::Lane::Key5),
        18 => Some(bmz_core::lane::Lane::Key6),
        19 => Some(bmz_core::lane::Lane::Key7),
        _ => None,
    }
}

fn build_measures(max_measure: u32, lengths: &[(u32, u32, u32)]) -> Vec<MeasureInfo> {
    let mut measures = Vec::new();
    let mut start_tick = 0_u64;

    for index in 0..=max_measure {
        let (num, den) = lengths
            .iter()
            .rev()
            .find(|(measure, _, _)| *measure == index)
            .map_or((1, 1), |(_, n, d)| (*n, *d));
        let tick_len = TICKS_PER_MEASURE as u64 * num as u64 / den.max(1) as u64;

        measures.push(MeasureInfo {
            index,
            length_ratio_num: num,
            length_ratio_den: den.max(1),
            start_tick: ChartTick(start_tick),
            tick_len,
        });
        start_tick += tick_len;
    }

    measures
}

fn split_command_value(body: &str) -> Option<(&str, &str)> {
    let index =
        body.char_indices().find(|(_, ch)| ch.is_ascii_whitespace()).map(|(index, _)| index)?;
    Some((&body[..index], &body[index..]))
}

fn parse_measure_length(value: &str) -> Option<(u32, u32)> {
    let number = value.parse::<f64>().ok()?;
    if !number.is_finite() || number <= 0.0 {
        return None;
    }

    let den = 1_000_000_u32;
    let num = (number * den as f64).round() as u32;
    Some(reduce_ratio(num, den))
}

fn reduce_ratio(mut num: u32, mut den: u32) -> (u32, u32) {
    let gcd = gcd(num, den).max(1);
    num /= gcd;
    den /= gcd;
    (num, den)
}

fn gcd(mut a: u32, mut b: u32) -> u32 {
    while b != 0 {
        let r = a % b;
        a = b;
        b = r;
    }
    a
}

fn parse_fixed_decimal_u32(value: &str) -> Option<u32> {
    value.chars().all(|ch| ch.is_ascii_digit()).then(|| value.parse().ok()).flatten()
}

fn parse_fixed_decimal_u16(value: &str) -> Option<u16> {
    value.chars().all(|ch| ch.is_ascii_digit()).then(|| value.parse().ok()).flatten()
}

fn parse_base36_key(value: &str) -> Option<u16> {
    let value = value.trim();
    if value.len() != 2 {
        return None;
    }

    let mut out = 0_u16;
    for byte in value.bytes() {
        let digit = match byte {
            b'0'..=b'9' => byte - b'0',
            b'A'..=b'Z' => byte - b'A' + 10,
            b'a'..=b'z' => byte - b'a' + 10,
            _ => return None,
        };
        out = out * 36 + digit as u16;
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use bmz_core::lane::Lane;

    use super::*;

    #[test]
    fn parses_7k_notes_resources_and_timing() {
        let text = "\
#TITLE Test Song
#ARTIST Composer
#BPM 150
#WAV01 kick.wav
#WAV0A snare.wav
#BPM01 180
#STOP01 192
#00011:0100
#00019:000A
#00108:0100
#00109:0001
";
        let mut warnings = Vec::new();

        let chart = parse_bms_text(text, &mut warnings).unwrap();

        assert_eq!(chart.metadata.title, "Test Song");
        assert_eq!(chart.metadata.artist, "Composer");
        assert_eq!(chart.metadata.initial_bpm, 150.0);
        assert_eq!(chart.resources.wavs.len(), 2);
        assert_eq!(chart.resources.bpm_table[0].bpm, 180.0);
        assert_eq!(chart.resources.stop_table[0].value, 192);
        assert!(chart.objects.iter().any(|object| matches!(
            object.kind,
            IntermediateObjectKind::VisibleNote { lane: Lane::Key1, wav_key: Some(1) }
        )));
        assert!(chart.objects.iter().any(|object| matches!(
            object.kind,
            IntermediateObjectKind::VisibleNote { lane: Lane::Key7, wav_key: Some(10) }
        )));
    }

    #[test]
    fn parses_measure_length_into_measure_ticks() {
        let text = "\
#BPM 120
#00102:0.5
#00111:01
#00211:01
";
        let mut warnings = Vec::new();

        let chart = parse_bms_text(text, &mut warnings).unwrap();

        assert_eq!(chart.measures[1].tick_len, TICKS_PER_MEASURE as u64 / 2);
        assert_eq!(
            chart.measures[2].start_tick,
            ChartTick(TICKS_PER_MEASURE as u64 / 2 + TICKS_PER_MEASURE as u64)
        );
    }
}

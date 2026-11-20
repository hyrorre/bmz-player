use std::io::{Read, Take};
use std::path::Path;

use anyhow::{Context, Result, bail};
use base64::Engine;
use base64::engine::general_purpose::{URL_SAFE, URL_SAFE_NO_PAD};
use bmz_core::clear::GaugeType;
use bmz_core::input::{InputDeviceKind, InputKind, ScratchDirection};
use bmz_core::lane::{KeyMode, LANE_COUNT, Lane};
use bmz_core::replay::ReplayEvent;
use bmz_core::time::TimeUs;
use bmz_gameplay::judge::model::JudgeAlgorithm;
use flate2::read::GzDecoder;
use serde::Deserialize;
use sha2::{Digest, Sha256};

use crate::ln_policy::LnScorePolicy;
use crate::screens::play_session::SRandomScheme;
use crate::select_options::{ArrangeOption, DoubleOption};

use super::replay::ReplayFile;

const MAX_OUTER_JSON_BYTES: u64 = 64 * 1024 * 1024;
const MAX_KEY_INPUT_BYTES: u64 = 16 * 1024 * 1024;
const KEY_INPUT_RECORD_BYTES: usize = 9;
const MAX_REPLAY_EVENTS: usize = MAX_KEY_INPUT_BYTES as usize / KEY_INPUT_RECORD_BYTES;
const RANDOM_SEED_MAX: i64 = (1 << 24) - 1;

#[derive(Debug, Clone, Deserialize)]
struct BeatorajaReplayData {
    #[serde(default)]
    sha256: String,
    #[serde(default)]
    mode: i64,
    #[serde(default)]
    keylog: Vec<LegacyKeyInput>,
    #[serde(default)]
    keyinput: Option<String>,
    #[serde(default)]
    gauge: i64,
    #[serde(default)]
    pattern: Option<Vec<serde_json::Value>>,
    #[serde(default, rename = "laneShufflePattern")]
    lane_shuffle_pattern: Option<Vec<Option<Vec<i64>>>>,
    #[serde(default)]
    rand: Vec<i32>,
    #[serde(default)]
    date: i64,
    #[serde(default, rename = "sevenToNinePattern")]
    seven_to_nine_pattern: i64,
    #[serde(default)]
    randomoption: i64,
    #[serde(default)]
    randomoptionseed: i64,
    #[serde(default)]
    randomoption2: i64,
    #[serde(default)]
    randomoption2seed: i64,
    #[serde(default)]
    doubleoption: i64,
    #[serde(default)]
    config: Option<BeatorajaPlayConfig>,
}

#[derive(Debug, Clone, Deserialize)]
struct BeatorajaPlayConfig {
    // libGDX omits fields with their default values, including Combo.
    #[serde(default)]
    judgetype: Option<String>,
}

impl BeatorajaPlayConfig {
    fn judge_algorithm(&self) -> Option<JudgeAlgorithm> {
        match self.judgetype.as_deref() {
            None | Some("Combo") => Some(JudgeAlgorithm::Combo),
            Some("Duration") => Some(JudgeAlgorithm::Duration),
            Some("Lowest") => Some(JudgeAlgorithm::Lowest),
            Some("Score") => Some(JudgeAlgorithm::Score),
            Some(_) => None,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
struct LegacyKeyInput {
    #[serde(default)]
    presstime: i64,
    #[serde(default)]
    keycode: i64,
    #[serde(default)]
    pressed: bool,
    #[serde(default)]
    time: i64,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum BeatorajaReplayDocumentData {
    Single(Box<BeatorajaReplayData>),
    Course(Vec<BeatorajaReplayData>),
}

impl LegacyKeyInput {
    fn time_us(&self) -> Result<i64> {
        if self.presstime != 0 {
            Ok(self.presstime)
        } else {
            self.time.checked_mul(1_000).context("legacy beatoraja key input time overflow")
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct BeatorajaReplayConversion {
    pub key_mode: KeyMode,
    pub ln_policy: LnScorePolicy,
    pub device_kind: InputDeviceKind,
    pub h_random_threshold_ms: Option<u32>,
}

#[derive(Debug, Clone)]
pub struct BeatorajaReplay {
    data: BeatorajaReplayData,
    key_inputs: Vec<DecodedKeyInput>,
}

#[derive(Debug, Clone)]
pub enum BeatorajaReplayDocument {
    Single(Box<BeatorajaReplay>),
    Course(Vec<BeatorajaReplay>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct DecodedKeyInput {
    time_us: i64,
    keycode: usize,
    pressed: bool,
}

pub fn load_beatoraja_replay(path: &Path) -> Result<BeatorajaReplay> {
    let (document, _) = load_beatoraja_replay_document_with_fingerprint(path)?;
    match document {
        BeatorajaReplayDocument::Single(replay) => Ok(*replay),
        BeatorajaReplayDocument::Course(replays) => {
            bail!("beatoraja course replay contains {} stages", replays.len())
        }
    }
}

pub(crate) fn load_beatoraja_replay_document_with_fingerprint(
    path: &Path,
) -> Result<(BeatorajaReplayDocument, String)> {
    let bytes = std::fs::read(path)
        .with_context(|| format!("failed to open beatoraja replay: {}", path.display()))?;
    let fingerprint = super::common::hash_to_hex(&Sha256::digest(&bytes));
    let replay = decode_beatoraja_replay_document(bytes.as_slice())
        .with_context(|| format!("failed to decode beatoraja replay: {}", path.display()))?;
    Ok((replay, fingerprint))
}

pub fn decode_beatoraja_replay(reader: impl Read) -> Result<BeatorajaReplay> {
    match decode_beatoraja_replay_document(reader)? {
        BeatorajaReplayDocument::Single(replay) => Ok(*replay),
        BeatorajaReplayDocument::Course(replays) => {
            bail!("beatoraja course replay contains {} stages", replays.len())
        }
    }
}

pub fn decode_beatoraja_replay_document(reader: impl Read) -> Result<BeatorajaReplayDocument> {
    let outer = read_gzip_limited(reader, MAX_OUTER_JSON_BYTES, "beatoraja replay JSON")?;
    let document: BeatorajaReplayDocumentData =
        serde_json::from_slice(&outer).context("invalid beatoraja replay JSON")?;

    match document {
        BeatorajaReplayDocumentData::Single(data) => {
            Ok(BeatorajaReplayDocument::Single(Box::new(decode_replay_data(*data)?)))
        }
        BeatorajaReplayDocumentData::Course(stages) => Ok(BeatorajaReplayDocument::Course(
            stages.into_iter().map(decode_replay_data).collect::<Result<Vec<_>>>()?,
        )),
    }
}

fn decode_replay_data(data: BeatorajaReplayData) -> Result<BeatorajaReplay> {
    if data.sha256.len() != 64 || !data.sha256.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        bail!("beatoraja replay has an invalid SHA256");
    }
    if data.pattern.as_ref().is_some_and(|pattern| !pattern.is_empty()) {
        bail!("legacy beatoraja PatternModifyLog replays are not supported");
    }
    if data.seven_to_nine_pattern != 0 {
        bail!("beatoraja 7-to-9 replay conversion is not supported");
    }

    let key_inputs = match data.keyinput.as_deref() {
        Some(encoded) if !encoded.is_empty() => decode_packed_key_input(encoded)?,
        _ => decode_legacy_key_input(&data.keylog)?,
    };
    validate_key_input_order(&key_inputs)?;

    Ok(BeatorajaReplay { data, key_inputs })
}

impl BeatorajaReplay {
    pub fn chart_sha256(&self) -> Result<[u8; 32]> {
        super::common::hex_to_hash::<32>(&self.data.sha256)
            .map_err(anyhow::Error::from)
            .context("invalid beatoraja replay SHA256")
    }

    pub const fn ln_mode(&self) -> i64 {
        self.data.mode
    }

    pub const fn played_at(&self) -> i64 {
        self.data.date
    }

    pub fn has_key_input(&self) -> bool {
        !self.key_inputs.is_empty()
    }

    pub fn to_replay_file(&self, conversion: BeatorajaReplayConversion) -> Result<ReplayFile> {
        let chart_sha256 = self.chart_sha256()?;
        let double_option = beatoraja_double_option(self.data.doubleoption)?
            .normalize_for_key_mode(conversion.key_mode);
        let event_key_mode = battle_key_mode(conversion.key_mode, double_option);
        ensure_supported_key_mode(event_key_mode)?;

        let arrange = beatoraja_arrange_option(self.data.randomoption, event_key_mode)?;
        let arrange_2p = if matches!(event_key_mode, KeyMode::K10 | KeyMode::K14) {
            beatoraja_arrange_option(self.data.randomoption2, event_key_mode)?
        } else {
            ArrangeOption::Normal
        };
        let arrange_seed = checked_seed(self.data.randomoptionseed, "1P")?;
        let arrange_seed_2p = if matches!(event_key_mode, KeyMode::K10 | KeyMode::K14) {
            Some(checked_seed(self.data.randomoption2seed, "2P")?)
        } else {
            None
        };
        let gauge_type = beatoraja_gauge_type(self.data.gauge)?;
        let events = self
            .key_inputs
            .iter()
            .map(|input| replay_event(*input, event_key_mode, conversion.device_kind))
            .filter_map(Result::transpose)
            .collect::<Result<Vec<_>>>()?;
        let lane_shuffle_pattern = replay_lane_shuffle_pattern(
            event_key_mode,
            arrange,
            arrange_2p,
            self.data.lane_shuffle_pattern.as_deref(),
        )?;
        let random_seed = pack_random_seed(arrange_seed, arrange_seed_2p)?;

        let mut file = ReplayFile::new_with_policy(
            chart_sha256,
            conversion.ln_policy,
            double_option.score_bucket(),
            self.data.date,
            Some(random_seed),
            arrange,
            arrange_2p,
            Some(arrange_seed),
            lane_shuffle_pattern,
            events,
        )
        .with_randomization(arrange_seed_2p, self.data.rand.clone(), Vec::new())
        .with_s_random_scheme(SRandomScheme::Legacy40MsV1)
        .with_playback_metadata(
            double_option,
            gauge_type,
            conversion.h_random_threshold_ms,
        );
        file.judge_algorithm =
            self.data.config.as_ref().and_then(BeatorajaPlayConfig::judge_algorithm);
        file.input_excludes_start_margin = true;
        file.judge_revision = 0;
        Ok(file)
    }
}

fn read_gzip_limited(reader: impl Read, limit: u64, description: &str) -> Result<Vec<u8>> {
    let decoder = GzDecoder::new(reader);
    let mut limited: Take<_> = decoder.take(limit + 1);
    let mut output = Vec::new();
    limited
        .read_to_end(&mut output)
        .with_context(|| format!("failed to decompress {description}"))?;
    if output.len() as u64 > limit {
        bail!("{description} exceeds the {limit}-byte limit");
    }
    Ok(output)
}

fn decode_packed_key_input(encoded: &str) -> Result<Vec<DecodedKeyInput>> {
    let compressed = URL_SAFE
        .decode(encoded)
        .or_else(|_| URL_SAFE_NO_PAD.decode(encoded))
        .context("invalid base64 in beatoraja keyinput")?;
    let bytes =
        read_gzip_limited(compressed.as_slice(), MAX_KEY_INPUT_BYTES, "beatoraja keyinput")?;
    if bytes.len() % KEY_INPUT_RECORD_BYTES != 0 {
        bail!("beatoraja keyinput has a truncated record");
    }

    let mut inputs = Vec::with_capacity(bytes.len() / KEY_INPUT_RECORD_BYTES);
    for record in bytes.as_chunks::<KEY_INPUT_RECORD_BYTES>().0 {
        let signed_keycode = record[0] as i8;
        if signed_keycode == 0 {
            bail!("beatoraja keyinput contains keycode zero");
        }
        let time_us = i64::from_le_bytes(record[1..].try_into().expect("eight-byte timestamp"));
        inputs.push(DecodedKeyInput {
            time_us,
            keycode: usize::from(signed_keycode.unsigned_abs() - 1),
            pressed: signed_keycode > 0,
        });
    }
    Ok(inputs)
}

fn decode_legacy_key_input(inputs: &[LegacyKeyInput]) -> Result<Vec<DecodedKeyInput>> {
    if inputs.len() > MAX_REPLAY_EVENTS {
        bail!("beatoraja legacy keylog contains too many events");
    }
    inputs
        .iter()
        .map(|input| {
            let time_us = input.time_us()?;
            if input.keycode < 0 {
                bail!("beatoraja legacy keylog contains an invalid event");
            }
            Ok(DecodedKeyInput {
                time_us,
                keycode: usize::try_from(input.keycode)
                    .context("beatoraja keycode does not fit usize")?,
                pressed: input.pressed,
            })
        })
        .collect()
}

fn validate_key_input_order(inputs: &[DecodedKeyInput]) -> Result<()> {
    if inputs.windows(2).any(|pair| pair[0].time_us > pair[1].time_us) {
        bail!("beatoraja key input is not ordered by timestamp");
    }
    Ok(())
}

fn ensure_supported_key_mode(key_mode: KeyMode) -> Result<()> {
    if !matches!(
        key_mode,
        KeyMode::K5 | KeyMode::K6 | KeyMode::K7 | KeyMode::K9 | KeyMode::K10 | KeyMode::K14
    ) {
        bail!("beatoraja replay key mode {} is not supported", key_mode.as_str());
    }
    Ok(())
}

fn battle_key_mode(key_mode: KeyMode, double_option: DoubleOption) -> KeyMode {
    match (key_mode, double_option) {
        (KeyMode::K5, DoubleOption::Battle | DoubleOption::BattleAutoScratch) => KeyMode::K10,
        (KeyMode::K7, DoubleOption::Battle | DoubleOption::BattleAutoScratch) => KeyMode::K14,
        _ => key_mode,
    }
}

fn beatoraja_double_option(value: i64) -> Result<DoubleOption> {
    match value {
        0 => Ok(DoubleOption::Off),
        1 => Ok(DoubleOption::Flip),
        2 => Ok(DoubleOption::Battle),
        3 => Ok(DoubleOption::BattleAutoScratch),
        _ => bail!("unknown beatoraja double option: {value}"),
    }
}

fn beatoraja_arrange_option(value: i64, key_mode: KeyMode) -> Result<ArrangeOption> {
    if key_mode == KeyMode::K9 && matches!(value, 4 | 7 | 8 | 9) {
        bail!("beatoraja PMS random option {value} has no exact BMZ replay representation");
    }
    match value {
        0 => Ok(ArrangeOption::Normal),
        1 => Ok(ArrangeOption::Mirror),
        2 => Ok(ArrangeOption::Random),
        3 => Ok(ArrangeOption::RRandom),
        4 => Ok(ArrangeOption::SRandom),
        5 => Ok(ArrangeOption::Spiral),
        6 => Ok(ArrangeOption::HRandom),
        7 => Ok(ArrangeOption::AllScratch),
        8 => Ok(ArrangeOption::RandomEx),
        9 => Ok(ArrangeOption::SRandomEx),
        _ => bail!("unknown beatoraja random option: {value}"),
    }
}

fn beatoraja_gauge_type(value: i64) -> Result<GaugeType> {
    match value {
        0 => Ok(GaugeType::AssistEasy),
        1 => Ok(GaugeType::Easy),
        2 => Ok(GaugeType::Normal),
        3 => Ok(GaugeType::Hard),
        4 => Ok(GaugeType::ExHard),
        5 => Ok(GaugeType::Hazard),
        6 => Ok(GaugeType::Class),
        7 => Ok(GaugeType::ExClass),
        8 => Ok(GaugeType::ExHardClass),
        _ => bail!("unknown beatoraja gauge type: {value}"),
    }
}

fn checked_seed(value: i64, side: &str) -> Result<i64> {
    if !(0..=RANDOM_SEED_MAX).contains(&value) {
        bail!("beatoraja {side} random seed is outside the 24-bit range: {value}");
    }
    Ok(value)
}

fn pack_random_seed(seed_1p: i64, seed_2p: Option<i64>) -> Result<i64> {
    let packed = i128::from(seed_1p) + i128::from(seed_2p.unwrap_or(0)) * (1_i128 << 24);
    i64::try_from(packed).context("packed beatoraja random seed overflow")
}

fn replay_event(
    input: DecodedKeyInput,
    key_mode: KeyMode,
    device_kind: InputDeviceKind,
) -> Result<Option<ReplayEvent>> {
    let Some((lane, scratch_direction)) = beatoraja_key_lane(key_mode, input.keycode)
        .with_context(|| format!("unsupported beatoraja keycode {}", input.keycode))?
    else {
        return Ok(None);
    };
    Ok(Some(ReplayEvent {
        lane,
        kind: if input.pressed { InputKind::Press } else { InputKind::Release },
        time: TimeUs(input.time_us),
        device_kind,
        scratch_direction,
    }))
}

fn beatoraja_key_lane(
    key_mode: KeyMode,
    keycode: usize,
) -> Result<Option<(Lane, Option<ScratchDirection>)>> {
    let mapped = match key_mode {
        KeyMode::K5 => match keycode {
            0..=4 => (key_lane(1 + keycode)?, None),
            5 => (Lane::Scratch, Some(ScratchDirection::Up)),
            6 => (Lane::Scratch, Some(ScratchDirection::Down)),
            _ => bail!("keycode outside 5K input range"),
        },
        KeyMode::K6 => match keycode {
            0..=2 => (key_lane(1 + keycode)?, None),
            4..=6 => (key_lane(keycode)?, None),
            // U_E 6K omits the center 7K key and both scratch directions.
            3 | 7 | 8 => return Ok(None),
            _ => bail!("keycode outside 6K input range"),
        },
        KeyMode::K7 => match keycode {
            0..=6 => (key_lane(1 + keycode)?, None),
            7 => (Lane::Scratch, Some(ScratchDirection::Up)),
            8 => (Lane::Scratch, Some(ScratchDirection::Down)),
            _ => bail!("keycode outside 7K input range"),
        },
        KeyMode::K9 => match keycode {
            0..=8 => (key_lane(1 + keycode)?, None),
            _ => bail!("keycode outside 9K input range"),
        },
        KeyMode::K10 => match keycode {
            0..=4 => (key_lane(1 + keycode)?, None),
            5 => (Lane::Scratch, Some(ScratchDirection::Up)),
            6 => (Lane::Scratch, Some(ScratchDirection::Down)),
            7..=11 => (key_lane(8 + keycode - 7)?, None),
            12 => (Lane::Scratch2, Some(ScratchDirection::Up)),
            13 => (Lane::Scratch2, Some(ScratchDirection::Down)),
            _ => bail!("keycode outside 10K input range"),
        },
        KeyMode::K14 => match keycode {
            0..=6 => (key_lane(1 + keycode)?, None),
            7 => (Lane::Scratch, Some(ScratchDirection::Up)),
            8 => (Lane::Scratch, Some(ScratchDirection::Down)),
            9..=15 => (key_lane(8 + keycode - 9)?, None),
            16 => (Lane::Scratch2, Some(ScratchDirection::Up)),
            17 => (Lane::Scratch2, Some(ScratchDirection::Down)),
            _ => bail!("keycode outside 14K input range"),
        },
        _ => bail!("unsupported beatoraja key mode"),
    };
    Ok(Some(mapped))
}

fn key_lane(number: usize) -> Result<Lane> {
    Lane::ALL
        .iter()
        .copied()
        .find(|lane| lane.index() == number)
        .context("key lane outside BMZ lane range")
}

fn replay_lane_shuffle_pattern(
    key_mode: KeyMode,
    arrange: ArrangeOption,
    arrange_2p: ArrangeOption,
    rows: Option<&[Option<Vec<i64>>]>,
) -> Result<Option<Vec<u8>>> {
    let players = if matches!(key_mode, KeyMode::K10 | KeyMode::K14) { 2 } else { 1 };
    let arrangements = [arrange, arrange_2p];
    if arrangements[..players].iter().any(|option| !is_lane_static(*option)) {
        return Ok(None);
    }
    if arrangements[..players].iter().all(|option| *option == ArrangeOption::Normal)
        && rows.is_none_or(|rows| rows.iter().take(players).all(Option::is_none))
    {
        return Ok(None);
    }
    let side_lanes = beatoraja_side_lane_count(key_mode)?;

    let mut pattern: Vec<u8> = (0..LANE_COUNT as u8).collect();
    let mut copied_recorded_pattern = false;
    for (side, &side_arrange) in arrangements.iter().enumerate().take(players) {
        let row = rows.and_then(|rows| rows.get(side)).and_then(Option::as_deref);
        if let Some(row) = row {
            if row.len() != side_lanes {
                bail!("beatoraja laneShufflePattern has an invalid side length");
            }
            for (local_destination, &source_raw) in row.iter().enumerate() {
                let destination_raw = side * side_lanes + local_destination;
                let source_raw = usize::try_from(source_raw)
                    .context("negative beatoraja laneShufflePattern entry")?;
                if source_raw / side_lanes != side {
                    bail!("beatoraja laneShufflePattern crosses player sides");
                }
                let destination = beatoraja_chart_lane(key_mode, destination_raw)?;
                let source = beatoraja_chart_lane(key_mode, source_raw)?;
                pattern[destination.index()] = source.index() as u8;
            }
            copied_recorded_pattern = true;
        } else if side_arrange == ArrangeOption::Mirror {
            apply_mirror_side_to_pattern(&mut pattern, key_mode, side)?;
        } else if !matches!(side_arrange, ArrangeOption::Normal) {
            return Ok(None);
        }
    }
    Ok(copied_recorded_pattern.then_some(pattern))
}

fn is_lane_static(option: ArrangeOption) -> bool {
    matches!(
        option,
        ArrangeOption::Normal
            | ArrangeOption::Mirror
            | ArrangeOption::Random
            | ArrangeOption::RRandom
            | ArrangeOption::RandomEx
    )
}

fn beatoraja_side_lane_count(key_mode: KeyMode) -> Result<usize> {
    match key_mode {
        KeyMode::K5 => Ok(6),
        KeyMode::K7 => Ok(8),
        KeyMode::K9 => Ok(9),
        KeyMode::K10 => Ok(6),
        KeyMode::K14 => Ok(8),
        _ => bail!("unsupported beatoraja key mode"),
    }
}

fn beatoraja_chart_lane(key_mode: KeyMode, raw_lane: usize) -> Result<Lane> {
    let lane = match key_mode {
        KeyMode::K5 => match raw_lane {
            0..=4 => key_lane(1 + raw_lane)?,
            5 => Lane::Scratch,
            _ => bail!("lane outside 5K range"),
        },
        KeyMode::K7 => match raw_lane {
            0..=6 => key_lane(1 + raw_lane)?,
            7 => Lane::Scratch,
            _ => bail!("lane outside 7K range"),
        },
        KeyMode::K9 => match raw_lane {
            0..=8 => key_lane(1 + raw_lane)?,
            _ => bail!("lane outside 9K range"),
        },
        KeyMode::K10 => match raw_lane {
            0..=4 => key_lane(1 + raw_lane)?,
            5 => Lane::Scratch,
            6..=10 => key_lane(8 + raw_lane - 6)?,
            11 => Lane::Scratch2,
            _ => bail!("lane outside 10K range"),
        },
        KeyMode::K14 => match raw_lane {
            0..=6 => key_lane(1 + raw_lane)?,
            7 => Lane::Scratch,
            8..=14 => key_lane(8 + raw_lane - 8)?,
            15 => Lane::Scratch2,
            _ => bail!("lane outside 14K range"),
        },
        _ => bail!("unsupported beatoraja key mode"),
    };
    Ok(lane)
}

fn apply_mirror_side_to_pattern(pattern: &mut [u8], key_mode: KeyMode, side: usize) -> Result<()> {
    let side_lanes = beatoraja_side_lane_count(key_mode)?;
    let has_scratch = key_mode != KeyMode::K9;
    let key_count = side_lanes - usize::from(has_scratch);
    for local_destination in 0..key_count {
        let destination_raw = side * side_lanes + local_destination;
        let source_raw = side * side_lanes + (key_count - 1 - local_destination);
        let destination = beatoraja_chart_lane(key_mode, destination_raw)?;
        let source = beatoraja_chart_lane(key_mode, source_raw)?;
        pattern[destination.index()] = source.index() as u8;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use base64::engine::general_purpose::URL_SAFE;
    use flate2::Compression;
    use flate2::write::GzEncoder;
    use serde_json::json;

    use super::*;

    fn gzip(bytes: &[u8]) -> Vec<u8> {
        let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(bytes).unwrap();
        encoder.finish().unwrap()
    }

    fn packed_keyinput(records: &[(i8, i64)]) -> String {
        let mut bytes = Vec::new();
        for &(keycode, time) in records {
            bytes.push(keycode as u8);
            bytes.extend_from_slice(&time.to_le_bytes());
        }
        URL_SAFE.encode(gzip(&bytes))
    }

    fn decode_json(value: serde_json::Value) -> BeatorajaReplay {
        decode_beatoraja_replay(gzip(value.to_string().as_bytes()).as_slice()).unwrap()
    }

    fn base_json() -> serde_json::Value {
        json!({
            "sha256": "0707070707070707070707070707070707070707070707070707070707070707",
            "mode": 1,
            "keyinput": packed_keyinput(&[(1, 10_000), (-1, 20_000)]),
            "gauge": 3,
            "rand": [2, 1],
            "date": 1_700_000_000_i64,
            "randomoption": 0,
            "randomoptionseed": 42,
            "randomoption2": 0,
            "randomoption2seed": 84,
            "doubleoption": 0
        })
    }

    fn conversion(key_mode: KeyMode) -> BeatorajaReplayConversion {
        BeatorajaReplayConversion {
            key_mode,
            ln_policy: LnScorePolicy::AutoCn,
            device_kind: InputDeviceKind::Controller,
            h_random_threshold_ms: Some(125),
        }
    }

    #[test]
    fn packed_keyinput_converts_to_bmz_replay_metadata() {
        let replay = decode_json(base_json());
        let converted = replay.to_replay_file(conversion(KeyMode::K7)).unwrap();

        assert_eq!(converted.chart_sha256_bytes().unwrap(), [7; 32]);
        assert_eq!(converted.ln_policy, "AutoCn");
        assert_eq!(converted.recorded_gauge_type(), Some(GaugeType::Hard));
        assert_eq!(converted.h_random_threshold_ms, Some(125));
        assert_eq!(converted.bms_random_choices, Some(vec![2, 1]));
        assert_eq!(converted.events.len(), 2);
        assert_eq!(converted.events[0].lane, Lane::Key1);
        assert_eq!(converted.events[0].kind, InputKind::Press);
        assert_eq!(converted.events[0].time, TimeUs(10_000));
        assert_eq!(converted.events[0].device_kind, InputDeviceKind::Controller);
        assert_eq!(converted.events[1].kind, InputKind::Release);
        assert_eq!(converted.effective_s_random_scheme().unwrap(), SRandomScheme::Legacy40MsV1);
        assert!(converted.input_excludes_start_margin);
        assert_eq!(converted.judge_algorithm, None);
    }

    #[test]
    fn judge_algorithm_uses_recorded_judgetype_and_libgdx_default() {
        for (config, expected) in [
            (json!({}), Some(JudgeAlgorithm::Combo)),
            (json!({"judgetype": "Duration"}), Some(JudgeAlgorithm::Duration)),
            (json!({"judgetype": "Lowest"}), Some(JudgeAlgorithm::Lowest)),
            (json!({"judgetype": "Score"}), Some(JudgeAlgorithm::Score)),
            (json!({"judgetype": "Unknown"}), None),
        ] {
            let mut value = base_json();
            value["config"] = config;
            let converted = decode_json(value).to_replay_file(conversion(KeyMode::K7)).unwrap();
            assert_eq!(converted.judge_algorithm, expected);
        }
    }

    #[test]
    fn scratch_keycodes_preserve_direction() {
        let mut value = base_json();
        value["keyinput"] =
            json!(packed_keyinput(&[(8, 1_000), (-8, 2_000), (9, 3_000), (-9, 4_000),]));
        let converted = decode_json(value).to_replay_file(conversion(KeyMode::K7)).unwrap();

        assert!(converted.events.iter().all(|event| event.lane == Lane::Scratch));
        assert_eq!(converted.events[0].scratch_direction, Some(ScratchDirection::Up));
        assert_eq!(converted.events[2].scratch_direction, Some(ScratchDirection::Down));
    }

    #[test]
    fn legacy_millisecond_keylog_is_supported() {
        let mut value = base_json();
        value.as_object_mut().unwrap().remove("keyinput");
        value["keylog"] = json!([
            {"presstime": 0, "time": 12, "keycode": 0, "pressed": true},
            {"presstime": 13_500, "time": 0, "keycode": 0, "pressed": false}
        ]);
        let converted = decode_json(value).to_replay_file(conversion(KeyMode::K5)).unwrap();

        assert_eq!(converted.events[0].time, TimeUs(12_000));
        assert_eq!(converted.events[1].time, TimeUs(13_500));
    }

    #[test]
    fn leading_negative_timestamps_are_preserved() {
        let mut value = base_json();
        value["keyinput"] = json!(packed_keyinput(&[(1, -50_000), (-1, -10_000), (1, 1_000)]));
        let converted = decode_json(value).to_replay_file(conversion(KeyMode::K7)).unwrap();

        assert_eq!(converted.events[0].time, TimeUs(-50_000));
        assert_eq!(converted.events[1].time, TimeUs(-10_000));
        assert_eq!(converted.events[2].time, TimeUs(1_000));
    }

    #[test]
    fn empty_keyinput_is_a_valid_empty_replay() {
        let mut value = base_json();
        value["keyinput"] = json!(packed_keyinput(&[]));
        let replay = decode_json(value);

        assert!(!replay.has_key_input());
        assert!(replay.to_replay_file(conversion(KeyMode::K7)).unwrap().events.is_empty());
    }

    #[test]
    fn six_key_replay_maps_ue_lanes_and_ignores_inactive_inputs() {
        let mut value = base_json();
        value["keyinput"] = json!(packed_keyinput(
            &(1_i8..=9).enumerate().map(|(i, key)| (key, i as i64 * 1_000)).collect::<Vec<_>>()
        ));
        let converted = decode_json(value).to_replay_file(conversion(KeyMode::K6)).unwrap();

        assert_eq!(
            converted.events.iter().map(|event| event.lane).collect::<Vec<_>>(),
            vec![Lane::Key1, Lane::Key2, Lane::Key3, Lane::Key4, Lane::Key5, Lane::Key6,]
        );
    }

    #[test]
    fn dp_lane_shuffle_rows_become_one_bmz_permutation() {
        let mut value = base_json();
        value["randomoption"] = json!(2);
        value["randomoption2"] = json!(2);
        value["laneShufflePattern"] =
            json!([[1, 0, 2, 3, 4, 5, 6, 7], [8, 9, 10, 11, 12, 13, 15, 14]]);
        let converted = decode_json(value).to_replay_file(conversion(KeyMode::K14)).unwrap();
        let pattern = converted.lane_shuffle_pattern.unwrap();

        assert_eq!(pattern[Lane::Key1.index()], Lane::Key2.index() as u8);
        assert_eq!(pattern[Lane::Key2.index()], Lane::Key1.index() as u8);
        assert_eq!(pattern[Lane::Key14.index()], Lane::Scratch2.index() as u8);
        assert_eq!(pattern[Lane::Scratch2.index()], Lane::Key14.index() as u8);
    }

    #[test]
    fn battle_replay_uses_expanded_input_layout() {
        let mut value = base_json();
        value["doubleoption"] = json!(2);
        value["keyinput"] = json!(packed_keyinput(&[(8, 1_000), (-8, 2_000)]));
        let converted = decode_json(value).to_replay_file(conversion(KeyMode::K5)).unwrap();

        assert_eq!(converted.double_option(), DoubleOption::Battle);
        assert_eq!(converted.events[0].lane, Lane::Key8);
    }

    #[test]
    fn malformed_and_unsupported_replays_are_rejected() {
        let mut unordered = base_json();
        unordered["keyinput"] = json!(packed_keyinput(&[(1, 2_000), (-1, 1_000)]));
        assert!(
            decode_beatoraja_replay(gzip(unordered.to_string().as_bytes()).as_slice()).is_err()
        );

        let mut old_pattern = base_json();
        old_pattern["pattern"] = json!([{"section": 0.0, "modify": [0]}]);
        assert!(
            decode_beatoraja_replay(gzip(old_pattern.to_string().as_bytes()).as_slice()).is_err()
        );

        let mut pms = base_json();
        pms["randomoption"] = json!(8);
        let replay = decode_json(pms);
        assert!(replay.to_replay_file(conversion(KeyMode::K9)).is_err());
    }
}

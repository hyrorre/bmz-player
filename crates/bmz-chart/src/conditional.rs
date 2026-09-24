//! KaleidBeat の実行時分岐。構文解析はロード時、採択後は中間譜面を再正規化する。
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::Arc,
};

use crate::{
    import::{error::ImportError, intermediate::*, normalize::normalize_chart},
    model::*,
    timing::{IMPORT_TICK_SCALE, TICKS_PER_MEASURE},
};

use bmz_core::{
    ids::{NoteId, SoundId},
    time::{ChartTick, TimeUs},
};

pub type BgmKey = (u32, u32, u32, u64, u32);

/// Preserve BGM provenance through options that move notes to/from BGM lanes.
pub struct ConditionalAudioKeys(BTreeMap<(u64, u32), std::collections::VecDeque<BgmKey>>);

impl ConditionalAudioKeys {
    pub fn capture(chart: &PlayableChart) -> Self {
        let mut keys: BTreeMap<_, std::collections::VecDeque<_>> = BTreeMap::new();
        for (event, key) in chart.bgm_events.iter().zip(&chart.metadata.conditional_bgm_keys) {
            keys.entry((event.tick.0, event.sound.0)).or_default().push_back(*key);
        }
        for note in chart.lane_notes.iter().flatten() {
            for (layer, sound) in note.sounds().enumerate() {
                let origin = (1u64 << 63) | (u64::from(note.id.0) << 32) | u64::from(sound.0);
                keys.entry((note.tick.0, sound.0)).or_default().push_back((
                    0,
                    0,
                    1,
                    origin,
                    layer as u32,
                ));
            }
        }
        Self(keys)
    }

    pub fn apply(mut self, chart: &mut PlayableChart) {
        chart.metadata.conditional_bgm_keys = chart
            .bgm_events
            .iter()
            .map(|event| {
                self.0
                    .get_mut(&(event.tick.0, event.sound.0))
                    .and_then(|keys| keys.pop_front())
                    .unwrap_or((u32::MAX, event.tick.0 as u32, 1, u64::from(event.sound.0), 0))
            })
            .collect();
    }
}

#[derive(Debug, Clone)]
pub struct Condition {
    pub variable: String,
    pub operator: String,
    pub value: f64,
}

impl Condition {
    fn parse(text: &str) -> Result<Self, ImportError> {
        for op in [">=", "<=", "==", "!=", ">", "<"] {
            if let Some((name, value)) = text.split_once(op) {
                let variable = name.trim().to_ascii_uppercase();
                if ![
                    "SCORE", "RATE", "COMBO", "MAXCOMBO", "PGREAT", "GREAT", "GOOD", "BAD", "POOR",
                    "MISS", "GAUGE", "LAMP", "AUTO",
                ]
                .contains(&variable.as_str())
                {
                    return Err(invalid("unknown CONDITIONAL variable"));
                }
                let value: f64 = value.trim().parse().map_err(|_| invalid("invalid WHEN value"))?;
                if !value.is_finite() {
                    return Err(invalid("non-finite WHEN value"));
                }
                return Ok(Self { variable, operator: op.into(), value });
            }
        }
        Err(invalid("WHEN requires a comparison"))
    }
    pub fn matches(&self, value: f64) -> bool {
        match self.operator.as_str() {
            ">=" => value >= self.value,
            "<=" => value <= self.value,
            "==" => value == self.value,
            "!=" => value != self.value,
            ">" => value > self.value,
            "<" => value < self.value,
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    fn import(text: &str) -> Result<PlayableChart, ImportError> {
        let mut file = tempfile::NamedTempFile::new().unwrap();
        file.write_all(text.as_bytes()).unwrap();
        Ok(crate::import::import_chart(file.path(), Some(1), false)?.chart)
    }
    const HEAD: &str = "#BPM 120\n#WAV01 a.wav\n#WAV02 b.wav\n";

    #[test]
    fn default_counts_and_all_assets_with_stable_notes() {
        let chart = import(&format!("{HEAD}#00111:01\n#CONDITIONAL 2\n#WHEN SCORE>=2\n#00311:0202\n#DEFAULT\n#00311:01\n#ENDCONDITIONAL\n")).unwrap();
        assert_eq!(chart.total_notes, 2);
        assert_eq!(chart.sounds.len(), 2);
        let program = chart.metadata.conditional.as_ref().unwrap();
        let selected = program.materialize(&[Some(0)]).unwrap();
        assert_eq!(selected.total_notes, 3);
        assert_eq!(selected.lane_notes[1][0].id, chart.lane_notes[1][0].id);
        assert_eq!(program.evaluation_time(0, &chart), TimeUs(4_000_000));
    }

    #[test]
    fn distinct_subtick_notes_keep_distinct_ids() {
        let data = format!("0101{}", "00".repeat(8190));
        let chart = import(&format!("{HEAD}#CONDITIONAL 0\n#WHEN AUTO==1\n#00111:{data}\n#DEFAULT\n#00111:{data}\n#ENDCONDITIONAL\n")).unwrap();
        let notes = &chart.lane_notes[bmz_core::lane::Lane::Key1.index()];
        assert_eq!(notes.len(), 2);
        assert_eq!(notes[0].tick, notes[1].tick);
        assert_ne!(notes[0].id, notes[1].id);
        assert_ne!(notes[0].time, notes[1].time);
    }

    #[test]
    fn fractional_evaluation_keeps_import_time_precision_and_start_margin() {
        let data = format!("0101{}", "00".repeat(8190));
        let mut chart = import(&format!("{HEAD}#00011:01\n#00111:{data}\n#CONDITIONAL 1.0001220703125\n#WHEN AUTO==1\n#00211:01\n#DEFAULT\n#ENDCONDITIONAL\n")).unwrap();
        crate::start_margin::apply_start_note_margin(&mut chart);
        let program = chart.metadata.conditional.as_ref().unwrap();
        assert_eq!(program.evaluation_time(0, &chart), TimeUs(3_000_244));
        let selected = program.materialize(&[Some(0)]).unwrap();
        assert_eq!(program.evaluation_time(0, &selected), TimeUs(3_000_244));
    }

    #[test]
    fn bpm_stop_and_future_measure_length_retime_following_branch() {
        let chart = import(&format!("{HEAD}#BPM01 240\n#STOP01 192\n#CONDITIONAL 1\n#WHEN AUTO==1\n#00108:01\n#00109:01\n#00202:0.5\n#DEFAULT\n#ENDCONDITIONAL\n#CONDITIONAL 3\n#WHEN AUTO==1\n#00411:02\n#DEFAULT\n#00411:01\n#ENDCONDITIONAL\n")).unwrap();
        let p = chart.metadata.conditional.as_ref().unwrap();
        assert_eq!(p.evaluation_time(1, &chart), TimeUs(6_000_000));
        let selected = p.materialize(&[Some(0), None]).unwrap();
        assert_eq!(p.evaluation_time(0, &selected), TimeUs(2_000_000));
        assert_eq!(p.evaluation_time(1, &selected), TimeUs(4_500_000));
        assert_eq!(selected.lane_notes.iter().flatten().next().unwrap().time, TimeUs(5_500_000));
    }

    #[test]
    fn ln_can_link_common_head_to_branch_end() {
        let chart = import(&format!("{HEAD}#LNOBJ ZZ\n#00011:01\n#CONDITIONAL 1\n#WHEN AUTO==1\n#00211:ZZ\n#DEFAULT\n#00311:ZZ\n#ENDCONDITIONAL\n")).unwrap();
        assert_eq!(chart.long_notes.len(), 1);
        let selected =
            chart.metadata.conditional.as_ref().unwrap().materialize(&[Some(0)]).unwrap();
        assert_eq!(selected.long_notes[0].start_note_id, chart.long_notes[0].start_note_id);
        assert_eq!(selected.long_notes[0].end_time, TimeUs(4_000_000));
    }

    #[test]
    fn hln_alternatives_keep_distinct_preloaded_voices_for_same_position() {
        let chart = import(&format!("{HEAD}#LNMODE 4\n#CONDITIONAL 1\n#WHEN AUTO==1\n#00251:02\n#00351:02\n#DEFAULT\n#00251:01\n#00351:01\n#ENDCONDITIONAL\n")).unwrap();
        let selected =
            chart.metadata.conditional.as_ref().unwrap().materialize(&[Some(0)]).unwrap();
        let old = chart.long_notes[0].sound.unwrap();
        let new = selected.long_notes[0].sound.unwrap();
        assert_ne!(old, new);
        assert!(chart.sounds.iter().find(|a| a.id == old).unwrap().path.ends_with("a.wav"));
        assert!(chart.sounds.iter().find(|a| a.id == new).unwrap().path.ends_with("b.wav"));
    }

    #[test]
    fn hln_and_bga_resources_cover_combinations_of_independent_blocks() {
        let chart = import(&format!("{HEAD}#HLNOBJ ZZ\n#BMP01 branch.mpg\n#CONDITIONAL 0\n#WHEN AUTO==1\n#00111:01\n#DEFAULT\n#ENDCONDITIONAL\n#CONDITIONAL 2\n#WHEN AUTO==1\n#00311:ZZ\n#00304:01\n#DEFAULT\n#ENDCONDITIONAL\n")).unwrap();
        assert!(chart.metadata.has_bga);
        let selected =
            chart.metadata.conditional.as_ref().unwrap().materialize(&[Some(0), Some(0)]).unwrap();
        let sound = selected.long_notes[0].sound.unwrap();
        assert!(
            chart.sounds.iter().any(|asset| asset.id == sound && asset.path.ends_with("a.wav"))
        );
    }

    #[test]
    fn rejects_unsafe_and_malformed_branches() {
        for body in [
            "#CONDITIONAL 1\n#WHEN AUTO==1\n#00011:01\n#DEFAULT\n#ENDCONDITIONAL",
            "#CONDITIONAL 1.5\n#WHEN AUTO==1\n#00102:0.5\n#DEFAULT\n#ENDCONDITIONAL",
            "#CONDITIONAL 1\n#WHEN AUTO==1\n#WAV01 other.wav\n#DEFAULT\n#ENDCONDITIONAL",
            "#CONDITIONAL 1\n#WHEN AUTO==1\n#00211:01\n#ENDCONDITIONAL",
            "#CONDITIONAL 1\n#WHEN TYPO==1\n#DEFAULT\n#ENDCONDITIONAL",
            "#WHEN AUTO==1\n#00211:01\n#DEFAULT\n#ENDCONDITIONAL",
            "#DEFAULT\n#00211:01",
        ] {
            assert!(import(&format!("{HEAD}{body}\n")).is_err(), "{body}");
        }
    }
}

#[derive(Debug, Clone)]
pub struct ConditionalBlock {
    pub position: f64,
    pub conditions: Vec<Condition>,
    branches: Vec<IntermediateChart>,
    lengths: Vec<BTreeMap<u32, f64>>,
}

type ChartTransform = dyn Fn(&mut PlayableChart) + Send + Sync;

#[derive(Clone)]
pub struct ConditionalProgram {
    pub blocks: Vec<ConditionalBlock>,
    common: IntermediateChart,
    common_chunks: Vec<(IntermediateChart, BTreeMap<u32, f64>)>,
    source: PathBuf,
    positions: BTreeMap<(usize, u32, u32, u32), u32>,
    voice_ids: BTreeMap<(u32, u32, usize), SoundId>,
    pub margin: TimeUs,
    pub transform: Option<Arc<ChartTransform>>,
}

impl std::fmt::Debug for ConditionalProgram {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ConditionalProgram")
            .field("blocks", &self.blocks.len())
            .finish_non_exhaustive()
    }
}

fn invalid(message: &str) -> ImportError {
    ImportError::InvalidChart { message: format!("CONDITIONAL: {message}") }
}

fn channel(line: &str) -> Option<(u32, &str, &str)> {
    let (head, data) = line.strip_prefix('#')?.split_once(':')?;
    if head.len() != 5 || !head.is_ascii() {
        return None;
    }
    Some((head[..3].parse().ok()?, &head[3..], data))
}

struct SourceBlock {
    position: f64,
    conditions: Vec<Condition>,
    branches: Vec<String>,
    default: bool,
}

fn directive(line: &str) -> (&str, &str) {
    let command = line.trim().strip_prefix('#').unwrap_or("").trim_start();
    command.split_once(char::is_whitespace).unwrap_or((command, ""))
}

/// Headers are global; branches contain channel rows only. Malformed blocks fail closed.
pub(crate) fn compile(
    text: &str,
    source: &Path,
    mut parse: impl FnMut(&str) -> Result<IntermediateChart, ImportError>,
) -> Result<IntermediateChart, ImportError> {
    if !text.lines().any(|line| {
        matches!(
            directive(line).0.to_ascii_uppercase().as_str(),
            "CONDITIONAL" | "WHEN" | "ELSEWHEN" | "DEFAULT" | "ENDCONDITIONAL"
        )
    }) {
        return parse(text);
    }
    let mut headers = String::new();
    let mut common_rows = String::new();
    let mut common_segments = Vec::new();
    let mut blocks = Vec::<SourceBlock>::new();
    let mut current: Option<SourceBlock> = None;
    for raw in text.lines() {
        let line = raw.trim();
        let (command, value) = directive(line);
        match command.to_ascii_uppercase().as_str() {
            "CONDITIONAL" => {
                if current.is_some() {
                    return Err(invalid("nested blocks are unsupported"));
                }
                common_segments.push(std::mem::take(&mut common_rows));
                let position: f64 =
                    value.trim().parse().map_err(|_| invalid("invalid evaluation position"))?;
                if !position.is_finite() || !(0.0..=999.999999).contains(&position) {
                    return Err(invalid("evaluation position out of range"));
                }
                current = Some(SourceBlock {
                    position,
                    conditions: Vec::new(),
                    branches: Vec::new(),
                    default: false,
                });
            }
            "WHEN" | "ELSEWHEN" => {
                let block = current.as_mut().ok_or_else(|| invalid("WHEN outside block"))?;
                if block.default
                    || (command.eq_ignore_ascii_case("WHEN") != block.branches.is_empty())
                {
                    return Err(invalid("invalid WHEN/ELSEWHEN order"));
                }
                block.conditions.push(Condition::parse(value)?);
                block.branches.push(String::new());
            }
            "DEFAULT" => {
                let block = current.as_mut().ok_or_else(|| invalid("DEFAULT outside block"))?;
                if block.default || block.branches.is_empty() {
                    return Err(invalid("invalid DEFAULT"));
                }
                block.default = true;
                block.branches.push(String::new());
            }
            "ENDCONDITIONAL" => {
                let block = current.take().ok_or_else(|| invalid("unmatched ENDCONDITIONAL"))?;
                if !block.default {
                    return Err(invalid("DEFAULT is required"));
                }
                blocks.push(block);
            }
            _ => {
                if let Some(block) = &mut current {
                    if line.is_empty() || !line.starts_with('#') {
                        continue;
                    }
                    let (measure, ch, data) = channel(line)
                        .ok_or_else(|| invalid("only channel rows are allowed inside a branch"))?;
                    if ch == "02" {
                        if f64::from(measure) < block.position {
                            return Err(invalid(
                                "cannot change a measure that has already started",
                            ));
                        }
                    } else {
                        if data.len() % 2 != 0 || !data.is_ascii() {
                            return Err(invalid("invalid channel data"));
                        }
                        let count = data.len() / 2;
                        for (i, obj) in data.as_bytes().as_chunks::<2>().0.iter().enumerate() {
                            if obj != b"00"
                                && f64::from(measure) + i as f64 / (count as f64) < block.position
                            {
                                return Err(invalid(
                                    "branch changes an event before its evaluation position",
                                ));
                            }
                        }
                    }
                    let rows =
                        block.branches.last_mut().ok_or_else(|| invalid("channel before WHEN"))?;
                    rows.push_str(raw);
                    rows.push('\n');
                } else if channel(line).is_some() {
                    common_rows.push_str(raw);
                    common_rows.push('\n');
                } else {
                    headers.push_str(raw);
                    headers.push('\n');
                }
            }
        }
    }
    if current.is_some() {
        return Err(invalid("unterminated block"));
    }
    common_segments.push(common_rows);
    let mut common = parse(&format!("{headers}{}", common_segments.concat()))?;
    resolve_stops(&mut common);
    let mut common_chunks = Vec::new();
    for rows in common_segments {
        let mut chunk = parse(&format!("{headers}{rows}"))?;
        resolve_stops(&mut chunk);
        common_chunks.push((chunk, measure_lengths(&rows)));
    }
    let mut compiled = Vec::new();
    for block in blocks {
        let mut branches = Vec::new();
        let mut lengths = Vec::new();
        for rows in block.branches {
            let mut chart = parse(&format!("{headers}{rows}"))?;
            resolve_stops(&mut chart);
            lengths.push(measure_lengths(&rows));
            branches.push(chart);
        }
        compiled.push(ConditionalBlock {
            position: block.position,
            conditions: block.conditions,
            branches,
            lengths,
        });
    }
    let mut program = ConditionalProgram {
        blocks: compiled,
        common,
        common_chunks,
        source: source.into(),
        positions: BTreeMap::new(),
        voice_ids: BTreeMap::new(),
        margin: TimeUs(0),
        transform: None,
    };
    // Video decoders are prepared once before play, including BGA used only by a branch.
    program.common.metadata.has_bga |= program
        .blocks
        .iter()
        .flat_map(|block| &block.branches)
        .any(|branch| branch.metadata.has_bga);
    for chart in
        std::iter::once(&program.common).chain(program.blocks.iter().flat_map(|b| &b.branches))
    {
        for o in &chart.objects {
            if let Some(lane) = object_lane(o) {
                let key = position_key(lane, o);
                let next = program.positions.len() as u32;
                program.positions.entry(key).or_insert(next);
            }
        }
    }
    if matches!(
        program.common.metadata.key_mode,
        bmz_core::lane::KeyMode::K5
            | bmz_core::lane::KeyMode::K7
            | bmz_core::lane::KeyMode::K10
            | bmz_core::lane::KeyMode::K14
    ) {
        let union_mode = bmz_core::lane::KeyMode::detect_from_lanes(
            program
                .positions
                .keys()
                .filter_map(|(lane, _, _, _)| bmz_core::lane::Lane::ALL.get(*lane % 32).copied()),
        );
        if union_mode.lane_count() > program.common.metadata.key_mode.lane_count() {
            program.common.metadata.key_mode = union_mode;
        }
    }
    program.reserve_voices();
    let mut default = program.intermediate(&[]);
    default.metadata.conditional = Some(Arc::new(program));
    Ok(default)
}

fn resolve_stops(chart: &mut IntermediateChart) {
    for object in &mut chart.objects {
        if let IntermediateObjectKind::Stop { stop_key } = object.kind
            && let Some(stop) = chart.resources.stop_table.iter().find(|s| s.key == stop_key)
        {
            object.kind =
                IntermediateObjectKind::BmsonStop { duration_pulses: stop.value, resolution: 48 };
        }
    }
}

fn measure_lengths(rows: &str) -> BTreeMap<u32, f64> {
    rows.lines()
        .filter_map(|line| {
            let (measure, ch, value) = channel(line.trim())?;
            if ch != "02" {
                return None;
            }
            let value: f64 = value.trim().parse().ok()?;
            (value.is_finite() && value > 0.0).then_some((measure, value))
        })
        .collect()
}

fn object_lane(o: &IntermediateObject) -> Option<usize> {
    match o.kind {
        IntermediateObjectKind::VisibleNote { lane, .. }
        | IntermediateObjectKind::LongChannelNote { lane, .. } => Some(lane.index()),
        IntermediateObjectKind::InvisibleNote { lane, .. } => Some(lane.index() + 32),
        IntermediateObjectKind::MineNote { lane, .. } => Some(lane.index() + 64),
        _ => None,
    }
}

fn position_key(lane: usize, o: &IntermediateObject) -> (usize, u32, u32, u32) {
    let mut a = o.position_num;
    let mut b = o.position_den;
    while b != 0 {
        (a, b) = (b, a % b);
    }
    let gcd = a.max(1);
    (lane, o.measure, o.position_num / gcd, o.position_den / gcd)
}

impl ConditionalProgram {
    fn reserve_voices(&mut self) {
        if self.common.metadata.long_note_mode != LongNoteMode::Hln
            && !self.common.typed_lnobj.iter().any(|(_, mode)| *mode == LongNoteMode::Hln)
        {
            return;
        }
        let mut sounds: BTreeMap<u32, Vec<u32>> = BTreeMap::new();
        let wavs = &self.common.resources.wavs;
        let mut add = |object: &IntermediateObject, wav_key: u16| {
            let Some(lane) = object_lane(object) else {
                return;
            };
            let Some(id) = self.positions.get(&position_key(lane, object)) else {
                return;
            };
            let Some(wav) = wavs.iter().find(|w| w.key == wav_key) else {
                return;
            };
            let source =
                wavs.iter().position(|w| w.path == wav.path && w.slice == wav.slice).unwrap();
            sounds.entry(*id).or_default().push(source as u32);
        };
        for chart in
            std::iter::once(&self.common).chain(self.blocks.iter().flat_map(|b| &b.branches))
        {
            for object in &chart.objects {
                if let IntermediateObjectKind::VisibleNote { wav_key: Some(key), .. }
                | IntermediateObjectKind::LongChannelNote { wav_key: Some(key), .. } =
                    object.kind
                {
                    add(object, key);
                }
            }
            for layer in chart.layered_note_sounds.iter().chain(&chart.long_end_sounds) {
                add(
                    &IntermediateObject {
                        measure: layer.measure,
                        position_num: layer.position_num,
                        position_den: layer.position_den,
                        kind: IntermediateObjectKind::VisibleNote {
                            lane: layer.lane,
                            wav_key: Some(layer.wav_key),
                        },
                    },
                    layer.wav_key,
                );
            }
        }
        let base = wavs.len() as u32;
        for (note, mut sources) in sounds {
            let layers = sources.len();
            sources.sort_unstable();
            sources.dedup();
            for source in sources {
                for layer in 0..layers {
                    let id = SoundId(base + self.voice_ids.len() as u32);
                    self.voice_ids.insert((note, source, layer), id);
                }
            }
        }
    }

    fn intermediate(&self, choices: &[Option<usize>]) -> IntermediateChart {
        let mut chart = self.common.clone();
        chart.objects.clear();
        chart.layered_note_sounds.clear();
        chart.long_end_sounds.clear();
        let mut lengths = BTreeMap::new();
        let mut max_measure = chart.measures.len().saturating_sub(1) as u32;
        for (i, (common, common_lengths)) in self.common_chunks.iter().enumerate() {
            chart.objects.extend_from_slice(&common.objects);
            chart.layered_note_sounds.extend_from_slice(&common.layered_note_sounds);
            chart.long_end_sounds.extend_from_slice(&common.long_end_sounds);
            lengths.extend(common_lengths);
            let Some(block) = self.blocks.get(i) else {
                continue;
            };
            let choice = choices.get(i).copied().flatten().unwrap_or(block.conditions.len());
            let branch = &block.branches[choice.min(block.conditions.len())];
            chart.objects.extend_from_slice(&branch.objects);
            chart.layered_note_sounds.extend_from_slice(&branch.layered_note_sounds);
            chart.long_end_sounds.extend_from_slice(&branch.long_end_sounds);
            lengths.extend(&block.lengths[choice.min(block.conditions.len())]);
            max_measure = max_measure
                .max(branch.measures.len().saturating_sub(1) as u32)
                .max(block.position.ceil() as u32 + 1);
            // Keep the input layout invariant even if DEFAULT uses fewer keys.
            for variant in &block.branches {
                if variant.metadata.key_mode.lane_count() > chart.metadata.key_mode.lane_count() {
                    chart.metadata.key_mode = variant.metadata.key_mode;
                }
            }
        }
        let mut stops = std::collections::BTreeSet::new();
        chart.objects.reverse();
        chart.objects.retain(|object| {
            !matches!(
                object.kind,
                IntermediateObjectKind::Stop { .. } | IntermediateObjectKind::BmsonStop { .. }
            ) || stops.insert(position_key(0, object))
        });
        chart.objects.reverse();
        let mut tick = 0;
        chart.measures = (0..=max_measure)
            .map(|index| {
                let length = lengths.get(&index).copied().unwrap_or(1.0);
                let tick_len = (length * f64::from(TICKS_PER_MEASURE) * IMPORT_TICK_SCALE as f64)
                    .round()
                    .max(1.0) as u64;
                let m = MeasureInfo {
                    index,
                    length,
                    length_ratio_num: (length * 1_000_000.0).round() as u32,
                    length_ratio_den: 1_000_000,
                    start_tick: ChartTick(tick),
                    tick_len,
                };
                tick = tick.saturating_add(tick_len);
                m
            })
            .collect();
        chart
    }

    pub fn materialize(&self, choices: &[Option<usize>]) -> Result<PlayableChart, ImportError> {
        let intermediate = self.intermediate(choices);
        let mut chart =
            normalize_chart(&self.source, intermediate.clone(), &mut Vec::new(), false)?;
        self.stabilize(&intermediate, &mut chart)?;
        chart.metadata.conditional_revision = 1;
        crate::start_margin::apply_fixed_start_margin(&mut chart, self.margin);
        if let Some(transform) = &self.transform {
            transform(&mut chart);
        }
        Ok(chart)
    }

    pub fn stabilize_default(&self, chart: &mut PlayableChart) -> Result<(), ImportError> {
        self.stabilize(&self.intermediate(&[]), chart)
    }

    fn stabilize(
        &self,
        intermediate: &IntermediateChart,
        chart: &mut PlayableChart,
    ) -> Result<(), ImportError> {
        let timing = crate::timing::build_timing_map_with_tick_scale(
            intermediate.metadata.initial_bpm.max(1.0),
            crate::import::normalize::collect_timing_events(intermediate, &mut Vec::new())?,
            IMPORT_TICK_SCALE,
        );
        // Evaluation uses import precision, not the rounded ticks used for rendering.
        chart.metadata.conditional_evaluation_times = self
            .blocks
            .iter()
            .map(|block| {
                let measure = &intermediate.measures[block.position.floor() as usize];
                let local = (measure.tick_len as f64 * block.position.fract()) as u64;
                timing.tick_to_time(ChartTick(measure.start_tick.0.saturating_add(local)))
            })
            .collect();
        let mut ids = BTreeMap::new();
        let mut bgm_keys: BTreeMap<(i64, u32), Vec<BgmKey>> = BTreeMap::new();
        for object in &intermediate.objects {
            if let IntermediateObjectKind::Bgm { wav_key } = object.kind {
                let time = timing.tick_to_time(crate::import::normalize::object_to_tick(
                    object,
                    &intermediate.measures,
                )?);
                if let Some(sound) =
                    intermediate.resources.wavs.iter().position(|w| w.key == wav_key)
                {
                    let (_, measure, num, den) = position_key(0, object);
                    let keys = bgm_keys.entry((time.0, sound as u32)).or_default();
                    keys.push((measure, num, den, u64::from(wav_key), keys.len() as u32));
                }
            }
        }
        let mut bgm_used = BTreeMap::new();
        chart.metadata.conditional_bgm_keys = chart
            .bgm_events
            .iter()
            .map(|event| {
                let key = (event.time.0, event.sound.0);
                let used = bgm_used.entry(key).or_insert(0usize);
                let value = bgm_keys[&key][*used];
                *used += 1;
                value
            })
            .collect();
        for object in &intermediate.objects {
            let Some(lane) = object_lane(object) else {
                continue;
            };
            let time = timing.tick_to_time(crate::import::normalize::object_to_tick(
                object,
                &intermediate.measures,
            )?);
            if let Some(id) = self.positions.get(&position_key(lane, object)) {
                ids.insert((lane, time.0), NoteId(*id));
            }
        }
        let mut remap = BTreeMap::new();
        for notes in &mut chart.lane_notes {
            for note in notes {
                let lane = note.lane.index()
                    + match note.kind {
                        NoteKind::Invisible => 32,
                        NoteKind::Mine => 64,
                        _ => 0,
                    };
                if let Some(id) = ids.get(&(lane, note.time.0)) {
                    remap.insert(note.id.0, *id);
                    note.id = *id;
                }
            }
        }
        // Reserve isolated HLN voice IDs by source position, independent of selected branches.
        let base_sounds = intermediate.resources.wavs.len() as u32;
        let mut sound_remap = BTreeMap::new();
        for notes in &mut chart.lane_notes {
            for note in notes {
                for (layer, sound) in
                    note.sound.iter_mut().chain(&mut note.layered_sounds).enumerate()
                {
                    if sound.0 < base_sounds {
                        continue;
                    }
                    let asset =
                        chart.sounds.iter().find(|a| a.id == *sound).expect("normalized HLN sound");
                    let source = chart.sounds[..base_sounds as usize]
                        .iter()
                        .position(|a| a.path == asset.path && a.slice == asset.slice)
                        .expect("HLN source sound");
                    let id = self.voice_ids[&(note.id.0, source as u32, layer)];
                    sound_remap.insert(sound.0, id);
                    *sound = id;
                }
            }
        }
        for asset in &mut chart.sounds {
            if let Some(id) = sound_remap.get(&asset.id.0) {
                asset.id = *id;
            }
        }
        for pair in &mut chart.long_notes {
            pair.start_note_id = remap[&pair.start_note_id.0];
            pair.end_note_id = remap[&pair.end_note_id.0];
            pair.sound = chart.lane_notes[pair.lane.index()]
                .iter()
                .find(|n| n.id == pair.start_note_id)
                .and_then(|n| n.sound);
        }
        Ok(())
    }

    pub fn evaluation_time(&self, index: usize, chart: &PlayableChart) -> TimeUs {
        chart.metadata.conditional_evaluation_times[index]
    }

    pub fn all_sounds(&self) -> Result<Vec<SoundAssetRef>, ImportError> {
        let mut assets = BTreeMap::new();
        for (i, block) in self.blocks.iter().enumerate() {
            for choice in 0..block.branches.len() {
                let mut choices = vec![None; self.blocks.len()];
                choices[i] = Some(choice);
                for asset in self.materialize(&choices)?.sounds {
                    assets.insert(asset.id.0, asset);
                }
            }
        }
        // A start and a typed end marker can come from different blocks. Reserve their
        // combined HLN voices without materializing the Cartesian product of all branches.
        for ((_, source, _), id) in &self.voice_ids {
            let mut asset = assets[source].clone();
            asset.id = *id;
            assets.insert(id.0, asset);
        }
        Ok(assets.into_values().collect())
    }
}

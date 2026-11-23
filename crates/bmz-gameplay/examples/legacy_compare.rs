//! Offline recorded OpenLR2 vs BMZ LEGACY judge-engine comparison.
//! One JSON request/result per line; see tools/legacy-compare/README.md.
use std::{
    collections::BTreeMap,
    io::{self, BufRead, Write},
    path::Path,
};

use anyhow::{Context, Result, bail, ensure};
use bmz_chart::{
    import::{AssetLookup, BmsRandomSource, import_chart_with_assets},
    model::{LongNoteMode, NoteKind},
};
use bmz_core::{
    input::{InputEvent, InputKind, InputSource},
    judge::Judge,
    lane::{KeyMode, Lane},
    time::TimeUs,
};
use bmz_gameplay::{
    judge::{
        engine::JudgeEngine,
        window::{
            judge_rank_spec_to_percent_optional_for_keymode_and_rule_mode,
            judge_windows_for_keymode_and_rule_mode, judge_windows_for_rule_mode_and_keymode,
        },
    },
    rule::RuleMode,
};
use serde::Deserialize;
use serde_json::{Value, json};

#[path = "legacy_compare/captured.rs"]
mod captured;
#[path = "legacy_compare/dump.rs"]
mod dump;
#[path = "legacy_compare/oracle_out.rs"]
mod oracle_out;
use captured::compare_poll_trace;

#[derive(Deserialize)]
struct Request {
    chart_path: String,
    replay_path: String,
    md5: String,
    trace_path: Option<String>,
    poll_trace: Option<String>,
    #[serde(default)]
    analysis_only: bool,
    /// Also write judgements, mines and score/gauge state in the
    /// openlr2_oracle output format, stopping when the gauge closes play.
    oracle_output_path: Option<String>,
    /// Loader dump only (NORMAL arrangement): write `H` / `N` lines here.
    dump_path: Option<String>,
    /// LR2 random seed for a loader dump.
    #[serde(default = "default_seed")]
    seed: u32,
}

fn default_seed() -> u32 {
    1
}

#[derive(Clone, Copy, Debug)]
struct Record {
    ms: i32,
    op: u8,
    value: i32,
}

fn records(bytes: &[u8]) -> Result<Vec<Record>> {
    ensure!(
        !bytes.is_empty() && bytes.len().is_multiple_of(12),
        "invalid 12-byte ReplayData length"
    );
    Ok(bytes
        .as_chunks::<12>()
        .0
        .iter()
        .map(|b| Record {
            ms: i32::from_le_bytes(b[0..4].try_into().unwrap()),
            op: b[4],
            value: i32::from_le_bytes(b[8..12].try_into().unwrap()),
        })
        .collect())
}

fn value(j: Judge) -> usize {
    match j {
        Judge::PGreat => 5,
        Judge::Great => 4,
        Judge::Good => 3,
        Judge::Bad => 2,
        Judge::Poor => 1,
        Judge::EmptyPoor => 0,
    }
}

fn input_lane(op: u8, keys: usize, sides: usize) -> Option<Lane> {
    if usize::from(op) <= keys {
        return Some(Lane::ALL[usize::from(op)]);
    }
    if sides == 2 && op == 20 {
        return Some(Lane::Scratch2);
    }
    if sides == 2 && (21..=20 + keys as u8).contains(&op) {
        return Some(Lane::ALL[usize::from(op) - 13]);
    }
    None
}

fn compare(req: &Request) -> Result<Value> {
    if let Some(path) = &req.dump_path {
        dump::dump_chart(&req.chart_path, &req.md5, req.seed, path)?;
        return Ok(json!({"status":"dumped"}));
    }
    let replay = if let Some(path) = &req.poll_trace {
        captured::headers_from_trace(Path::new(path), &req.md5)?
    } else {
        records(&std::fs::read(&req.replay_path)?)?
    };
    let mut headers = BTreeMap::new();
    for r in &replay {
        if r.ms == 0 && r.op >= 40 && r.op != 210 {
            headers.insert(r.op, r.value);
        }
    }
    let header =
        |op| headers.get(&op).copied().with_context(|| format!("missing replay header {op}"));
    // Reject options whose interpretation is not reproduced here. Never infer a layout
    // from expected judgements: the reference must remain independent.
    for op in [106, 107, 201, 202, 204, 205, 206] {
        ensure!(header(op)? == 0, "unsupported replay option {op}={}", header(op)?);
    }
    ensure!(header(90)? == 0, "pitch playback is unsupported");
    let arrange = header(103)?;
    ensure!((0..=2).contains(&arrange), "unsupported arrangement {arrange}");
    let seed = u32::try_from(header(200)?).context("negative seed")?;
    ensure!(seed <= 0x7ffe, "unsupported seed {seed}");
    let mut imported = import_chart_with_assets(
        Path::new(&req.chart_path),
        BmsRandomSource::Lr2Seed(seed),
        AssetLookup::Declared,
    )?;
    let actual_md5: String =
        imported.chart.identity.file_md5.iter().map(|b| format!("{b:02x}")).collect();
    ensure!(actual_md5 == req.md5.to_ascii_lowercase(), "chart MD5 does not match replay filename");
    bmz_chart::import::openlr2::apply_from_bytes(
        &mut imported.chart,
        Path::new(&req.chart_path),
        &std::fs::read(&req.chart_path)?,
        &BmsRandomSource::Lr2Seed(seed),
        None,
    )?;
    let chart = &mut imported.chart;
    let (keys, sides) = match chart.metadata.key_mode {
        KeyMode::K5 => (5, 1),
        KeyMode::K7 => (7, 1),
        KeyMode::K10 => (5, 2),
        KeyMode::K14 => (7, 2),
        mode => bail!("unsupported key mode {mode:?}"),
    };
    ensure!(chart.judge_rank_events.is_empty(), "dynamic judge rank is unsupported");
    ensure!(
        chart.metadata.conditional.is_none() && chart.metadata.free_zones.is_empty(),
        "conditional/free-zone chart is unsupported"
    );
    ensure!(
        chart.metadata.long_note_mode == LongNoteMode::Ln
            && chart.long_notes.iter().all(|p| p.mode.is_none_or(|m| m == LongNoteMode::Ln)),
        "only standard LN is supported"
    );
    ensure!(
        chart.lane_notes.iter().flatten().all(|n| matches!(
            n.kind,
            NoteKind::Tap
                | NoteKind::LongStart
                | NoteKind::LongEnd
                | NoteKind::Invisible
                | NoteKind::Mine
        )),
        "unsupported note kind (extension)"
    );
    let has_mines = chart.lane_notes.iter().flatten().any(|n| n.kind == NoteKind::Mine);
    // The poll trace records ProcNoteOnTiming arguments only, not held lanes.
    ensure!(!(has_mines && req.poll_trace.is_some()), "mines are unsupported with a poll trace");
    let fixed_key = usize::try_from(header(105)?).context("negative fixed key")?;
    ensure!(fixed_key <= keys, "invalid fixed key {fixed_key}");
    let mut table: Vec<usize> = (0..16).collect();
    for side in 0..sides {
        let side_arrange = if side == 0 { arrange } else { header(153)? };
        ensure!((0..=2).contains(&side_arrange), "unsupported P2 arrangement {side_arrange}");
        if side == 1 {
            for op in [156, 157] {
                ensure!(header(op)? == 0, "unsupported P2 option {op}");
            }
        }
        let fixed = if side == 0 {
            fixed_key
        } else {
            usize::try_from(header(155)?).context("negative P2 fixed key")?
        };
        ensure!(fixed <= keys, "invalid fixed key {fixed}");
        let mut side_table: Vec<_> = match side_arrange {
            0 => (0..=keys).collect(),
            1 => std::iter::once(0).chain((1..=keys).rev()).collect(),
            _ => imported
                .lr2_random_state
                .as_mut()
                .context("missing LR2 random state")?
                .shuffle_key_table(keys),
        };
        if side_arrange == 2 && fixed > 0 {
            let position = side_table.iter().position(|&key| key == fixed).unwrap();
            side_table.swap(1, position);
        }
        for key in 1..=keys {
            table[side * 7 + key] = side * 7 + side_table[key];
        }
    }
    // Source chart lane -> replay input lane. Apply identically to LN pairs.
    let mut lanes = std::array::from_fn(|_| Vec::new());
    for notes in &chart.lane_notes {
        for note in notes {
            let mut note = note.clone();
            note.lane = Lane::ALL[table[note.lane.index()]];
            lanes[note.lane.index()].push(note);
        }
    }
    chart.lane_notes = lanes;
    for pair in &mut chart.long_notes {
        pair.lane = Lane::ALL[table[pair.lane.index()]];
    }

    let inputs: Vec<_> =
        replay.iter().filter(|r| input_lane(r.op, keys, sides).is_some()).collect();
    ensure!(inputs.iter().all(|r| r.value == 0 || r.value == 1), "invalid input value");
    ensure!(inputs.windows(2).all(|p| p[0].ms <= p[1].ms), "non-monotonic inputs");
    ensure!(inputs.iter().all(|r| r.ms >= 0), "negative input timestamp");
    let percent = judge_rank_spec_to_percent_optional_for_keymode_and_rule_mode(
        chart.metadata.judge_rank_spec,
        chart.metadata.key_mode,
        RuleMode::Legacy,
    );
    let base = judge_windows_for_keymode_and_rule_mode(chart.metadata.key_mode, RuleMode::Legacy);
    let windows = judge_windows_for_rule_mode_and_keymode(
        base,
        percent,
        RuleMode::Legacy,
        chart.metadata.key_mode,
    );
    if req.analysis_only {
        let notes: Vec<_> = chart.lane_notes.iter().flatten()
            .filter(|n| matches!(n.kind, NoteKind::Tap | NoteKind::LongStart))
            .map(|n| json!({"id":n.id.0,"lane":n.lane.index(),"time_us":n.time.0,
                "kind":format!("{:?}",n.kind),
                "tail_us":chart.long_notes.iter().find(|p|p.start_note_id==n.id).map(|p|p.end_time.0)}))
            .collect();
        return Ok(json!({"status":"analysis_input","notes":notes,"windows":windows.note,
            "rank_percent":percent,"seed":seed,"arrange":arrange,"chart_notes":chart.total_notes}));
    }
    let chart = std::sync::Arc::new(imported.chart);
    let mut judge =
        JudgeEngine::new_with_window_set(windows, RuleMode::Legacy).with_chart_index(chart.clone());
    // Mines share the lane cursor with notes, as on the BMZ LEGACY polling clock.
    judge.set_legacy_inline_mines(true);
    if let Some(path) = &req.poll_trace {
        return compare_poll_trace(&mut judge, &chart, Path::new(path), &req.md5, seed);
    }
    let expected: Vec<_> =
        replay.iter().filter(|r| r.op == 210 || (sides == 2 && r.op == 211)).collect();
    ensure!(!expected.is_empty(), "no recorded judgements (op 210/211)");
    ensure!(expected.iter().all(|r| (0..=5).contains(&r.value)), "invalid recorded judgement");
    let mut expected_counts = [0u32; 6];
    for r in &expected {
        expected_counts[r.value as usize] += 1;
    }
    // ApplyJudgeMine records op 214 (+1 for the right DP side) with the damage.
    let expected_mines: Vec<(i32, i32)> = replay
        .iter()
        .filter(|r| r.op == 214 || (sides == 2 && r.op == 215))
        .map(|r| (r.ms, r.value))
        .collect();
    let mut mine_hits: Vec<(i64, usize, f64)> = Vec::new();
    let mut held = [false; bmz_core::lane::LANE_COUNT];
    let reference_notes: u32 = expected_counts[1..].iter().sum();
    let complete = reference_notes == chart.total_notes;
    let last_input = inputs.last().map_or(0, |r| i64::from(r.ms));
    let end_ms = (chart.end_time.0 / 1000 + 2001).max(last_input + 2001);
    ensure!(end_ms <= 3_600_000, "chart/replay exceeds one-hour safety limit");
    let mut cursor = 0;
    let mut counts = [0_u32; 6];
    let mut sequence = Vec::new();
    let mut first_difference = None;
    let mut trace = req
        .trace_path
        .as_ref()
        .map(|p| std::fs::File::create(p).map(io::BufWriter::new))
        .transpose()?;
    if let Some(out) = &mut trace {
        writeln!(
            out,
            "index,bmz_ms,reference_ms,bmz_judge,reference_judge,bmz_lane,bmz_note_id,delta_us"
        )?;
    }
    let mut oracle = match &req.oracle_output_path {
        Some(path) => Some(oracle_out::OracleOut::new(Path::new(path), &chart, header(101)?)?),
        None => None,
    };
    let mut closed_at = None;
    // Explicit 1ms polling model. Original OpenLR2 frame boundaries are not recorded.
    // This is a judge-engine diagnostic, not a full GameSession/gauge simulation.
    for ms in 0..=end_ms {
        let now = TimeUs(ms * 1000);
        let mut events = Vec::new();
        let start = cursor;
        while inputs.get(cursor).is_some_and(|r| i64::from(r.ms) == ms) {
            cursor += 1;
        }
        for lane in bmz_gameplay::judge::engine::LEGACY_LANE_ORDER {
            let mut has_input = false;
            for r in
                inputs[start..cursor].iter().filter(|r| input_lane(r.op, keys, sides) == Some(lane))
            {
                has_input = true;
                held[lane.index()] = r.value == 1;
                let outcome = judge.process_input(
                    &chart,
                    InputEvent {
                        lane,
                        kind: if r.value == 1 { InputKind::Press } else { InputKind::Release },
                        time: now,
                        source: InputSource::Replay,
                        device_kind: Default::default(),
                        scratch_direction: None,
                    },
                );
                record_lane(&mut oracle, ms, lane, &outcome, &mut mine_hits, &mut events)?;
            }
            if !has_input {
                let outcome =
                    judge.process_legacy_lane_poll(&chart, lane, now, held[lane.index()], false);
                record_lane(&mut oracle, ms, lane, &outcome, &mut mine_hits, &mut events)?;
            }
        }
        if let Some(oracle) = &mut oracle
            && oracle.end_poll(ms)?
        {
            closed_at = Some(ms + 1);
        }
        for event in events.into_iter().filter(|e| e.affects_score) {
            let got = value(event.judge);
            let index = sequence.len();
            if let Some(out) = &mut trace {
                let reference = expected.get(index);
                writeln!(
                    out,
                    "{index},{ms},{},{got},{},{},{},{}",
                    reference.map(|r| r.ms.to_string()).unwrap_or_default(),
                    reference.map(|r| r.value.to_string()).unwrap_or_default(),
                    event.lane.index(),
                    event.note_id.map(|id| id.0.to_string()).unwrap_or_default(),
                    event.delta.0
                )?;
            }
            if first_difference.is_none()
                && expected.get(index).map(|r| r.value as usize) != Some(got)
            {
                first_difference = Some(json!({"index": index, "bmz_ms": ms, "bmz_judge": got,
                    "reference_ms": expected.get(index).map(|r| r.ms), "reference_judge": expected.get(index).map(|r| r.value),
                    "lane": event.lane.index(), "note_id": event.note_id.map(|id| id.0), "delta_us": event.delta.0}));
            }
            counts[got] += 1;
            sequence.push(got as i32);
        }
        if closed_at.is_some() {
            break;
        }
    }
    if let Some(oracle) = oracle {
        match closed_at {
            Some(ms) => oracle.finish(ms, "closed")?,
            None => oracle.finish(end_ms + 1, "end")?,
        }
    }
    let sequence_equal = sequence.iter().copied().eq(expected.iter().map(|r| r.value));
    if let Some(out) = &mut trace {
        for (index, reference) in expected.iter().enumerate().skip(sequence.len()) {
            writeln!(out, "{index},,{},,{},,,", reference.ms, reference.value)?;
        }
        out.flush()?;
    }
    if first_difference.is_none() && sequence.len() < expected.len() {
        let r = expected[sequence.len()];
        first_difference = Some(json!({"index":sequence.len(), "bmz_judge":null,
            "reference_ms":r.ms, "reference_judge":r.value}));
    }
    let mines_equal = mine_hits.len() == expected_mines.len()
        && mine_hits.iter().zip(&expected_mines).all(|(got, want)| got.2 as i32 == want.1);
    let mine_ms_deltas: Vec<i64> = mine_hits
        .iter()
        .zip(&expected_mines)
        .map(|(got, want)| got.0 - i64::from(want.0))
        .collect();
    let status = if !complete {
        "incomplete_reference"
    } else if !mines_equal {
        "mismatch"
    } else if counts == expected_counts && sequence_equal {
        "match"
    } else if counts == expected_counts {
        "judgement_sequence_mismatch"
    } else {
        "mismatch"
    };
    Ok(
        json!({"status":status, "bmz_counts_ep_p_b_g_gr_pg":counts, "reference_counts_ep_p_b_g_gr_pg":expected_counts,
        "bmz_ex":counts[5]*2+counts[4], "reference_ex":expected_counts[5]*2+expected_counts[4],
        "ex_delta": i64::from(counts[5]*2+counts[4])-i64::from(expected_counts[5]*2+expected_counts[4]),
        "sequence_equal":sequence_equal, "first_sequence_difference":first_difference,
        "chart_has_mines":has_mines, "bmz_mine_hits":mine_hits.len(), "reference_mine_hits":expected_mines.len(),
        "mine_hits_equal":mines_equal, "mine_hit_ms_deltas":mine_ms_deltas,
        "bmz_mine_hit_list":mine_hits.iter().map(|h| (h.0, h.1)).collect::<Vec<_>>(),
        "reference_mine_hit_list":expected_mines.iter().map(|m| m.0).collect::<Vec<_>>(),
        "chart_mines":has_mines.then(|| chart.lane_notes.iter().flatten()
            .filter(|n| n.kind == NoteKind::Mine)
            .map(|n| (n.time.0, n.lane.index())).collect::<Vec<_>>()),
        "chart_notes":chart.total_notes, "reference_notes":reference_notes, "rank_percent":percent,
        "arrange":arrange, "fixed_key":fixed_key, "seed":seed, "source_to_input_lanes":table, "import_warning_count":imported.warnings.len(),
        "model":"openlr2-sequential-timing-lane-polls-1ms", "full_session_verified":false}),
    )
}

/// Collects one lane's outcome and, in oracle mode, applies it to the score
/// and gauges in lane order as GameSession does.
fn record_lane(
    oracle: &mut Option<oracle_out::OracleOut>,
    ms: i64,
    lane: Lane,
    outcome: &bmz_gameplay::judge::model::JudgeOutcome,
    mine_hits: &mut Vec<(i64, usize, f64)>,
    events: &mut Vec<bmz_gameplay::judge::model::JudgementEvent>,
) -> Result<()> {
    mine_hits.extend(outcome.mine_hits.iter().map(|h| (ms, lane.index(), h.damage)));
    if let Some(oracle) = oracle {
        let scored: Vec<_> = outcome.events.iter().filter(|e| e.affects_score).cloned().collect();
        let mines: Vec<_> = outcome.mine_hits.iter().map(|h| (lane, h.damage)).collect();
        oracle.lane(ms, &scored, &mines, |e| value(e.judge))?;
    }
    events.extend(outcome.events.iter().cloned());
    Ok(())
}

fn main() -> Result<()> {
    for line in io::stdin().lock().lines() {
        let line = line?;
        let result = serde_json::from_str::<Request>(&line)
            .map_err(anyhow::Error::from)
            .and_then(|r| compare(&r));
        let output = result.unwrap_or_else(
            |e| json!({"status":"unsupported_or_error", "reason":format!("{e:#}")}),
        );
        println!("{}", serde_json::to_string(&output)?);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dp_replay_input_lanes_keep_both_scratches_and_keys() {
        assert_eq!(input_lane(20, 7, 2), Some(Lane::Scratch2));
        assert_eq!(input_lane(21, 7, 2), Some(Lane::Key8));
        assert_eq!(input_lane(27, 7, 2), Some(Lane::Key14));
        assert_eq!(input_lane(30, 7, 2), None);
        assert_eq!(input_lane(20, 7, 1), None);
    }

    #[test]
    fn dp_and_standard_long_scratch_recordings_can_be_compared() {
        let dir = tempfile::tempdir().unwrap();
        for (text, inputs, expected_ex) in [
            (
                "#BPM 120\n#RANK 3\n#PLAYER 3\n#00111:01\n#00129:01\n",
                vec![(2000, 1, 1), (2000, 210, 5), (2000, 27, 1), (2000, 210, 5)],
                4,
            ),
            (
                "#BPM 120\n#RANK 3\n#PLAYER 1\n#00156:01\n#00256:01\n",
                vec![(2000, 0, 1), (4000, 0, 0), (4000, 210, 5)],
                2,
            ),
        ] {
            let path = dir.path().join("chart.bms");
            std::fs::write(&path, text).unwrap();
            let imported =
                import_chart_with_assets(&path, BmsRandomSource::Lr2Seed(1), AssetLookup::Declared)
                    .unwrap();
            let md5 = imported.chart.identity.file_md5.iter().map(|b| format!("{b:02x}")).collect();
            let replay_path = dir.path().join("chart.lr2rep");
            let mut rows = vec![
                (0_i32, 105, 0),
                (0, 106, 0),
                (0, 107, 0),
                (0, 201, 0),
                (0, 202, 0),
                (0, 204, 0),
                (0, 205, 0),
                (0, 206, 0),
                (0, 90, 0),
                (0, 103, 0),
                (0, 200, 1),
                (0, 153, 0),
                (0, 155, 0),
                (0, 156, 0),
                (0, 157, 0),
            ];
            rows.extend(inputs);
            let mut bytes = Vec::new();
            for (ms, op, val) in rows {
                bytes.extend_from_slice(&ms.to_le_bytes());
                bytes.extend_from_slice(&[op, 0, 0, 0]);
                bytes.extend_from_slice(&i32::to_le_bytes(val));
            }
            std::fs::write(&replay_path, bytes).unwrap();
            let result = compare(&Request {
                chart_path: path.to_string_lossy().into_owned(),
                replay_path: replay_path.to_string_lossy().into_owned(),
                md5,
                trace_path: None,
                poll_trace: None,
                analysis_only: false,
            })
            .unwrap();
            assert_eq!(result["status"], "match");
            assert_eq!(result["bmz_ex"], expected_ex);
        }
    }

    #[test]
    fn rejects_truncated_and_empty_replays() {
        assert!(records(&[]).is_err());
        assert!(records(&[0; 11]).is_err());
        assert!(records(&[0; 13]).is_err());
    }

    #[test]
    fn reads_padded_little_endian_records() {
        let bytes = [0x78, 0x56, 0x34, 0x12, 210, 99, 88, 77, 5, 0, 0, 0];
        let rows = records(&bytes).unwrap();
        assert_eq!(rows[0].ms, 0x12345678);
        assert_eq!(rows[0].op, 210);
        assert_eq!(rows[0].value, 5);
    }

    #[test]
    fn compares_a_complete_recorded_play_and_detects_a_score_difference() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("tap.bms");
        std::fs::write(&path, b"#TITLE comparison test\n#BPM 120\n#RANK 3\n#PLAYER 1\n#00111:01\n")
            .unwrap();
        let imported =
            import_chart_with_assets(&path, BmsRandomSource::Lr2Seed(1), AssetLookup::Declared)
                .unwrap();
        assert_eq!(imported.chart.lane_notes[Lane::Key1.index()][0].time.0, 2_000_000);
        let md5: String =
            imported.chart.identity.file_md5.iter().map(|b| format!("{b:02x}")).collect();
        let replay_path = dir.path().join("tap.lr2rep");
        let req = Request {
            chart_path: path.to_string_lossy().into_owned(),
            replay_path: replay_path.to_string_lossy().into_owned(),
            md5,
            trace_path: Some(dir.path().join("trace.csv").to_string_lossy().into_owned()),
            poll_trace: None,
            analysis_only: false,
        };
        for (input_ms, status, ex, arrange, fixed_key, lane) in [
            (2000_i32, "match", 2, 0, 0, 1),
            (2025, "mismatch", 1, 0, 0, 1),
            (2000, "match", 2, 2, 5, 5),
        ] {
            let mut bytes = Vec::new();
            for (ms, op, val) in [
                (0, 105, fixed_key),
                (0, 106, 0),
                (0, 107, 0),
                (0, 201, 0),
                (0, 202, 0),
                (0, 204, 0),
                (0, 205, 0),
                (0, 206, 0),
                (0, 90, 0),
                (0, 103, arrange),
                (0, 200, 1),
                (input_ms, lane, 1),
                (input_ms, 210, 5),
            ] {
                bytes.extend_from_slice(&ms.to_le_bytes());
                bytes.extend_from_slice(&[op, 0, 0, 0]);
                bytes.extend_from_slice(&i32::to_le_bytes(val));
            }
            std::fs::write(&replay_path, bytes).unwrap();
            let result = compare(&req).unwrap();
            assert_eq!(result["status"], status);
            assert_eq!(result["bmz_ex"], ex);
            assert_eq!(result["reference_ex"], 2);
            let trace = std::fs::read_to_string(req.trace_path.as_ref().unwrap()).unwrap();
            assert_eq!(trace.lines().count(), 2);
        }
        let invalid = Request { md5: "0".repeat(32), ..req };
        assert!(compare(&invalid).unwrap_err().to_string().contains("MD5"));
    }

    #[test]
    fn equal_counts_with_different_judgement_order_are_not_a_match() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("two.bms");
        std::fs::write(
            &path,
            b"#TITLE order test\n#BPM 120\n#RANK 3\n#PLAYER 1\n#00111:01\n#00112:01\n",
        )
        .unwrap();
        let imported =
            import_chart_with_assets(&path, BmsRandomSource::Lr2Seed(1), AssetLookup::Declared)
                .unwrap();
        let md5: String =
            imported.chart.identity.file_md5.iter().map(|b| format!("{b:02x}")).collect();
        let replay_path = dir.path().join("two.lr2rep");
        let mut bytes = Vec::new();
        for (ms, op, val) in [
            (0_i32, 105, 0),
            (0, 106, 0),
            (0, 107, 0),
            (0, 201, 0),
            (0, 202, 0),
            (0, 204, 0),
            (0, 205, 0),
            (0, 206, 0),
            (0, 90, 0),
            (0, 103, 0),
            (0, 200, 1),
            (2000, 1, 1),
            (2000, 210, 4),
            (2025, 2, 1),
            (2025, 210, 5),
        ] {
            bytes.extend_from_slice(&ms.to_le_bytes());
            bytes.extend_from_slice(&[op, 0, 0, 0]);
            bytes.extend_from_slice(&i32::to_le_bytes(val));
        }
        std::fs::write(&replay_path, bytes).unwrap();
        let result = compare(&Request {
            chart_path: path.to_string_lossy().into_owned(),
            replay_path: replay_path.to_string_lossy().into_owned(),
            md5,
            trace_path: None,
            poll_trace: None,
            analysis_only: false,
        })
        .unwrap();
        assert_eq!(result["status"], "judgement_sequence_mismatch");
        assert_eq!(result["ex_delta"], 0);
        assert_eq!(result["sequence_equal"], false);
    }
}

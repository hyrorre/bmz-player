//! Compare against real ProcNoteOnTiming arguments; never infer timing from scores.
use super::*;

#[derive(Default)]
struct Poll {
    row: Option<Vec<String>>,
    expected: Vec<usize>,
}

pub(super) fn headers_from_trace(path: &Path, md5: &str) -> Result<Vec<Record>> {
    let text = std::fs::read_to_string(path)?;
    for line in text.lines().skip(1).filter(|line| line.starts_with("P,")) {
        let row: Vec<_> = line.split(',').collect();
        ensure!(row.len() >= 33, "capture metadata is missing");
        if !row[19].is_empty() && !row[19].eq_ignore_ascii_case(md5) {
            continue;
        }
        for value in &row[26..33] {
            ensure!(
                value.parse::<i32>()? == 0,
                "capture uses unsupported scratch/assist/battle/flip/pitch options"
            );
        }
        let mut headers = Vec::new();
        for (op, index) in [(103, 22), (153, 23), (105, 24), (155, 25), (200, 20)] {
            headers.push(Record { ms: 0, op, value: row[index].parse()? });
        }
        for op in [106, 107, 156, 157, 201, 202, 204, 205, 206, 90] {
            headers.push(Record { ms: 0, op, value: 0 });
        }
        return Ok(headers);
    }
    bail!("no captured polls match the requested MD5")
}

pub(super) fn compare_poll_trace(
    engine: &mut JudgeEngine,
    chart: &bmz_chart::model::PlayableChart,
    path: &Path,
    md5: &str,
    seed: u32,
) -> Result<Value> {
    let text = std::fs::read_to_string(path)?;
    let mut polls: BTreeMap<u64, Poll> = BTreeMap::new();
    for line in text.lines().skip(1).filter(|line| !line.is_empty()) {
        let row: Vec<_> = line.split(',').map(str::to_owned).collect();
        ensure!(row.len() >= 12, "truncated captured trace row");
        let id = row[1].parse()?;
        match row[0].as_str() {
            "P" => {
                ensure!(row.len() >= 33, "capture metadata is missing");
                polls.entry(id).or_default().row = Some(row);
            }
            "J" => {
                let grade: usize = row[11].parse()?;
                ensure!(grade <= 5, "invalid captured judgement");
                polls.entry(id).or_default().expected.push(grade);
            }
            _ => bail!("unknown captured record type"),
        }
    }
    let mut bmz = [0u32; 6];
    let mut reference = [0u32; 6];
    let mut first_difference = None;
    let mut timing_differences = std::collections::BTreeSet::new();
    let mut selected = 0;
    let mut previous_score = 0;
    let mut trace_hash_present = true;
    for (id, poll) in polls {
        let row = poll
            .row
            .context("judgement has no completed poll; close OpenLR2 normally before comparing")?;
        if (!row[19].is_empty() && !row[19].eq_ignore_ascii_case(md5))
            || row[20].parse::<u32>()? != seed
        {
            continue;
        }
        trace_hash_present &= !row[19].is_empty();
        for value in &row[26..33] {
            ensure!(
                value.parse::<i32>()? == 0,
                "capture uses unsupported scratch/assist/battle/flip/pitch options"
            );
        }
        let timing: i64 = row[2].parse()?;
        let raw_lane: usize = row[3].parse()?;
        let lane = match raw_lane {
            0..=7 => Lane::ALL[raw_lane],
            10 => Lane::Scratch2,
            11..=17 => Lane::ALL[raw_lane - 3],
            _ => {
                ensure!(poll.expected.is_empty(), "unsupported captured lane {raw_lane}");
                continue;
            }
        };
        let key: i32 = row[4].parse()?;
        let cursor: usize = row[5].parse()?;
        let before_score: u32 = row[12].parse()?;
        ensure!(
            before_score >= previous_score,
            "capture contains multiple plays; record one play per capture"
        );
        ensure!(selected != 0 || before_score == 0, "capture started after scoring began");
        previous_score = row[13].parse()?;
        selected += 1;
        if let Some(note) = chart
            .notes_for_lane(lane)
            .iter()
            .filter(|n| matches!(n.kind, NoteKind::Tap | NoteKind::LongStart))
            .nth(cursor)
        {
            let expected_ms = row[7].parse::<f64>()?.trunc() as i64;
            if note.time.0 / 1000 != expected_ms {
                timing_differences.insert((lane.index(), cursor));
            }
            if let Some(pair) = chart.long_notes.iter().find(|pair| pair.start_note_id == note.id) {
                let tail_ms = row[8].parse::<f64>()?.trunc() as i64;
                if pair.end_time.0 / 1000 != tail_ms {
                    timing_differences.insert((lane.index(), cursor));
                }
            }
        }
        let width = |index: usize| -> Result<i64> { Ok(row[index].parse::<i64>()? * 1000) };
        let window = bmz_gameplay::judge::model::JudgeWindow::symmetric(
            width(14)?,
            width(15)?,
            width(16)?,
            width(17)?,
            width(18)?,
            0,
        );
        engine.windows = window;
        engine.window_set = bmz_gameplay::judge::model::JudgeWindows::uniform(window);
        let now = TimeUs(timing * 1000);
        let outcome = if key == 1 || key == 3 {
            engine.process_input(
                chart,
                InputEvent {
                    lane,
                    kind: if key == 1 { InputKind::Press } else { InputKind::Release },
                    time: now,
                    source: InputSource::Replay,
                    device_kind: Default::default(),
                    scratch_direction: None,
                },
            )
        } else {
            engine.process_legacy_idle_lane(chart, lane, now, false)
        };
        let got: Vec<_> =
            outcome.events.iter().filter(|e| e.affects_score).map(|e| value(e.judge)).collect();
        for &grade in &got {
            bmz[grade] += 1;
        }
        for &grade in &poll.expected {
            reference[grade] += 1;
        }
        if got != poll.expected && first_difference.is_none() {
            first_difference = Some(json!({"poll_id":id,"judge_ms":timing,"lane":raw_lane,
                "key_state":key,"bmz":got,"reference":poll.expected}));
        }
    }
    ensure!(selected > 0, "no captured polls match the requested MD5 and seed");
    let status = if first_difference.is_none()
        && timing_differences.is_empty()
        && bmz[5] * 2 + bmz[4] == previous_score
    {
        "match"
    } else {
        "mismatch"
    };
    Ok(json!({"status":status,"bmz_ex":bmz[5]*2+bmz[4],"reference_ex":previous_score,
        "ex_delta":i64::from(bmz[5]*2+bmz[4])-i64::from(previous_score),
        "bmz_counts_ep_p_b_g_gr_pg":bmz,"reference_counts_ep_p_b_g_gr_pg":reference,
        "sequence_equal":first_difference.is_none(),"first_sequence_difference":first_difference,
        "note_timing_difference_count":timing_differences.len(),"captured_polls":selected,
        "model":"captured-openlr2-procnotetiming-arguments","trace_hash_present":trace_hash_present,
        "full_session_verified":false}))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn actual_poll_time_is_used_instead_of_the_replay_recording_clock() {
        let dir = tempfile::tempdir().unwrap();
        let path = Path::new("captured.bms");
        let bytes = b"#BPM 120\n#RANK 3\n#00111:01\n";
        let chart = bmz_chart::import::import_chart_bytes_with_assets(
            path,
            bytes,
            BmsRandomSource::Lr2Seed(1),
            AssetLookup::Declared,
        )
        .unwrap()
        .chart;
        let file = dir.path().join("capture.csv");
        let content = "kind,poll_id,judge_ms,lane,key_state,cursor_before,cursor_after,note_ms,tail_ms,active_before,active_after,judge,ex_before,ex_after,pg_ms,gr_ms,gd_ms,bad_ms,empty_poor_ms,md5,seed,key_mode,arrange,arrange_2p,fixed_key,fixed_key_2p,scratch_random,scratch_random_2p,assist,assist_2p,battle,dp_flip,pitch_on\nJ,9,2021,1,,,,,,,,5,,,,,,,\nP,9,2021,1,1,0,1,2000,0,0,-1,,0,2,21,60,120,200,1000,testhash,1,7,0,0,0,0,0,0,0,0,0,0,0\n";
        std::fs::write(&file, content).unwrap();
        let headers = headers_from_trace(&file, "testhash").unwrap();
        assert!(headers.iter().any(|r| r.op == 200 && r.value == 1));
        let window = bmz_gameplay::judge::window::note_judge_window_for_rule_mode(
            KeyMode::K7,
            RuleMode::Legacy,
        );
        let mut judge = JudgeEngine::new_with_rule_mode(window, RuleMode::Legacy);
        let result = compare_poll_trace(&mut judge, &chart, &file, "testhash", 1).unwrap();
        assert_eq!(result["status"], "match");
        assert_eq!(result["bmz_ex"], 2);
        assert_eq!(result["trace_hash_present"], true);
        assert!(headers_from_trace(&file, "different-hash").is_err());
        // The first trace build omitted its hash. An independently supplied
        // chart may still be checked, but the missing provenance stays visible.
        std::fs::write(&file, content.replace("testhash", "")).unwrap();
        let mut judge = JudgeEngine::new_with_rule_mode(window, RuleMode::Legacy);
        let result = compare_poll_trace(&mut judge, &chart, &file, "testhash", 1).unwrap();
        assert_eq!(result["status"], "match");
        assert_eq!(result["trace_hash_present"], false);
        // Unsupported options must be rejected, never reported as a match.
        let changed = content.trim_end().strip_suffix(",0").unwrap().to_owned() + ",1\n";
        std::fs::write(&file, changed).unwrap();
        assert!(headers_from_trace(&file, "testhash").is_err());
        // At 2022ms the same input would be GREAT. The captured argument is
        // 2021ms; no +/-1ms correction or expected-grade fitting is applied.
    }
}

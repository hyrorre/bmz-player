use super::*;

pub fn is_rian_ir_provider(provider: &str) -> bool {
    matches!(provider.trim().to_ascii_lowercase().as_str(), "rian-ir" | "rianir")
}

pub fn is_rian_ir_config(provider: &IrProviderConfig) -> bool {
    is_rian_ir_provider(&provider.provider) || is_rian_ir_provider(&provider.provider_key)
}

pub fn score_submission_supported(ln_policy: LnScorePolicy, double_option: DoubleOption) -> bool {
    ln_policy != LnScorePolicy::ForceHln
        && !matches!(double_option, DoubleOption::Battle | DoubleOption::BattleAutoScratch)
}

/// Mirrors rianIR's non-random clear-score duration guard before queueing a retry job.
pub fn score_duration_plausible(
    clear: &str,
    length_ms: Option<u64>,
    play_duration_ms: Option<u64>,
    has_random: bool,
) -> bool {
    let (Some(length_ms), Some(play_duration_ms)) = (length_ms, play_duration_ms) else {
        return true;
    };
    if has_random || matches!(clear, "NoPlay" | "Failed") || length_ms == 0 || play_duration_ms == 0
    {
        return true;
    }

    let length_ms = length_ms as f64;
    let play_duration_ms = play_duration_ms as f64;
    play_duration_ms <= length_ms * 1.15 + 15_000.0
        && play_duration_ms >= length_ms * 0.85 - 5_000.0
}

pub fn course_submission_supported(
    _ln_setting: LnPolicySetting,
    double_option: DoubleOption,
) -> bool {
    !matches!(double_option, DoubleOption::Battle | DoubleOption::BattleAutoScratch)
}

pub fn body_for_rule_mode(rule_mode: RuleMode) -> &'static str {
    match rule_mode {
        RuleMode::Beatoraja => "beatoraja",
        RuleMode::Lr2Oraja => "LR2oraja",
        RuleMode::Dx => "DX MODE",
    }
}

pub fn body_for_rule_name(rule_mode: &str) -> Result<&'static str> {
    match rule_mode {
        "Beatoraja" | "beatoraja" => Ok("beatoraja"),
        "Lr2Oraja" | "LR2oraja" => Ok("LR2oraja"),
        "Dx" | "DX MODE" => Ok("DX MODE"),
        other => bail!("unsupported rianIR rule mode '{other}'"),
    }
}

pub(super) fn ensure_score_payload_supported(payload: &IrScoreSubmission) -> Result<()> {
    if payload.chart.ln_profile.has_defined_hln || payload.rule.ln_policy == LnScorePolicy::ForceHln
    {
        bail!("rianIR does not accept HLN charts or scores");
    }
    let double =
        payload.play_options.get("applied_double_option").and_then(Value::as_str).unwrap_or("off");
    if matches!(double, "battle" | "battle_auto_scratch" | "battle_assist") {
        bail!("rianIR does not accept BATTLE / BATTLE AS scores");
    }
    Ok(())
}

pub(super) fn score_request(
    payload: &IrScoreSubmission,
    player_id: &str,
    api_token: &str,
) -> Result<Value> {
    let judges = aggregate_judges(payload.result.judges);
    let body = body_for_rule_name(&payload.rule.rule_mode)?;
    let song_ln_mode = source_ln_mode_id(payload.chart.ln_profile);
    let ln_mode = played_ln_mode_id(payload.chart.ln_profile, payload.rule.ln_policy);
    let played_at = payload.result.played_at;
    let mut request = json!({
        "player_name": player_id,
        "api_token": api_token,
        "client_hash": crate::ir::client_hash::current_client_hash(),
        "client_git_commit": option_env!("GIT_COMMIT_HASH").unwrap_or("unknown"),
        "client": "bmz-player",
        "client_version": env!("CARGO_PKG_VERSION"),
        "sha256": payload.chart.sha256,
        "md5": payload.chart.md5.as_deref().unwrap_or(""),
        // rianIR の既存 Java connector と同じ B64 prefix を使う。
        "song_title": b64(&payload.chart.title),
        "subtitle": b64(&payload.chart.subtitle),
        "genre": b64(&payload.chart.genre),
        "artist": b64(&payload.chart.artist),
        "subartist": b64(&payload.chart.subartists.join(", ")),
        "play_mode": play_mode(&payload.chart.mode),
        "song_ln_mode": song_ln_mode,
        "ln_mode": ln_mode,
        "minbpm": payload.chart.bpm.and_then(|bpm| bpm.min).unwrap_or(0.0),
        "maxbpm": payload.chart.bpm.and_then(|bpm| bpm.max).unwrap_or(0.0),
        "song_level": payload.chart.level.unwrap_or(0),
        "length": payload.chart.length_ms.map(|ms| ms as f64 / 1000.0).unwrap_or(0.0),
        "clear_type": clear_type_id(&payload.result.clear)?,
        "exscore": payload.result.ex_score,
        "maxcombo": payload.result.max_combo,
        "minbp": payload.result.min_bp,
        "date": played_at,
        "pgreat": judges.pgreat,
        "great": judges.great,
        "good": judges.good,
        "bad": judges.bad,
        "poor": judges.poor,
        "miss": judges.miss,
        "play_option": 0,
        "arrange_1p": arrange_value(payload, "arrange_1p"),
        "arrange_2p": arrange_value(payload, "arrange_2p"),
        "double_option": double_option_value(payload),
        "play_seed": seed_value(payload),
        "play_assist": 0,
        "play_gauge": gauge_type_id(&payload.rule.gauge)?,
        "total_notes": payload.result.notes,
        "play_duration": payload.result.duration_ms.map(|ms| ms as f64 / 1000.0).unwrap_or(0.0),
        "body": body,
    });
    // `length_ms` is also the version marker for duration-aware queue jobs. Jobs
    // serialized by older BMZ versions retain their legacy wire shape and do not
    // reinterpret chart end time as hardware-clock play duration.
    if let Some(length_ms) = payload.chart.length_ms {
        request["length_ms"] = json!(length_ms);
        request["play_duration_ms"] = json!(payload.result.duration_ms.unwrap_or_default());
        request["has_random"] = json!(payload.chart.features.random);
    }
    request["ln_mode_format"] = Value::String("canonical-v1".to_string());
    let signature = signature(
        api_token,
        &[
            player_id.to_string(),
            payload.chart.sha256.clone(),
            payload.result.ex_score.to_string(),
            payload.result.max_combo.to_string(),
            payload.result.min_bp.to_string(),
            played_at.to_string(),
        ],
    )?;
    request["signature"] = Value::String(signature);
    Ok(request)
}

pub(super) fn course_request(payload: &Value, player_id: &str, api_token: &str) -> Result<Value> {
    let course = payload.get("course").context("course payload is missing course")?;
    let rule = payload.get("rule").context("course payload is missing rule")?;
    let result = payload.get("result").context("course payload is missing result")?;
    let play_options = payload.get("play_options").unwrap_or(&Value::Null);
    let course_title = course.get("title").and_then(Value::as_str).unwrap_or("Unknown Course");
    let charts: Vec<String> = course
        .get("charts")
        .and_then(Value::as_array)
        .context("course payload is missing charts")?
        .iter()
        .map(|chart| {
            chart
                .as_str()
                .map(str::to_string)
                .context("course payload contains a non-string chart hash")
        })
        .collect::<Result<_>>()?;
    if charts.is_empty() {
        bail!("course payload has no chart hashes");
    }
    let course_hash = crate::ir::course_payload::compute_rian_course_hash_v1(course_title, &charts);
    let played_at = required_i64(result, "played_at")?;
    let ex_score = required_u64(result, "ex_score")?;
    let max_combo = required_u64(result, "max_combo")?;
    let bp = required_u64(result, "bp")?;
    let body = body_for_rule_name(required_str(rule, "rule_mode")?)?;
    let judges = result.get("judges").and_then(Value::as_object);
    let total_notes = result
        .get("total_notes")
        .and_then(Value::as_u64)
        .or_else(|| result.get("max_ex_score").and_then(Value::as_u64).map(|v| v / 2))
        .unwrap_or(0);
    let ln_policy = required_str(rule, "ln_policy")?;
    if !matches!(ln_policy, "AutoLn" | "AutoCn" | "AutoHcn" | "ForceLn" | "ForceCn" | "ForceHcn") {
        bail!("unsupported rianIR course LN policy '{ln_policy}'");
    }
    let ln_mode = match rule.get("effective_ln_mode").and_then(Value::as_u64) {
        Some(mode @ 0..=3) => mode as u8,
        Some(mode) => bail!("unsupported rianIR course effective LN mode '{mode}'"),
        None => effective_ln_mode_id_from_name(ln_policy)?,
    };
    let arrange =
        normalized_arrange(play_options.get("option").and_then(Value::as_str).unwrap_or("normal"));
    let mut request = json!({
        "player_name": player_id,
        "api_token": api_token,
        "client_hash": crate::ir::client_hash::current_client_hash(),
        "client_git_commit": option_env!("GIT_COMMIT_HASH").unwrap_or("unknown"),
        "client": "bmz-player",
        "client_version": env!("CARGO_PKG_VERSION"),
        "course_sha256": course_hash,
        "course_md5": "",
        "course_title": b64(course_title),
        "clear_type": clear_type_id(required_str(result, "clear")?)?,
        "exscore": ex_score,
        "maxcombo": max_combo,
        "minbp": bp,
        "date": played_at,
        "pgreat": judge_value(judges, "pgreat"),
        "great": judge_value(judges, "great"),
        "good": judge_value(judges, "good"),
        "bad": judge_value(judges, "bad"),
        "poor": judge_value(judges, "poor"),
        "miss": judge_value(judges, "empty_poor"),
        "play_option": 0,
        "arrange_1p": arrange,
        "arrange_2p": "normal",
        "double_option": "off",
        "play_seed": value_seed(play_options),
        "play_assist": 0,
        "play_gauge": gauge_type_id(required_str(rule, "gauge")?)?,
        "total_notes": total_notes,
        "ln_mode": ln_mode,
        "body": body,
        "constraint": constraint_names(course.get("constraints").unwrap_or(&Value::Null)),
        // BMZ の course queue は stage の hash だけを保持しており、曲名などを
        // 正確に再構成できない。rianIR 側へ誤った chart metadata を登録しないため、
        // 初期版では任意フィールドの tracks を空にする。
        "tracks": [],
    });
    request["ln_mode_format"] = Value::String("canonical-v1".to_string());
    request["signature"] = Value::String(signature(
        api_token,
        &[
            player_id.to_string(),
            course_hash.clone(),
            ex_score.to_string(),
            max_combo.to_string(),
            played_at.to_string(),
        ],
    )?);
    Ok(request)
}

#[derive(Debug, Clone, Copy)]
pub(super) struct AggregatedJudges {
    pgreat: u32,
    great: u32,
    good: u32,
    bad: u32,
    poor: u32,
    miss: u32,
}

pub(super) fn aggregate_judges(judges: IrJudgePayload) -> AggregatedJudges {
    AggregatedJudges {
        pgreat: judges.fast.pgreat.saturating_add(judges.slow.pgreat),
        great: judges.fast.great.saturating_add(judges.slow.great),
        good: judges.fast.good.saturating_add(judges.slow.good),
        bad: judges.fast.bad.saturating_add(judges.slow.bad),
        poor: judges.fast.poor.saturating_add(judges.slow.poor),
        // BMZ EmptyPoor is rianIR/beatoraja MISS.
        miss: judges.fast.empty_poor.saturating_add(judges.slow.empty_poor),
    }
}

pub(super) fn play_mode(mode: &str) -> String {
    match mode {
        "9K" => "popn-9k".to_string(),
        other if other.ends_with('K') => format!("beat-{}", other.to_ascii_lowercase()),
        other => other.to_string(),
    }
}

fn chart_ln_profile(profile: IrChartLnProfile) -> ChartLnProfile {
    ChartLnProfile {
        has_undefined_ln: profile.has_undefined_ln,
        has_defined_ln: profile.has_defined_ln,
        has_defined_cn: profile.has_defined_cn,
        has_defined_hcn: profile.has_defined_hcn,
        has_defined_hln: profile.has_defined_hln,
    }
}

fn ln_mode_id(mode: Option<LongNoteMode>) -> u8 {
    match mode {
        None => 0,
        Some(LongNoteMode::Ln) => 1,
        Some(LongNoteMode::Cn) => 2,
        Some(LongNoteMode::Hcn) => 3,
        Some(LongNoteMode::Hln) => 4,
    }
}

pub(super) fn source_ln_mode_id(profile: IrChartLnProfile) -> u8 {
    ln_mode_id(crate::ln_policy::source_ln_mode(chart_ln_profile(profile)))
}

pub(super) fn played_ln_mode_id(profile: IrChartLnProfile, policy: LnScorePolicy) -> u8 {
    ln_mode_id(crate::ln_policy::played_ln_mode(chart_ln_profile(profile), policy))
}

pub(super) fn effective_ln_mode_id_from_name(policy: &str) -> Result<u8> {
    match policy {
        "ForceLn" => Ok(1),
        "ForceCn" => Ok(2),
        "ForceHcn" => Ok(3),
        other => bail!("unsupported rianIR LN policy '{other}'"),
    }
}

pub(super) fn clear_type_id(clear: &str) -> Result<u8> {
    Ok(match clear {
        "NoPlay" => 0,
        "Failed" => 1,
        "AssistEasy" => 2,
        "LightAssistEasy" => 3,
        "Easy" => 4,
        "Normal" => 5,
        "Hard" => 6,
        "ExHard" => 7,
        "FullCombo" => 8,
        "Perfect" => 9,
        "Max" => 10,
        other => bail!("unsupported rianIR clear type '{other}'"),
    })
}

pub(super) fn clear_type_name(clear: i64) -> String {
    match clear {
        0 => "NoPlay",
        1 => "Failed",
        2 => "AssistEasy",
        3 => "LightAssistEasy",
        4 => "Easy",
        5 => "Normal",
        6 => "Hard",
        7 => "ExHard",
        8 => "FullCombo",
        9 => "Perfect",
        10 => "Max",
        _ => "NoPlay",
    }
    .to_string()
}

pub(super) fn gauge_type_id(gauge: &str) -> Result<u8> {
    Ok(match gauge {
        "AssistEasy" => 0,
        "Easy" => 1,
        "Normal" => 2,
        "Hard" => 3,
        "ExHard" => 4,
        "Hazard" => 5,
        "Class" => 6,
        "ExClass" => 7,
        "ExHardClass" => 8,
        other => bail!("unsupported rianIR gauge '{other}'"),
    })
}

pub(super) fn arrange_value(payload: &IrScoreSubmission, key: &str) -> String {
    normalized_arrange(payload.play_options.get(key).and_then(Value::as_str).unwrap_or("normal"))
}

pub(super) fn normalized_arrange(value: &str) -> String {
    match value.trim().to_ascii_lowercase().as_str() {
        "all-scr" | "allscratch" => "all-scratch".to_string(),
        other => other.to_string(),
    }
}

pub(super) fn double_option_value(payload: &IrScoreSubmission) -> String {
    payload
        .play_options
        .get("applied_double_option")
        .and_then(Value::as_str)
        .filter(|value| matches!(*value, "off" | "flip"))
        .unwrap_or("off")
        .to_string()
}

pub(super) fn seed_value(payload: &IrScoreSubmission) -> i64 {
    value_seed(&json!(payload.play_options))
}

pub(super) fn value_seed(play_options: &Value) -> i64 {
    ["random_seed", "seed"]
        .iter()
        .find_map(|key| {
            let value = play_options.get(*key)?;
            value.as_i64().or_else(|| value.as_str()?.parse().ok())
        })
        .unwrap_or(0)
}

pub(super) fn b64(value: &str) -> String {
    format!("B64:{}", base64::engine::general_purpose::STANDARD.encode(value.as_bytes()))
}

pub(super) fn signature(api_token: &str, fields: &[String]) -> Result<String> {
    let data = serde_json::to_string(fields)?;
    Ok(hmac_sha256_hex(api_token.as_bytes(), data.as_bytes()))
}

pub(super) fn hmac_sha256_hex(key: &[u8], data: &[u8]) -> String {
    const BLOCK: usize = 64;
    let mut normalized_key = [0_u8; BLOCK];
    if key.len() > BLOCK {
        normalized_key[..32].copy_from_slice(&Sha256::digest(key));
    } else {
        normalized_key[..key.len()].copy_from_slice(key);
    }
    let mut inner_pad = [0x36_u8; BLOCK];
    let mut outer_pad = [0x5c_u8; BLOCK];
    for index in 0..BLOCK {
        inner_pad[index] ^= normalized_key[index];
        outer_pad[index] ^= normalized_key[index];
    }
    let mut inner = Sha256::new();
    inner.update(inner_pad);
    inner.update(data);
    let inner_hash = inner.finalize();
    let mut outer = Sha256::new();
    outer.update(outer_pad);
    outer.update(inner_hash);
    outer.finalize().iter().map(|byte| format!("{byte:02x}")).collect()
}

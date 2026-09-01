//! コーススコア IR payload (docs/ir.md §19)。
//!
//! course identity はサーバーと同じ規則で
//! `SHA256(canonical_json({ charts, constraints }))` として計算する。
//! canonical 規則は tamper evidence と同じ「キー昇順 compact JSON」。

use bmz_core::input::InputDeviceKind;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use crate::screens::course_session::CourseResultSummary;
use crate::select_options::ArrangeOption;
use crate::storage::common::{hash_to_hex, hex_to_hash};

/// コース定義のうち identity / registry に必要な部分。
#[derive(Debug, Clone)]
pub struct IrCourseDefinition {
    /// 譜面 SHA256 (hex)、プレイ順。
    pub charts: Vec<String>,
    /// constraint 群 (class / speed / judge / gauge / ln)。
    pub constraints: Value,
    pub title: String,
    /// "dan" | "course"
    pub kind: String,
}

#[derive(Debug, Clone)]
pub struct IrCourseIdentity {
    pub definition: IrCourseDefinition,
    pub course_hash: String,
    /// rianIR/beatoraja connector互換:
    /// SHA256(UTF-8(decoded title + ordered chart SHA256 hex strings))。
    pub rian_course_hash_v1: String,
    /// BMS-IR/LR2互換の長いcourse key。BMZ内部identityには使わない。
    pub bms_ir_course_key: Option<String>,
    pub constraints_json: String,
    pub chart_sha256s_json: String,
    pub chart_sha256s: Vec<[u8; 32]>,
}

#[derive(Debug, Clone)]
pub struct IrCourseSubmissionContext {
    pub played_at: i64,
    /// コース全体の譜面 profile から正規化した LnScorePolicy の文字列表現。
    pub ln_policy: String,
    pub rule_mode: String,
    pub gauge: String,
    pub device_type: InputDeviceKind,
    pub arrange: String,
    pub random_seed: Option<i64>,
    pub idempotency_key: String,
    pub bms_ir_course_key: Option<String>,
}

const BMS_IR_DAN_COURSE_KEY_PREFIX: &str = "00000000002000000000000000005190";

pub fn compute_course_hash(definition: &IrCourseDefinition) -> String {
    let canonical = super::device_key::canonical_json_value(&json!({
        "charts": definition.charts,
        "constraints": definition.constraints,
    }))
    .unwrap_or_default();
    hash_to_hex(&Sha256::digest(canonical.as_bytes()))
}

/// rianIRのcourse送信・取得・URL生成で共通利用する互換hash。
///
/// `title` はB64 decode済みの表示文字列、`charts` はプレイ順とする。BMZ内部や
/// BMZ公式IRのcourse identityには使わない。
pub fn compute_rian_course_hash_v1(title: &str, charts: &[String]) -> String {
    let mut digest = Sha256::new();
    digest.update(title.as_bytes());
    for chart in charts {
        digest.update(chart.as_bytes());
    }
    hash_to_hex(&digest.finalize())
}

fn bms_ir_course_key(md5s: &[String]) -> String {
    format!("{BMS_IR_DAN_COURSE_KEY_PREFIX}{}", md5s.concat())
}

fn valid_bms_ir_table_course_key(value: &str, chart_count: usize) -> bool {
    let value = value.trim();
    value.len() == 32 * (chart_count + 1) && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

pub fn course_identity_from_stored(
    library_db: &crate::storage::library_db::LibraryDatabase,
    stored: &crate::storage::library_db::StoredCourse,
) -> Option<IrCourseIdentity> {
    let mut charts = Vec::with_capacity(stored.definition.entries.len());
    let mut chart_sha256s = Vec::with_capacity(stored.definition.entries.len());
    let mut chart_md5s = Vec::with_capacity(stored.definition.entries.len());
    for entry in &stored.definition.entries {
        let sha = entry.sha256.clone().or_else(|| {
            let md5 = entry.md5.as_ref()?;
            let md5 = crate::storage::common::hex_to_hash::<16>(md5).ok()?;
            let sha = library_db.chart_sha256_by_md5(md5).ok().flatten()?;
            Some(hash_to_hex(&sha))
        })?;
        let parsed = hex_to_hash::<32>(&sha).ok()?;
        let md5 = entry
            .md5
            .as_deref()
            .and_then(|value| hex_to_hash::<16>(value).ok())
            .map(|value| hash_to_hex(&value))
            .or_else(|| {
                library_db
                    .list_charts_by_sha256(parsed)
                    .ok()?
                    .first()
                    .map(|chart| hash_to_hex(&chart.md5))
            });
        charts.push(sha);
        chart_sha256s.push(parsed);
        chart_md5s.push(md5);
    }
    let definition = IrCourseDefinition {
        charts,
        constraints: serde_json::to_value(&stored.definition.constraints).ok()?,
        title: stored.definition.title.clone(),
        kind: match stored.definition.kind {
            bmz_core::course::CourseKind::Dan => "dan".to_string(),
            bmz_core::course::CourseKind::Course => "course".to_string(),
        },
    };
    let course_hash = compute_course_hash(&definition);
    let rian_course_hash_v1 = compute_rian_course_hash_v1(&definition.title, &definition.charts);
    let bms_ir_course_key =
        if stored.source.starts_with(crate::ir::table::BMS_IR_TABLE_SOURCE_PREFIX)
            && valid_bms_ir_table_course_key(&stored.definition.key, chart_md5s.len())
        {
            Some(stored.definition.key.trim().to_ascii_lowercase())
        } else {
            chart_md5s.into_iter().collect::<Option<Vec<_>>>().map(|md5s| bms_ir_course_key(&md5s))
        };
    let constraints_json = super::device_key::canonical_json_value(&definition.constraints).ok()?;
    let chart_sha256s_json =
        super::device_key::canonical_json_value(&json!(definition.charts)).ok()?;
    Some(IrCourseIdentity {
        definition,
        course_hash,
        rian_course_hash_v1,
        bms_ir_course_key,
        constraints_json,
        chart_sha256s_json,
        chart_sha256s,
    })
}

/// サーバーの `POST /api/v1/course-scores` payload を組み立てる。
pub fn build_course_submission(
    definition: &IrCourseDefinition,
    result: &CourseResultSummary,
    context: &IrCourseSubmissionContext,
) -> Value {
    let course_hash = compute_course_hash(definition);
    let entries: Vec<Value> = result
        .entry_summaries
        .iter()
        .zip(definition.charts.iter())
        .map(|(entry, sha256)| {
            json!({
                "sha256": sha256,
                "ex_score": entry.ex_score,
                "max_combo": entry.max_combo,
                "bp": entry.bp,
                "clear": entry.clear_type.as_str(),
                // canonical JSON の互換性 (Rust "62.0" vs JS "62") のため
                // float は payload に含めず、ゲージは整数 % に丸める。
                "gauge_end": entry.gauge_value.round() as i64,
            })
        })
        .collect();
    let trophies: Vec<&str> = result
        .trophy_results
        .iter()
        .filter(|trophy| trophy.achieved)
        .map(|trophy| trophy.name.as_str())
        .collect();
    let clear = course_result_clear_type(result).as_str();
    let gauge_value = result.final_gauge_value.round() as i64;
    let mut play_options = json!({
        "device_type": context.device_type.as_str(),
        "option": arrange_option_ir_from_persistent(&context.arrange),
    });
    if let Some(seed) = context.random_seed {
        play_options["random_seed"] = json!(seed.to_string());
        play_options["seed"] = json!(seed.to_string());
    }
    let entry_randomizations: Vec<Value> = result
        .entry_arranges
        .iter()
        .map(|arrange| {
            json!({
                "arrange_1p": arrange_option_ir(arrange.arrange),
                "arrange_2p": arrange_option_ir(arrange.arrange_2p),
                "seed": arrange
                    .packed_beatoraja_seed_from_sides()
                    .map(|seed| seed.to_string()),
                "seed_scheme": if arrange.legacy_seed {
                    crate::storage::replay::SEED_SCHEME_LEGACY_SHARED_V3
                } else {
                    crate::storage::replay::SEED_SCHEME_BEATORAJA_24BIT_V1
                },
                "bms_random_choices": arrange.bms_random_choices,
                "bms_switch_choices": arrange.bms_switch_choices,
            })
        })
        .collect();
    play_options["entry_randomizations"] = json!(entry_randomizations);

    let mut payload = json!({
        "client": {
            "name": "BMZ",
            "version": env!("CARGO_PKG_VERSION"),
            "platform": std::env::consts::OS,
        },
        "course": {
            "course_hash": course_hash,
            "title": definition.title,
            "kind": definition.kind,
            "charts": definition.charts,
            "constraints": definition.constraints,
        },
        "rule": {
            "gauge": context.gauge,
            "ln_policy": context.ln_policy,
            "effective_ln_mode": course_ln_mode_id(result.course_ln_mode),
            "rule_mode": context.rule_mode,
            "scoring": "bms_ex_score_v1",
        },
        "result": {
            "clear": clear,
            "course_clear": result.course_clear,
            "course_failed": result.course_failed,
            "played_entries": result.played_entries,
            "trophies": trophies,
            "ex_score": result.total_ex_score,
            "max_ex_score": result.max_ex_score,
            "total_notes": result.total_notes,
            "max_combo": result.course_max_combo,
            "bp": result.bp,
            "judges": {
                "pgreat": result.judge_counts.pgreat,
                "great": result.judge_counts.great,
                "good": result.judge_counts.good,
                "bad": result.judge_counts.bad,
                "poor": result.judge_counts.poor,
                "empty_poor": result.judge_counts.empty_poor,
            },
            "gauge_value": gauge_value,
            "entries": entries,
            "played_at": context.played_at,
        },
        "play_options": play_options,
        "idempotency_key": context.idempotency_key,
    });
    if let Some(course_key) = &context.bms_ir_course_key {
        payload["course"]["course_key"] = json!(course_key);
    }
    payload
}

const fn course_ln_mode_id(mode: Option<bmz_chart::model::LongNoteMode>) -> u8 {
    match mode {
        None => 0,
        Some(bmz_chart::model::LongNoteMode::Ln) => 1,
        Some(bmz_chart::model::LongNoteMode::Cn) => 2,
        Some(bmz_chart::model::LongNoteMode::Hcn) => 3,
    }
}

fn arrange_option_ir_from_persistent(value: &str) -> String {
    arrange_option_ir(ArrangeOption::from_persistent_str(value))
}

fn arrange_option_ir(value: ArrangeOption) -> String {
    value.as_str().to_ascii_lowercase()
}

fn course_result_clear_type(result: &CourseResultSummary) -> bmz_core::clear::ClearType {
    use bmz_core::clear::{ClearType, GaugeType};

    if result.course_failed {
        return ClearType::Failed;
    }
    if result.played_entries == 0 {
        return ClearType::NoPlay;
    }

    match result.final_gauge_type {
        GaugeType::AssistEasy => ClearType::AssistEasy,
        GaugeType::Easy => ClearType::Easy,
        GaugeType::Normal | GaugeType::Class => ClearType::Normal,
        GaugeType::Hard | GaugeType::ExClass => ClearType::Hard,
        GaugeType::ExHard | GaugeType::Hazard | GaugeType::ExHardClass => ClearType::ExHard,
    }
}

#[cfg(test)]
mod tests {
    use bmz_core::clear::{ClearType, GaugeType};
    use bmz_core::course::CourseKind;
    use bmz_core::lane::KeyMode;

    use crate::ln_policy::LnScorePolicy;
    use crate::screens::result_model::{
        ResultFastSlowJudgeCounts, ResultJudgeCounts, ResultSummary,
    };

    use super::*;

    #[test]
    fn course_hash_is_stable_and_constraint_sensitive() {
        let base = IrCourseDefinition {
            charts: vec!["ab".repeat(32), "cd".repeat(32)],
            constraints: json!({ "gauge": "Class", "ln": "Off" }),
            title: "Dan 1".to_string(),
            kind: "dan".to_string(),
        };
        let same = compute_course_hash(&base);
        assert_eq!(same.len(), 64);
        assert_eq!(same, compute_course_hash(&base));

        let mut reordered = base.clone();
        reordered.charts.reverse();
        assert_ne!(same, compute_course_hash(&reordered));

        let mut other_constraint = base.clone();
        other_constraint.constraints = json!({ "gauge": "ExClass", "ln": "Off" });
        assert_ne!(same, compute_course_hash(&other_constraint));

        // タイトルは identity に影響しない。
        let mut renamed = base.clone();
        renamed.title = "Renamed".to_string();
        assert_eq!(same, compute_course_hash(&renamed));
    }

    #[test]
    fn rian_course_hash_v1_matches_beatoraja_title_and_ordered_charts() {
        let charts = vec!["ab".repeat(32), "cd".repeat(32)];
        assert_eq!(
            compute_rian_course_hash_v1("段位", &charts),
            "c3a672ab2881fdd8efb583ff04e94fa88c9ff730941eb72063dadc59101f6d77"
        );

        let mut reversed = charts.clone();
        reversed.reverse();
        assert_ne!(
            compute_rian_course_hash_v1("段位", &charts),
            compute_rian_course_hash_v1("段位", &reversed)
        );
        assert_ne!(
            compute_rian_course_hash_v1("段位", &charts),
            compute_rian_course_hash_v1("別名", &charts)
        );
    }

    #[test]
    fn rian_course_hash_v1_does_not_include_constraints() {
        let charts = vec!["ab".repeat(32)];
        let normal = IrCourseDefinition {
            charts: charts.clone(),
            constraints: json!({ "judge": "normal" }),
            title: "Course".to_string(),
            kind: "course".to_string(),
        };
        let no_good =
            IrCourseDefinition { constraints: json!({ "judge": "no_good" }), ..normal.clone() };

        assert_eq!(
            compute_rian_course_hash_v1(&normal.title, &normal.charts),
            compute_rian_course_hash_v1(&no_good.title, &no_good.charts)
        );
    }

    #[test]
    fn course_hash_uses_canonical_json_number_formatting() {
        let definition = IrCourseDefinition {
            charts: vec!["ab".repeat(32)],
            constraints: json!({
                "total": 160.0,
                "fraction": 4.50,
                "small": 1e-6,
            }),
            title: "Dan 1".to_string(),
            kind: "dan".to_string(),
        };
        let canonical = crate::ir::device_key::canonical_json_value(&json!({
            "charts": definition.charts.clone(),
            "constraints": definition.constraints.clone(),
        }))
        .unwrap();

        assert_eq!(
            canonical,
            format!(
                "{{\"charts\":[\"{}\"],\"constraints\":{{\"fraction\":4.5,\"small\":0.000001,\"total\":160}}}}",
                "ab".repeat(32)
            )
        );
        assert_eq!(
            compute_course_hash(&definition),
            crate::storage::common::hash_to_hex(&Sha256::digest(canonical.as_bytes()))
        );
    }

    #[test]
    fn course_submission_uses_canonical_ln_policy_and_course_max_combo() {
        let definition = IrCourseDefinition {
            charts: vec!["ab".repeat(32)],
            constraints: json!({ "gauge": "Class" }),
            title: "Dan 1".to_string(),
            kind: "dan".to_string(),
        };
        let result = CourseResultSummary {
            course_id: 1,
            course_score_id: None,
            course_played_at: None,
            ln_policy: LnScorePolicy::ForceHcn,
            rule_mode: bmz_gameplay::rule::RuleMode::Beatoraja,
            title: "Dan 1".to_string(),
            kind: CourseKind::Dan,
            course_titles: Default::default(),
            entry_summaries: Vec::new(),
            entry_arranges: Vec::new(),
            total_ex_score: 0,
            max_ex_score: 0,
            total_notes: 0,
            course_ln_mode: Some(bmz_chart::model::LongNoteMode::Hcn),
            bp: 0,
            final_clear_type: bmz_core::clear::ClearType::NoPlay,
            final_gauge_type: bmz_core::clear::GaugeType::Class,
            final_gauge_value: 0.0,
            course_max_combo: 123,
            judge_counts: ResultJudgeCounts::default(),
            trophy_results: Vec::new(),
            course_clear: false,
            course_failed: false,
            total_entries: 0,
            played_entries: 0,
            replay_slots: [false; 4],
            saved_replay_slots: [false; 4],
            best_score: None,
            previous_best_score: None,
        };
        let payload = build_course_submission(
            &definition,
            &result,
            &IrCourseSubmissionContext {
                played_at: 1_767_225_600,
                ln_policy: LnScorePolicy::ForceHcn.as_str().to_string(),
                rule_mode: "Dx".to_string(),
                gauge: "Class".to_string(),
                device_type: InputDeviceKind::Keyboard,
                arrange: "NORMAL".to_string(),
                random_seed: None,
                idempotency_key: "course-test".to_string(),
                bms_ir_course_key: Some("ab".repeat(32)),
            },
        );

        assert_eq!(payload["rule"]["ln_policy"], "ForceHcn");
        assert_eq!(payload["course"]["course_key"], "ab".repeat(32));
        assert_eq!(payload["rule"]["effective_ln_mode"], 3);
        assert_eq!(payload["rule"]["rule_mode"], "Dx");
        assert_eq!(payload["result"]["max_combo"], json!(123));
        assert_eq!(payload["result"]["clear"], json!("NoPlay"));
    }

    #[test]
    fn course_submission_uses_final_course_gauge_for_result_lamp() {
        let definition = IrCourseDefinition {
            charts: vec!["ab".repeat(32)],
            constraints: json!({ "gauge": "ExClass" }),
            title: "Dan 1".to_string(),
            kind: "dan".to_string(),
        };
        let result = CourseResultSummary {
            course_id: 1,
            course_score_id: None,
            course_played_at: None,
            ln_policy: LnScorePolicy::AutoLn,
            rule_mode: bmz_gameplay::rule::RuleMode::Beatoraja,
            title: "Dan 1".to_string(),
            kind: CourseKind::Dan,
            course_titles: Default::default(),
            entry_summaries: vec![stage_summary(ClearType::NoPlay, 0.0)],
            entry_arranges: Vec::new(),
            total_ex_score: 1234,
            max_ex_score: 2000,
            total_notes: 1000,
            course_ln_mode: None,
            bp: 0,
            final_clear_type: ClearType::Hard,
            final_gauge_type: GaugeType::ExClass,
            final_gauge_value: 66.4,
            course_max_combo: 456,
            judge_counts: ResultJudgeCounts::default(),
            trophy_results: Vec::new(),
            course_clear: true,
            course_failed: false,
            total_entries: 1,
            played_entries: 1,
            replay_slots: [false; 4],
            saved_replay_slots: [false; 4],
            best_score: None,
            previous_best_score: None,
        };

        let payload = build_course_submission(
            &definition,
            &result,
            &IrCourseSubmissionContext {
                played_at: 1_767_225_600,
                ln_policy: LnScorePolicy::AutoLn.as_str().to_string(),
                rule_mode: "Beatoraja".to_string(),
                gauge: "ExClass".to_string(),
                device_type: InputDeviceKind::Keyboard,
                arrange: "NORMAL".to_string(),
                random_seed: None,
                idempotency_key: "course-final-clear".to_string(),
                bms_ir_course_key: Some("ab".repeat(32)),
            },
        );

        assert_eq!(payload["result"]["clear"], json!("Hard"));
        assert_eq!(payload["result"]["gauge_value"], json!(66));
        assert_eq!(payload["result"]["entries"][0]["clear"], json!("NoPlay"));
        assert_eq!(payload["rule"]["effective_ln_mode"], 0);
    }

    #[test]
    fn course_submission_keeps_course_lamp_without_a_trophy() {
        let definition = IrCourseDefinition {
            charts: vec!["ab".repeat(32), "cd".repeat(32)],
            constraints: json!({ "gauge": "Hard" }),
            title: "Dan 1".to_string(),
            kind: "dan".to_string(),
        };
        let result = CourseResultSummary {
            course_id: 1,
            course_score_id: None,
            course_played_at: None,
            ln_policy: LnScorePolicy::AutoLn,
            rule_mode: bmz_gameplay::rule::RuleMode::Beatoraja,
            title: "Dan 1".to_string(),
            kind: CourseKind::Dan,
            course_titles: Default::default(),
            entry_summaries: vec![
                stage_summary(ClearType::NoPlay, 80.0),
                stage_summary(ClearType::FullCombo, 100.0),
            ],
            entry_arranges: Vec::new(),
            total_ex_score: 1234,
            max_ex_score: 2000,
            total_notes: 1000,
            course_ln_mode: None,
            bp: 0,
            final_clear_type: ClearType::NoPlay,
            final_gauge_type: GaugeType::Hard,
            final_gauge_value: 66.4,
            course_max_combo: 456,
            judge_counts: ResultJudgeCounts::default(),
            trophy_results: Vec::new(),
            course_clear: false,
            course_failed: false,
            total_entries: 2,
            played_entries: 2,
            replay_slots: [false; 4],
            saved_replay_slots: [false; 4],
            best_score: None,
            previous_best_score: None,
        };

        let payload = build_course_submission(
            &definition,
            &result,
            &IrCourseSubmissionContext {
                played_at: 1_767_225_600,
                ln_policy: LnScorePolicy::AutoLn.as_str().to_string(),
                rule_mode: "Beatoraja".to_string(),
                gauge: "Hard".to_string(),
                device_type: InputDeviceKind::Keyboard,
                arrange: "NORMAL".to_string(),
                random_seed: None,
                idempotency_key: "course-separated-clear".to_string(),
                bms_ir_course_key: Some("ab".repeat(32)),
            },
        );

        assert_eq!(payload["result"]["clear"], json!("Hard"));
        assert_eq!(payload["result"]["course_clear"], json!(false));
        assert_eq!(payload["result"]["entries"][0]["clear"], json!("NoPlay"));
        assert_eq!(payload["result"]["entries"][1]["clear"], json!("FullCombo"));
    }

    #[test]
    fn course_submission_marks_failed_course_as_failed() {
        let definition = IrCourseDefinition {
            charts: vec!["ab".repeat(32)],
            constraints: json!({ "gauge": "ExHardClass" }),
            title: "Dan 1".to_string(),
            kind: "dan".to_string(),
        };
        let result = CourseResultSummary {
            course_id: 1,
            course_score_id: None,
            course_played_at: None,
            ln_policy: LnScorePolicy::AutoLn,
            rule_mode: bmz_gameplay::rule::RuleMode::Beatoraja,
            title: "Dan 1".to_string(),
            kind: CourseKind::Dan,
            course_titles: Default::default(),
            entry_summaries: vec![stage_summary(ClearType::NoPlay, 0.0)],
            entry_arranges: Vec::new(),
            total_ex_score: 1234,
            max_ex_score: 2000,
            total_notes: 1000,
            course_ln_mode: None,
            bp: 789,
            final_clear_type: ClearType::NoPlay,
            final_gauge_type: GaugeType::ExHardClass,
            final_gauge_value: 0.0,
            course_max_combo: 456,
            judge_counts: ResultJudgeCounts::default(),
            trophy_results: Vec::new(),
            course_clear: false,
            course_failed: true,
            total_entries: 1,
            played_entries: 1,
            replay_slots: [false; 4],
            saved_replay_slots: [false; 4],
            best_score: None,
            previous_best_score: None,
        };

        let payload = build_course_submission(
            &definition,
            &result,
            &IrCourseSubmissionContext {
                played_at: 1_767_225_600,
                ln_policy: LnScorePolicy::AutoLn.as_str().to_string(),
                rule_mode: "Beatoraja".to_string(),
                gauge: "ExHardClass".to_string(),
                device_type: InputDeviceKind::Keyboard,
                arrange: "NORMAL".to_string(),
                random_seed: None,
                idempotency_key: "course-failed".to_string(),
                bms_ir_course_key: Some("ab".repeat(32)),
            },
        );

        assert_eq!(payload["result"]["clear"], json!("Failed"));
        assert_eq!(payload["result"]["bp"], json!(789));
    }

    fn stage_summary(clear_type: ClearType, gauge_value: f32) -> ResultSummary {
        ResultSummary {
            clear_type,
            skin_attempt: Default::default(),
            target_name: String::new(),
            arrange: "NORMAL".to_string(),
            arrange_2p: "NORMAL".to_string(),
            lane_shuffle_pattern: Vec::new(),
            ex_score: 0,
            max_combo: 0,
            bp: 0,
            cb: 0,
            gauge_value,
            gauge_type: GaugeType::ExClass,
            total_notes: 0,
            duration_ms: 0,
            initial_bpm: 0.0,
            min_bpm: 0.0,
            max_bpm: 0.0,
            main_bpm: 0.0,
            total_gauge: 0.0,
            judge_rank: None,
            key_mode: KeyMode::K7,
            has_long_notes: false,
            long_note_mode: bmz_chart::model::LongNoteMode::Ln,
            judge_counts: ResultJudgeCounts::default(),
            fast_slow_counts: ResultFastSlowJudgeCounts::default(),
            replay_path: String::new(),
            replay_slots: [false; 4],
            saved_replay_slots: [false; 4],
            score_history_id: 0,
            best_ex_score: None,
            best_clear_type: None,
            best_max_combo: None,
            best_bp: None,
            previous_best_ex_score: None,
            previous_best_clear_type: None,
            previous_best_max_combo: None,
            previous_best_bp: None,
            target_ex_score: None,
            target_max_combo: None,
            target_bp: None,
            target_clear_type: None,
            ir_queued_jobs: 0,
            ir_last_error: None,
            title: String::new(),
            subtitle: String::new(),
            artist: String::new(),
            subartist: String::new(),
            genre: String::new(),
            difficulty_name: String::new(),
            play_level: String::new(),
            graph: Default::default(),
        }
    }
}

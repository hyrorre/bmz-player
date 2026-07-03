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
    pub constraints_json: String,
    pub chart_sha256s_json: String,
    pub chart_sha256s: Vec<[u8; 32]>,
}

#[derive(Debug, Clone)]
pub struct IrCourseSubmissionContext {
    pub played_at: i64,
    /// LnPolicySetting の文字列表現 (コースは譜面ごとに解決が変わるため設定値)。
    pub ln_policy_setting: String,
    pub rule_mode: String,
    pub gauge: String,
    pub device_type: InputDeviceKind,
    pub arrange: String,
    pub random_seed: Option<i64>,
    pub idempotency_key: String,
}

pub fn compute_course_hash(definition: &IrCourseDefinition) -> String {
    let canonical = super::device_key::canonical_json_value(&json!({
        "charts": definition.charts,
        "constraints": definition.constraints,
    }))
    .unwrap_or_default();
    hash_to_hex(&Sha256::digest(canonical.as_bytes()))
}

pub fn course_identity_from_stored(
    library_db: &crate::storage::library_db::LibraryDatabase,
    stored: &crate::storage::library_db::StoredCourse,
) -> Option<IrCourseIdentity> {
    let mut charts = Vec::with_capacity(stored.definition.entries.len());
    let mut chart_sha256s = Vec::with_capacity(stored.definition.entries.len());
    for entry in &stored.definition.entries {
        let sha = entry.sha256.clone().or_else(|| {
            let md5 = entry.md5.as_ref()?;
            let md5 = crate::storage::common::hex_to_hash::<16>(md5).ok()?;
            let sha = library_db.chart_sha256_by_md5(md5).ok().flatten()?;
            Some(hash_to_hex(&sha))
        })?;
        let parsed = hex_to_hash::<32>(&sha).ok()?;
        charts.push(sha);
        chart_sha256s.push(parsed);
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
    let constraints_json = super::device_key::canonical_json_value(&definition.constraints).ok()?;
    let chart_sha256s_json =
        super::device_key::canonical_json_value(&json!(definition.charts)).ok()?;
    Some(IrCourseIdentity {
        definition,
        course_hash,
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
    let bp = result.judge_counts.bad + result.judge_counts.poor + result.judge_counts.empty_poor;
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
    let clear = if result.course_failed { "Failed" } else { result.final_clear_type.as_str() };
    let gauge_value = result.final_gauge_value.round() as i64;
    let mut play_options = json!({
        "device_type": context.device_type.as_str(),
        "option": arrange_option_ir_from_persistent(&context.arrange),
    });
    if let Some(seed) = context.random_seed {
        play_options["random_seed"] = json!(seed);
        play_options["seed"] = json!(seed);
    }

    json!({
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
            "ln_policy": context.ln_policy_setting,
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
            "max_combo": result.course_max_combo,
            "bp": bp,
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
    })
}

fn arrange_option_ir_from_persistent(value: &str) -> String {
    ArrangeOption::from_persistent_str(value).as_str().to_ascii_lowercase()
}

#[cfg(test)]
mod tests {
    use bmz_core::clear::{ClearType, GaugeType};
    use bmz_core::course::CourseKind;
    use bmz_core::lane::KeyMode;

    use crate::ln_policy::LnPolicySetting;
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
            rule_mode: bmz_gameplay::rule::RuleMode::Beatoraja,
            title: "Dan 1".to_string(),
            kind: CourseKind::Dan,
            course_titles: Default::default(),
            entry_summaries: Vec::new(),
            entry_arranges: Vec::new(),
            total_ex_score: 0,
            max_ex_score: 0,
            total_notes: 0,
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
                ln_policy_setting: LnPolicySetting::ForceHcn.as_ir_str().to_string(),
                rule_mode: "Dx".to_string(),
                gauge: "Class".to_string(),
                device_type: InputDeviceKind::Keyboard,
                arrange: "NORMAL".to_string(),
                random_seed: None,
                idempotency_key: "course-test".to_string(),
            },
        );

        assert_eq!(payload["rule"]["ln_policy"], "ForceHcn");
        assert_eq!(payload["rule"]["rule_mode"], "Dx");
        assert_eq!(payload["result"]["max_combo"], json!(123));
    }

    #[test]
    fn course_submission_uses_final_course_clear_for_result_lamp() {
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
            rule_mode: bmz_gameplay::rule::RuleMode::Beatoraja,
            title: "Dan 1".to_string(),
            kind: CourseKind::Dan,
            course_titles: Default::default(),
            entry_summaries: vec![stage_summary(ClearType::NoPlay, 0.0)],
            entry_arranges: Vec::new(),
            total_ex_score: 1234,
            max_ex_score: 2000,
            total_notes: 1000,
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
                ln_policy_setting: LnPolicySetting::AutoLn.as_ir_str().to_string(),
                rule_mode: "Beatoraja".to_string(),
                gauge: "ExClass".to_string(),
                device_type: InputDeviceKind::Keyboard,
                arrange: "NORMAL".to_string(),
                random_seed: None,
                idempotency_key: "course-final-clear".to_string(),
            },
        );

        assert_eq!(payload["result"]["clear"], json!("Hard"));
        assert_eq!(payload["result"]["gauge_value"], json!(66));
        assert_eq!(payload["result"]["entries"][0]["clear"], json!("NoPlay"));
    }

    fn stage_summary(clear_type: ClearType, gauge_value: f32) -> ResultSummary {
        ResultSummary {
            clear_type,
            arrange: "NORMAL".to_string(),
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

//! Synchronous BMS class updates. This is not a course score import.
use anyhow::{Context, Result, bail, ensure};
use bmz_core::{
    clear::{ClearType, GaugeType},
    course::*,
    lane::KeyMode,
};
use bmz_gameplay::rule::RuleMode;
use serde::{Deserialize, Serialize};

use crate::config::profile_config::{IrProviderConfig, KeyModeConversionConfig};
use crate::ln_policy::LnScorePolicy;
use crate::screens::course_session::ActiveCourseSession;
use crate::select_options::{ArrangeOption, DoubleOption};
use crate::storage::network_db::{IrScoreJobRecord, NetworkDatabase};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StageEvidence {
    pub chart_md5: String,
    pub chart_sha256: String,
    pub score_history_id: i64,
    pub played_at: i64,
    pub source_key_mode: Option<KeyMode>,
    pub primary_key_mode: KeyMode,
    pub conversion: KeyModeConversionConfig,
    pub native_scoring: bool,
    pub ln_policy: LnScorePolicy,
    pub rule_mode: RuleMode,
    pub gauge: GaugeType,
    pub clear: ClearType,
    pub gauge_value: f32,
    /// Actual values carried into the next stage, including gauges that were
    /// not selected for this stage's Result by automatic gauge switching.
    pub gauge_carry: Vec<(GaugeType, f32)>,
    pub completed: bool,
    pub normal_score: bool,
    pub has_bms_random: bool,
    pub arrange: ArrangeOption,
    pub arrange_2p: ArrangeOption,
    pub double_option: DoubleOption,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct DanPlay {
    course_id: i64,
    course_key: String,
    release: bool,
    constraints: CourseConstraints,
    ln_policy: LnScorePolicy,
    rule_mode: RuleMode,
    final_gauge: GaugeType,
    final_clear: ClearType,
    course_clear: bool,
    course_failed: bool,
    stages: [StageEvidence; 4],
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct DanRequest {
    course: DanCourse,
    score: DanScore,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct DanCourse {
    charts: [DanChart; 4],
    constraint: Vec<String>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct DanChart {
    md5: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct DanScore {
    clear: ClearType,
    option: u32,
    lntype: u8,
}

/// Saved separately from mutable network state. Missing old-format provenance
/// cannot be rebuilt from the current library, profile, or course definition.
#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct DanJob {
    version: u32,
    // Local outbox identity only; never sent as an API idempotency header.
    idempotency_key: String,
    origin: String,
    account_id: String,
    course_score_id: i64,
    candidate: Candidate,
}
#[derive(Debug, Serialize, Deserialize)]
enum Candidate {
    Eligible { play: Box<DanPlay>, request: DanRequest },
    Ineligible { reason: String },
}

fn valid_hash(value: &str, len: usize) -> bool {
    value.len() == len
        && value.bytes().all(|b| b.is_ascii_hexdigit())
        && value.bytes().any(|b| b != b'0')
}

pub(crate) fn capture(course: &ActiveCourseSession) -> Result<DanPlay> {
    ensure!(
        course.definition.entries.len() == 4 && course.entry_results.len() == 4,
        "Bokutachi dan: four actually played stages are required"
    );
    ensure!(
        !course.score_save_disabled && course.replay_stage_limit.is_none(),
        "Bokutachi dan: this course play cannot be submitted"
    );
    let mut stages = Vec::with_capacity(4);
    for (defined, played) in course.definition.entries.iter().zip(&course.entry_results) {
        ensure!(
            defined.chart_id == Some(played.chart_id),
            "Bokutachi dan: played chart order differs from the course"
        );
        let stage = played
            .finished
            .bokutachi_dan_stage
            .clone()
            .context("Bokutachi dan: missing original stage evidence")?;
        ensure!(
            defined.md5.as_ref().is_none_or(|v| v.eq_ignore_ascii_case(&stage.chart_md5))
                && defined
                    .sha256
                    .as_ref()
                    .is_none_or(|v| v.eq_ignore_ascii_case(&stage.chart_sha256)),
            "Bokutachi dan: played chart identity differs from the course"
        );
        stages.push(stage);
    }
    let summary = course.clone().into_result();
    let play = DanPlay {
        course_id: course.course_id,
        course_key: course.definition.key.clone(),
        release: course.definition.release,
        constraints: course.definition.constraints.clone(),
        ln_policy: course.ln_policy,
        rule_mode: course.rule_mode,
        final_gauge: summary.final_gauge_type,
        final_clear: summary.final_clear_type,
        course_clear: summary.course_clear,
        course_failed: summary.course_failed,
        stages: stages.try_into().map_err(|_| anyhow::anyhow!("missing dan stages"))?,
    };
    play.request()?;
    Ok(play)
}

/// Keep every declared constraint, including unknown ones. Do not erase a
/// forbidden CN/HCN/GAUGE constraint when the effective LN policy is ForceLn.
fn constraints(value: &CourseConstraints) -> Vec<String> {
    value
        .source_constraints
        .iter()
        .map(String::as_str)
        .chain(value.canonical_names())
        .map(|name| {
            match name {
                "grade" => "CLASS",
                "grade_mirror" => "MIRROR",
                "grade_random" => "RANDOM",
                "gauge_5k" => "GAUGE_5KEYS",
                "gauge_7k" => "GAUGE_7KEYS",
                "gauge_9k" => "GAUGE_9KEYS",
                "gauge_24k" => "GAUGE_24KEYS",
                other => other,
            }
            .to_ascii_uppercase()
        })
        .fold(Vec::new(), |mut names, name| {
            if !names.contains(&name) {
                names.push(name);
            }
            names
        })
}

/// Only arrangements with an exact beatoraja representation are accepted here.
/// The Direct Manual F/MF -> RANDOM metadata conversion is unrelated.
fn packed_option(stage: &StageEvidence) -> Result<u32> {
    let side = |a| match a {
        ArrangeOption::Normal => Ok(0),
        ArrangeOption::Mirror => Ok(1),
        _ => bail!("Bokutachi dan: RANDOM and other arrangements are not supported"),
    };
    let dp = match stage.double_option {
        DoubleOption::Off => 0,
        DoubleOption::Flip => 1,
        _ => bail!("Bokutachi dan: BATTLE is not supported"),
    };
    let dp_options = if stage.primary_key_mode == KeyMode::K14 {
        10 * side(stage.arrange_2p)? + 100 * dp
    } else {
        // beatoraja packs the unused second side/DP settings only for DP.
        0
    };
    Ok(side(stage.arrange)? + dp_options)
}

fn dan_gauge(gauge: GaugeType) -> bool {
    matches!(gauge, GaugeType::Class | GaugeType::ExClass | GaugeType::ExHardClass)
}
fn cleared(clear: ClearType) -> bool {
    matches!(
        clear,
        ClearType::Normal
            | ClearType::Hard
            | ClearType::ExHard
            | ClearType::FullCombo
            | ClearType::Perfect
            | ClearType::Max
    )
}

impl DanPlay {
    fn request(&self) -> Result<DanRequest> {
        ensure!(self.release, "Bokutachi dan: IR submission is disabled for this course");
        ensure!(
            self.ln_policy == LnScorePolicy::ForceLn && self.rule_mode == RuleMode::Lr2Oraja,
            "Bokutachi dan: saved LN-only and LR2oraja rules are required"
        );
        ensure!(
            self.course_clear
                && !self.course_failed
                && cleared(self.final_clear)
                && dan_gauge(self.final_gauge),
            "Bokutachi dan: a completed dan-gauge clear is required"
        );
        let constraint = constraints(&self.constraints);
        ensure!(
            !constraint
                .iter()
                .any(|v| v == "CN" || v == "HCN" || (v.starts_with("GAUGE_") && v != "GAUGE_LR2")),
            "Bokutachi dan: CN, HCN, and non-LR2 gauge constraints are not supported"
        );
        let mode = self.stages[0].primary_key_mode;
        ensure!(
            matches!(mode, KeyMode::K7 | KeyMode::K14),
            "Bokutachi dan: only native BMS 7K/14K courses are supported"
        );
        let option = packed_option(&self.stages[0])?;
        ensure!(
            option <= 1,
            "Bokutachi dan: the API accepts only packed option 0 or 1 (no 2P MIRROR or FLIP)"
        );
        for stage in &self.stages {
            ensure!(
                valid_hash(&stage.chart_md5, 32) && valid_hash(&stage.chart_sha256, 64),
                "Bokutachi dan: original chart MD5/SHA256 is missing or invalid"
            );
            ensure!(
                stage.source_key_mode == Some(mode)
                    && stage.primary_key_mode == mode
                    && stage.conversion == KeyModeConversionConfig::Off
                    && stage.native_scoring,
                "Bokutachi dan: every stage must use the same native scoring mode"
            );
            ensure!(
                stage.completed
                    && stage.normal_score
                    && stage.score_history_id > 0
                    && stage.played_at >= 0
                    && !stage.has_bms_random
                    && cleared(stage.clear)
                    && stage.gauge_value.is_finite()
                    && stage.gauge_value > 0.0,
                "Bokutachi dan: a stage is incomplete, assisted, branched, or not a valid clear"
            );
            // LR2oraja selects LR2 gauge definitions regardless of a missing
            // GAUGE_LR2 string. Check the actual saved rule and dan gauge.
            ensure!(
                stage.ln_policy == LnScorePolicy::ForceLn
                    && stage.rule_mode == self.rule_mode
                    && dan_gauge(stage.gauge),
                "Bokutachi dan: stage LN/rule/gauge is not eligible"
            );
            let carried = stage
                .gauge_carry
                .iter()
                .filter(|(gauge, _)| *gauge == self.final_gauge)
                .collect::<Vec<_>>();
            ensure!(
                carried.len() == 1 && carried[0].1.is_finite() && carried[0].1 > 0.0,
                "Bokutachi dan: final course gauge did not survive every stage"
            );
            ensure!(packed_option(stage)? == option, "Bokutachi dan: stage arrangements differ");
            ensure!(
                self.constraints.class != CourseClassConstraint::Grade || option == 0,
                "Bokutachi dan: MIRROR is disallowed by this course"
            );
        }
        Ok(DanRequest {
            course: DanCourse {
                charts: self
                    .stages
                    .each_ref()
                    .map(|s| DanChart { md5: s.chart_md5.to_ascii_lowercase() }),
                constraint,
            },
            score: DanScore { clear: self.final_clear, option, lntype: 0 },
        })
    }
}

impl DanJob {
    pub(crate) fn new(
        provider: &IrProviderConfig,
        course_score_id: i64,
        play: &Result<DanPlay>,
    ) -> Result<Self> {
        let candidate = match play {
            Ok(play) => {
                Candidate::Eligible { play: Box::new(play.clone()), request: play.request()? }
            }
            Err(error) => Candidate::Ineligible { reason: format!("{error:#}") },
        };
        Ok(Self {
            version: 1,
            idempotency_key: format!("bmz-bokutachi-dan-{course_score_id}"),
            origin: crate::ir::credentials::credential_origin(&provider.base_url)?,
            account_id: provider.account_id.clone(),
            course_score_id,
            candidate,
        })
    }

    pub(crate) fn ineligible_reason(&self) -> Option<&str> {
        match &self.candidate {
            Candidate::Ineligible { reason } => Some(reason),
            _ => None,
        }
    }
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum DanOutcome {
    Updated { set: String, value: String },
    Unchanged { set: String, value: String },
    NotCleared,
}

pub(crate) fn parse_response(response: serde_json::Value) -> Result<DanOutcome> {
    ensure!(
        response.get("success").and_then(|v| v.as_bool()) == Some(true),
        "Bokutachi dan: API did not report success"
    );
    match response.get("description").and_then(|v| v.as_str()) {
        Some("Class not updated, as you didn't clear this course.") => {
            ensure!(
                response.get("body").is_none_or(|b| b.is_null()),
                "Bokutachi dan: unexpected not-cleared response body"
            );
            Ok(DanOutcome::NotCleared)
        }
        Some(description @ ("Successfully updated class." | "Class not updated.")) => {
            let body = response.get("body").context("Bokutachi dan: missing class result")?;
            let field = |key| {
                body.get(key)
                    .and_then(|v| v.as_str())
                    .filter(|v| !v.is_empty())
                    .map(str::to_string)
                    .context("Bokutachi dan: invalid class result")
            };
            let (set, value) = (field("set")?, field("value")?);
            if description == "Successfully updated class." {
                Ok(DanOutcome::Updated { set, value })
            } else {
                Ok(DanOutcome::Unchanged { set, value })
            }
        }
        _ => bail!("Bokutachi dan: unknown response; no new class confirmed"),
    }
}

pub(crate) async fn step(
    db: &mut NetworkDatabase,
    root: &std::path::Path,
    provider: &IrProviderConfig,
    job: &IrScoreJobRecord,
    now: i64,
    report: &mut crate::ir::sync::IrSyncReport,
) -> Result<()> {
    let saved: DanJob = serde_json::from_str(&job.payload_json)
        .context("Bokutachi dan: missing or invalid saved course evidence")?;
    ensure!(
        saved.version == 1
            && saved.course_score_id > 0
            && saved.idempotency_key == format!("bmz-bokutachi-dan-{}", job.local_score_id)
            && saved.course_score_id == job.local_score_id
            && saved.account_id == job.account_id
            && saved.origin == crate::ir::credentials::credential_origin(&provider.base_url)?,
        "Bokutachi dan: saved course account/origin differs"
    );
    let (play, request) = match &saved.candidate {
        Candidate::Eligible { play, request } => (play, request),
        Candidate::Ineligible { reason } => {
            return Err(super::client::IneligibleChart(reason.clone()).into());
        }
    };
    ensure!(
        play.ln_policy == job.ln_policy && play.request()? == *request,
        "Bokutachi dan: saved request differs from played course"
    );
    let credentials = crate::ir::credentials::load_credentials(root, &job.provider, &saved.origin)?
        .context("Bokutachi dan: configure and validate the API key to resume")?;
    ensure!(
        credentials.account_id == job.account_id,
        "Bokutachi dan: restore this account's API key"
    );
    if let Some(next) = db.reserve_bokutachi_post(&saved.origin, now)? {
        db.defer_bokutachi_job(
            job.id,
            now,
            next,
            "Bokutachi dan: waiting for class update rate limit",
        )?;
        return Ok(());
    }
    let result = super::BokutachiClient::new(&saved.origin)?
        .submit_dan(&credentials.access_token, request)
        .await
        .map_err(|error| {
            if crate::ir::http_error::status_code_from_error(&error)
                == Some(reqwest::StatusCode::NOT_FOUND)
            {
                super::client::IneligibleChart(
                    "Bokutachi dan: unsupported course; class was not updated".into(),
                )
                .into()
            } else {
                error
            }
        })?;
    let (updated, message) = match &result {
        DanOutcome::Updated { set, value } => {
            (true, format!("Bokutachi: class updated ({set}: {value})"))
        }
        DanOutcome::Unchanged { set, value } => {
            (false, format!("Bokutachi: class update completed with no change ({set}: {value})"))
        }
        DanOutcome::NotCleared => {
            (false, "Bokutachi: class not updated; server did not accept this as a clear".into())
        }
    };
    // No score ID, score ledger, replay child, import ID, or poll is involved.
    db.finish_bokutachi_dan(job.id, now, updated, &message)?;
    report.messages.push(message);
    Ok(())
}

#[cfg(test)]
pub(crate) mod tests;

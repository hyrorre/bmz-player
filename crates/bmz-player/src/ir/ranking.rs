//! Screen-independent ranking transport. UI caches and submission attempts stay in screens.
use super::bmz_official::{BmzOfficialIrClient, IrCourseRankingRequest, IrRankingRequest};
use super::sync::ensure_fresh_credentials;
use super::types::{IrCourseRankingResult, IrRankingResult, IrRankingScope};
use crate::ln_policy::LnScorePolicy;
use crate::select_options::DoubleOptionScoreBucket;
use bmz_gameplay::rule::RuleMode;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct ChartRankingQuery {
    pub profile_root: PathBuf,
    pub provider: String,
    pub base_url: String,
    pub chart_sha256_hex: String,
    pub ln_policy: LnScorePolicy,
    pub double_option: DoubleOptionScoreBucket,
    pub rule_mode: RuleMode,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CourseRankingTarget {
    pub course_hash: String,
    pub rian_course_hash_v1: String,
    pub bms_ir_course_key: Option<String>,
    pub gauge: String,
    pub ln_policy: String,
    pub rule_mode: RuleMode,
}

pub(crate) async fn fetch_course_ranking(
    profile_root: &Path,
    provider: &str,
    base_url: &str,
    target: &CourseRankingTarget,
) -> anyhow::Result<IrCourseRankingResult> {
    if crate::ir::rian_ir::is_rian_ir_provider(provider) {
        return crate::ir::rian_ir::RianIrClient::new(base_url)?
            .fetch_course_ranking(
                &target.rian_course_hash_v1,
                crate::ir::rian_ir::body_for_rule_mode(target.rule_mode),
                crate::ir::rian_ir::RIAN_IR_RANKING_LIMIT,
            )
            .await;
    }
    if crate::ir::bms_ir::is_bms_ir_provider(provider) {
        let credentials = crate::ir::sync::ensure_fresh_credentials(
            profile_root,
            provider,
            base_url,
            now_unix_seconds(),
        )
        .await?;
        return crate::ir::bms_ir::BmsIrClient::new(base_url)?
            .fetch_course_ranking(
                &target.course_hash,
                target.bms_ir_course_key.as_deref().unwrap_or(""),
                &IrCourseRankingRequest {
                    gauge: target.gauge.clone(),
                    ln_policy: target.ln_policy.clone(),
                    limit: 20,
                },
                target.rule_mode,
                &credentials.account_id,
                &credentials.access_token,
            )
            .await;
    }
    let client = BmzOfficialIrClient::anonymous(base_url)?;
    client
        .fetch_course_ranking(
            &target.course_hash,
            &IrCourseRankingRequest {
                gauge: target.gauge.clone(),
                ln_policy: target.ln_policy.clone(),
                limit: 20,
            },
        )
        .await
}

pub(crate) async fn fetch_ranking(
    query: &ChartRankingQuery,
    scope: IrRankingScope,
) -> anyhow::Result<IrRankingResult> {
    let limit = result_ranking_limit(&query.provider);
    fetch_ranking_with_limit(query, scope, limit).await
}

pub(crate) fn result_ranking_limit(provider: &str) -> u32 {
    if crate::ir::rian_ir::is_rian_ir_provider(provider) {
        crate::ir::rian_ir::RIAN_IR_RANKING_LIMIT
    } else {
        crate::ir::types::default_ranking_limit()
    }
}

pub(crate) async fn fetch_ranking_with_limit(
    query: &ChartRankingQuery,
    scope: IrRankingScope,
    limit: u32,
) -> anyhow::Result<IrRankingResult> {
    let now = now_unix_seconds();
    if crate::ir::rian_ir::is_rian_ir_provider(&query.provider) {
        let credentials =
            ensure_fresh_credentials(&query.profile_root, &query.provider, &query.base_url, now)
                .await
                .ok();
        return crate::ir::rian_ir::RianIrClient::new(&query.base_url)?
            .fetch_ranking(
                &query.chart_sha256_hex,
                crate::ir::rian_ir::body_for_rule_mode(query.rule_mode),
                scope,
                limit.min(crate::ir::rian_ir::RIAN_IR_RANKING_LIMIT),
                credentials.as_ref().map(|credentials| credentials.account_id.as_str()),
            )
            .await;
    }
    if crate::ir::bms_ir::is_bms_ir_provider(&query.provider) {
        let credentials =
            ensure_fresh_credentials(&query.profile_root, &query.provider, &query.base_url, now)
                .await?;
        return crate::ir::bms_ir::BmsIrClient::new(&query.base_url)?
            .fetch_ranking(
                &query.chart_sha256_hex,
                &IrRankingRequest {
                    scope,
                    ln_policy: query.ln_policy.as_str().to_string(),
                    double_option: query.double_option,
                    rule_mode: query.rule_mode,
                    limit,
                    offset: 0,
                },
                &credentials.account_id,
                &credentials.access_token,
            )
            .await;
    }
    let mut client = BmzOfficialIrClient::anonymous(&query.base_url)?;
    // self / rivals scope は認証必須。global は匿名でも可。
    match ensure_fresh_credentials(&query.profile_root, &query.provider, &query.base_url, now).await
    {
        Ok(credentials) => client.set_access_token(credentials.access_token),
        Err(error) if scope != IrRankingScope::Global => return Err(error),
        Err(_) => {}
    }
    client
        .fetch_ranking(
            &query.chart_sha256_hex,
            &IrRankingRequest {
                scope,
                ln_policy: query.ln_policy.as_str().to_string(),
                double_option: query.double_option,
                rule_mode: query.rule_mode,
                limit,
                offset: 0,
            },
        )
        .await
}

fn now_unix_seconds() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

use super::download::{now_unix_seconds, parse_scope, primary_provider};
use super::jobs::{sync_cli_jobs, sync_cli_jobs_for_kind};
use super::*;

pub(super) async fn ranking(
    app_paths: &AppPaths,
    profile_paths: &ProfilePaths,
    profile: &ProfileConfig,
    sha256: &str,
    ln_policy: &str,
    scope: &str,
    limit: u32,
) -> Result<()> {
    let provider = primary_provider(profile)?;
    if crate::ir::bms_ir::is_bms_ir_config(provider)
        || crate::ir::rian_ir::is_rian_ir_config(provider)
    {
        let hash = crate::storage::common::hex_to_hash::<32>(sha256)?;
        let provenance = crate::storage::library_db::LibraryDatabase::open(&app_paths.library_db)?
            .list_charts_by_sha256(hash)?
            .into_iter()
            .next()
            .map(|chart| chart.kaleid)
            .unwrap_or_default();
        if let Some(reason) = crate::ir::kaleid::block_reason(provider, &provenance) {
            bail!(reason);
        }
    }
    let scope = parse_scope(scope)?;
    let now = now_unix_seconds();
    if crate::ir::rian_ir::is_rian_ir_config(provider) {
        if scope != IrRankingScope::Global {
            bail!("rianIR supports --scope global only");
        }
        let provider_key = crate::ir::provider_key::configured_provider_key(provider)
            .context("IR provider key is not set; log in again")?;
        let credentials = ensure_fresh_credentials(
            profile_paths.root_dir.as_path(),
            provider_key,
            &provider.base_url,
            now,
        )
        .await
        .ok();
        let result = crate::ir::rian_ir::RianIrClient::new(&provider.base_url)?
            .fetch_ranking(
                sha256,
                crate::ir::rian_ir::body_for_rule_mode(profile.play.rule_mode),
                scope,
                limit,
                credentials.as_ref().map(|credentials| credentials.account_id.as_str()),
            )
            .await?;
        print_ranking(&result, ln_policy, profile.play.rule_mode);
        return Ok(());
    }
    if crate::ir::bms_ir::is_bms_ir_config(provider) {
        let provider_key = crate::ir::provider_key::configured_provider_key(provider)
            .context("IR provider key is not set; log in again")?;
        let credentials = ensure_fresh_credentials(
            profile_paths.root_dir.as_path(),
            provider_key,
            &provider.base_url,
            now,
        )
        .await?;
        let result = crate::ir::bms_ir::BmsIrClient::new(&provider.base_url)?
            .fetch_ranking(
                sha256,
                &IrRankingRequest {
                    scope,
                    ln_policy: ln_policy.to_string(),
                    double_option: crate::select_options::DoubleOptionScoreBucket::Off,
                    rule_mode: profile.play.rule_mode,
                    limit,
                    offset: 0,
                },
                &credentials.account_id,
                &credentials.access_token,
            )
            .await?;
        print_ranking(&result, ln_policy, profile.play.rule_mode);
        return Ok(());
    }
    let mut client = BmzOfficialIrClient::anonymous(&provider.base_url)?;
    if let Some(provider_key) = crate::ir::provider_key::configured_provider_key(provider)
        && let Ok(credentials) = ensure_fresh_credentials(
            profile_paths.root_dir.as_path(),
            provider_key,
            &provider.base_url,
            now,
        )
        .await
    {
        client.set_access_token(credentials.access_token);
    }

    let result = client
        .fetch_ranking(
            sha256,
            &IrRankingRequest {
                scope,
                ln_policy: ln_policy.to_string(),
                double_option: crate::select_options::DoubleOptionScoreBucket::Off,
                rule_mode: profile.play.rule_mode,
                limit,
                offset: 0,
            },
        )
        .await?;

    print_ranking(&result, ln_policy, profile.play.rule_mode);
    Ok(())
}

fn print_ranking(
    result: &crate::ir::types::IrRankingResult,
    ln_policy: &str,
    rule_mode: bmz_gameplay::rule::RuleMode,
) {
    println!("chart: {}", result.chart.sha256);
    if result.ranking.entries.is_empty() {
        println!("no scores for ln_policy={ln_policy} rule_mode={}", rule_mode.as_str());
        return;
    }
    println!("{:>4}  {:<24} {:>7} {:<16} {:>6} {:>5}", "#", "player", "EX", "clear", "combo", "bp");
    for entry in &result.ranking.entries {
        println!(
            "{:>4}  {:<24} {:>7} {:<16} {:>6} {:>5}",
            entry.rank,
            entry.player.display_name,
            entry.score.ex_score,
            entry.score.clear,
            entry.score.max_combo,
            entry.score.min_bp,
        );
    }
    if let Some(own) = &result.ranking.self_summary {
        println!("self rank: {}", own.rank);
    }
}

pub(super) async fn sync(
    app_paths: &AppPaths,
    profile_paths: &ProfilePaths,
    profile: &ProfileConfig,
) -> Result<()> {
    crate::storage::migration::migrate_score_db(&profile_paths.score_db)?;
    crate::storage::migration::migrate_network_db(&profile_paths.network_db)?;
    let mut network_db = NetworkDatabase::open(&profile_paths.network_db)?;
    let report = sync_cli_jobs(
        &mut network_db,
        &profile_paths.score_db,
        profile_paths.root_dir.as_path(),
        app_paths.logs_dir.as_path(),
        &profile.ir,
        IR_CLI_SYNC_BATCH_LIMIT,
    )
    .await?;
    println!("submitted: {}, failed: {}", report.submitted, report.failed);
    for message in &report.messages {
        println!("  {message}");
    }
    Ok(())
}

pub(super) async fn attest_submitted_scores(
    app_paths: &AppPaths,
    profile_paths: &ProfilePaths,
    profile: &ProfileConfig,
    provider: Option<&str>,
    sync_after_enqueue: bool,
    all: bool,
) -> Result<()> {
    crate::storage::migration::migrate_score_db(&profile_paths.score_db)?;
    crate::storage::migration::migrate_network_db(&profile_paths.network_db)?;

    let (provider_key, account_id) = resolve_local_upload_target(&profile.ir, provider)?;
    let mut network_db = NetworkDatabase::open(&profile_paths.network_db)?;
    let enqueued = network_db.enqueue_ir_score_attestation_jobs(
        &provider_key,
        &account_id,
        now_unix_seconds(),
    )?;
    println!("provider: {provider_key}");
    println!("account: {account_id}");
    println!("queued score attestations: {enqueued}");

    let mut remaining = network_db.unfinished_ir_score_job_count_for_kind(
        &provider_key,
        &account_id,
        IrJobKind::Attestation,
    )?;
    if !sync_after_enqueue || remaining == 0 {
        println!("remaining queued score attestations: {remaining}");
        return Ok(());
    }

    let mut submitted = 0_u32;
    loop {
        let report = sync_cli_jobs_for_kind(
            &mut network_db,
            &profile_paths.score_db,
            profile_paths.root_dir.as_path(),
            app_paths.logs_dir.as_path(),
            &profile.ir,
            &provider_key,
            &account_id,
            IrJobKind::Attestation,
            IR_CLI_SYNC_BATCH_LIMIT,
        )
        .await?;
        submitted = submitted.saturating_add(report.submitted);
        println!("submitted: {}, failed: {}", report.submitted, report.failed);
        for message in &report.messages {
            println!("  {message}");
        }
        remaining = network_db.unfinished_ir_score_job_count_for_kind(
            &provider_key,
            &account_id,
            IrJobKind::Attestation,
        )?;
        println!("remaining queued score attestations: {remaining}");
        if report.failed > 0 {
            bail!(
                "score attestation stopped after {submitted} submissions because {} jobs failed",
                report.failed
            );
        }
        if !all || remaining == 0 {
            return Ok(());
        }
        if report.submitted == 0 {
            bail!(
                "score attestation is waiting for an existing sending job; rerun after its 5-minute lease expires"
            );
        }
    }
}

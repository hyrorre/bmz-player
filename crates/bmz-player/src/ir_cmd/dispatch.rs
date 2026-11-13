pub async fn run_ir_command(cmd: IrCommand) -> Result<()> {
    let app_paths = resolve_app_paths()?;
    run_ir_command_with_paths(cmd, &app_paths).await
}

pub async fn run_ir_command_with_paths(cmd: IrCommand, app_paths: &AppPaths) -> Result<()> {
    run_ir_command_with_paths_and_profile(cmd, app_paths, None).await
}

pub async fn run_ir_command_with_paths_and_profile(
    cmd: IrCommand,
    app_paths: &AppPaths,
    profile_id: Option<&str>,
) -> Result<()> {
    let (profile_paths, mut profile) = load_active_profile_with_paths(app_paths, profile_id)?;
    match cmd {
        IrCommand::Login { email, password, base_url, provider } => {
            login(&profile_paths, &mut profile, &provider, &email, password, base_url).await
        }
        IrCommand::Logout { provider } => logout(&profile_paths, &mut profile, &provider).await,
        IrCommand::Status => status(&profile_paths, &profile).await,
        IrCommand::Ranking { sha256, ln_policy, scope, limit } => {
            ranking(app_paths, &profile_paths, &profile, &sha256, &ln_policy, &scope, limit).await
        }
        IrCommand::Sync => sync(app_paths, &profile_paths, &profile).await,
        IrCommand::UploadLocal {
            provider,
            limit,
            dry_run,
            sync: sync_after_enqueue,
            all,
            resend,
            include_course_stages,
            include_replay,
        } => {
            let options = IrLocalUploadOptions {
                provider,
                limit,
                dry_run,
                resend,
                include_course_stages,
                include_replay,
            };
            upload_local(app_paths, &profile_paths, &profile, options, sync_after_enqueue, all)
                .await
        }
        IrCommand::DownloadScores { provider, limit, dry_run } => {
            download_scores(
                app_paths,
                &profile_paths,
                &profile,
                IrScoreDownloadOptions { provider, limit, dry_run },
            )
            .await
        }
        IrCommand::AttestSubmitted { provider, sync, all } => {
            attest_submitted_scores(
                app_paths,
                &profile_paths,
                &profile,
                provider.as_deref(),
                sync,
                all,
            )
            .await
        }
        IrCommand::CleanupImported { provider, apply } => {
            cleanup_imported_scores(&profile_paths, &profile, provider.as_deref(), apply).await
        }
        IrCommand::CleanupDuplicate { history_id, provider, apply } => {
            cleanup_duplicate_score_history(
                &profile_paths,
                &profile,
                provider.as_deref(),
                history_id,
                apply,
            )
            .await
        }
        IrCommand::Rivals { action } => rivals(&profile_paths, &mut profile, action).await,
        IrCommand::DeviceKey { rotate } => device_key(&profile_paths, &profile, rotate).await,
        IrCommand::Replay { score_id } => replay(&profile_paths, &profile, &score_id).await,
    }
}
use super::account::{device_key, load_active_profile_with_paths, replay, rivals};
use super::auth::{login, logout, status};
use super::cleanup::{cleanup_duplicate_score_history, cleanup_imported_scores};
use super::download::download_scores;
use super::ranking::{attest_submitted_scores, ranking, sync};
use super::upload::upload_local;
use super::*;

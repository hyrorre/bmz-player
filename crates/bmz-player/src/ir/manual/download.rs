use std::path::Path;

use anyhow::{Context, Result, bail};

use crate::config::profile_config::IrProviderConfig;
use crate::ir::bmz_official::BmzOfficialIrClient;
use crate::ir::download::{
    IrOwnScoreHistoryClient, IrScoreDownloadOptions, IrScoreDownloadReport,
    download_ir_scores_with_hooks,
};
use crate::ir::sync::ensure_fresh_credentials;
use crate::storage::network_db::NetworkDatabase;
use crate::storage::score_db::ScoreDatabase;

use super::{
    IrManualCancel, IrManualProgress, IrManualProgressSink, IrSyncContext, now_unix_seconds,
    selected_provider,
};

/// `bmz ir download-scores` 相当。取得したページごとに `DownloadPage` を通知する。
///
/// このfutureは `NetworkDatabase` への参照を保持したまま待機するため `Send` ではない。
/// 専用スレッド上の `block_on` などで実行すること。
pub async fn download_scores(
    ctx: &IrSyncContext<'_>,
    network_db_path: &Path,
    options: &IrScoreDownloadOptions,
    progress: IrManualProgressSink<'_>,
    cancel: &IrManualCancel,
) -> Result<IrScoreDownloadReport> {
    let provider = selected_provider(ctx.ir_config, options.provider.as_deref())?.clone();
    if crate::ir::bokutachi::is_config(&provider) {
        return Err(crate::ir::bokutachi::unsupported());
    }
    if !provider.enabled {
        bail!("IR provider '{}' is disabled", provider.provider);
    }
    let provider_key = crate::ir::provider_key::configured_provider_key(&provider)
        .context("IR provider key is not set; log in again")?
        .to_string();
    let credentials = ensure_fresh_credentials(
        ctx.profile_root,
        &provider_key,
        &provider.base_url,
        now_unix_seconds(),
    )
    .await?;
    ensure_download_account(&provider, &credentials.account_id)?;
    let client = if crate::ir::bms_ir::is_bms_ir_config(&provider) {
        IrOwnScoreHistoryClient::BmsIr {
            client: crate::ir::bms_ir::BmsIrClient::new(&provider.base_url)?,
            player_id: credentials.account_id.clone(),
            game_token: credentials.access_token,
        }
    } else {
        IrOwnScoreHistoryClient::BmzOfficial(BmzOfficialIrClient::new(
            &provider.base_url,
            credentials.access_token,
        )?)
    };
    let mut score_db = ScoreDatabase::open(ctx.score_db)?;
    let network_db = NetworkDatabase::open(network_db_path)?;
    progress(IrManualProgress::DownloadStarted {
        provider_key: provider_key.clone(),
        account_id: credentials.account_id.clone(),
    });
    download_ir_scores_with_hooks(
        &client,
        &provider_key,
        &credentials.account_id,
        &mut score_db,
        &network_db,
        options,
        now_unix_seconds(),
        &mut |page_report| progress(IrManualProgress::DownloadPage(page_report.clone())),
        Some(cancel),
    )
    .await
}

/// 取り込んだscoreのsourceは認証情報のaccountで記録し、upload-localは設定のaccountで
/// 「取得元へ送り返さない」判定をする。両者がずれると判定が効かなくなるため、
/// 設定と異なるaccountでは取得しない。
fn ensure_download_account(provider: &IrProviderConfig, signed_in_account: &str) -> Result<()> {
    let configured = provider.account_id.trim();
    if !configured.is_empty() && configured != signed_in_account {
        bail!(
            "signed-in IR account '{signed_in_account}' differs from the configured account '{configured}'; run `bmz ir login` again"
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn download_requires_the_configured_account() {
        let mut provider = IrProviderConfig::bmz_ir();
        provider.account_id = "account-a".to_string();
        assert!(ensure_download_account(&provider, "account-a").is_ok());

        let error = ensure_download_account(&provider, "account-b").unwrap_err();
        assert!(error.to_string().contains("differs from the configured account"));

        provider.account_id.clear();
        assert!(ensure_download_account(&provider, "account-b").is_ok());
    }
}

use super::download::{now_unix_seconds, primary_provider};
/// `ir replay <SCORE_ID>` — IR リプレイをダウンロードし、hash を検証して
/// プロファイル配下に保存する。`--boot-replay-file` でそのまま再生できる。
use super::*;

pub(super) async fn replay(
    profile_paths: &ProfilePaths,
    profile: &ProfileConfig,
    score_id: &str,
) -> Result<()> {
    use sha2::{Digest, Sha256};

    let provider = primary_provider(profile)?;
    if crate::ir::bms_ir::is_bms_ir_config(provider) {
        bail!("BMS-IR does not provide replay file downloads");
    }
    let client = BmzOfficialIrClient::anonymous(&provider.base_url)?;
    let (bytes, declared_hash) = client.download_replay(score_id).await?;

    let actual_hash = crate::storage::common::hash_to_hex(&Sha256::digest(&bytes));
    if !declared_hash.is_empty() && actual_hash != declared_hash {
        bail!("downloaded replay hash mismatch: expected {declared_hash}, got {actual_hash}");
    }

    let dir = profile_paths.replay_dir.join("ir");
    std::fs::create_dir_all(&dir)?;
    let path = dir.join(format!("{score_id}.toml"));
    std::fs::write(&path, &bytes)?;

    println!("saved replay: {} ({} bytes, sha256 {actual_hash})", path.display(), bytes.len());
    println!("play it with: bmz --boot-replay-file {}", path.display());
    Ok(())
}

/// `ir device-key` — 署名鍵の表示。`rotate` で旧鍵を失効し新しい鍵を登録する。
pub(super) async fn device_key(
    profile_paths: &ProfilePaths,
    profile: &ProfileConfig,
    rotate: bool,
) -> Result<()> {
    use crate::ir::device_key::{load_or_create_device_key, rotate_registered_device_key};

    let provider = primary_provider(profile)?;
    if crate::ir::bms_ir::is_bms_ir_config(provider) {
        bail!("BMS-IR does not use BMZ device keys");
    }
    let provider_key = crate::ir::provider_key::configured_provider_key(provider)
        .context("IR provider key is not set; log in again")?;
    let root = profile_paths.root_dir.as_path();
    let key = load_or_create_device_key(root, provider_key)?;

    if !rotate {
        println!("provider: {}", provider.provider);
        println!("endpoint key: {provider_key}");
        println!("public key: {}", key.public_key);
        println!("server key id: {}", key.key_id.as_deref().unwrap_or("(not registered)"));
        return Ok(());
    }

    let credentials =
        ensure_fresh_credentials(root, provider_key, &provider.base_url, now_unix_seconds())
            .await?;
    let client = BmzOfficialIrClient::new(&provider.base_url, credentials.access_token)?;

    let new_key = rotate_registered_device_key(root, provider_key, &client).await?;
    let key_id = new_key.key_id.as_deref().unwrap_or("(not registered)");

    println!("rotated device key for {}", provider.provider);
    println!("endpoint key: {provider_key}");
    println!("public key: {}", new_key.public_key);
    println!("server key id: {key_id}");
    Ok(())
}

pub(super) async fn rivals(
    profile_paths: &ProfilePaths,
    profile: &mut ProfileConfig,
    action: Option<RivalAction>,
) -> Result<()> {
    let provider = primary_provider(profile)?.clone();
    let provider_key = crate::ir::provider_key::configured_provider_key(&provider)
        .context("IR provider key is not set; log in again")?
        .to_string();
    if crate::ir::rian_ir::is_rian_ir_provider(&provider.provider) {
        if action.is_some() {
            bail!("rianIR rivals can be added or removed only on the rianIR website");
        }
        let account_id = provider.account_id.trim();
        if account_id.is_empty() {
            bail!("rianIR account ID is not set; log in again");
        }
        let rivals = crate::ir::rian_ir::RianIrClient::new(&provider.base_url)?
            .fetch_rivals(account_id)
            .await?;
        if sync_ir_rivals_into_profile(profile, &provider_key, &rivals) {
            profile.updated_at = now_unix_seconds();
            save_profile_config(&profile_paths.profile_toml, profile)?;
        }
        print_rivals(&rivals);
        return Ok(());
    }
    if crate::ir::bms_ir::is_bms_ir_config(&provider) {
        if action.is_some() {
            bail!("BMS-IR rivals can be added or removed only on the BMS-IR website");
        }
        let credentials = ensure_fresh_credentials(
            profile_paths.root_dir.as_path(),
            &provider_key,
            &provider.base_url,
            now_unix_seconds(),
        )
        .await?;
        let response = crate::ir::bms_ir::BmsIrClient::new(&provider.base_url)?
            .get_rivals(&credentials.account_id, &credentials.access_token)
            .await?;
        if sync_ir_rivals_into_profile(profile, &provider_key, &response.rivals) {
            profile.updated_at = now_unix_seconds();
            save_profile_config(&profile_paths.profile_toml, profile)?;
        }
        print_rivals(&response.rivals);
        return Ok(());
    }
    let credentials = ensure_fresh_credentials(
        profile_paths.root_dir.as_path(),
        &provider_key,
        &provider.base_url,
        now_unix_seconds(),
    )
    .await?;
    let client = BmzOfficialIrClient::new(&provider.base_url, credentials.access_token)?;

    match action {
        Some(RivalAction::Add { player_id }) => {
            client.set_rival(&player_id, true).await?;
            println!("added rival: {player_id}");
        }
        Some(RivalAction::Remove { player_id }) => {
            client.set_rival(&player_id, false).await?;
            println!("removed rival: {player_id}");
        }
        None => {}
    }

    let response = client.get_rivals().await?;
    if sync_ir_rivals_into_profile(profile, &provider_key, &response.rivals) {
        profile.updated_at = now_unix_seconds();
        save_profile_config(&profile_paths.profile_toml, profile)?;
    }

    print_rivals(&response.rivals);
    Ok(())
}

fn print_rivals(rivals: &[crate::ir::types::IrRivalEntry]) {
    if rivals.is_empty() {
        println!("no rivals registered");
        return;
    }
    for rival in rivals {
        let name = rival
            .profile
            .as_ref()
            .map(|profile| profile.display_name.as_str())
            .filter(|name| !name.is_empty())
            .unwrap_or("(no name)");
        println!("- {name} ({})", rival.player_id);
    }
}

/// IR のライバル一覧をプロファイルの `RivalConfig` に同期する。
///
/// `source = Ir` かつ同一 provider のエントリだけを対象とし、サーバーに
/// 存在しないものは削除、新規は追加、表示名は更新する。手動登録された
/// LocalProfile / ExternalFile のエントリには触らない。
/// 変更があった場合に true を返す。
pub fn sync_ir_rivals_into_profile(
    profile: &mut ProfileConfig,
    provider: &str,
    rivals: &[crate::ir::types::IrRivalEntry],
) -> bool {
    use crate::config::profile_config::{RivalEntry, RivalSourceConfig};

    let mut changed = false;
    // サーバーに存在しない IR エントリを削除する。
    let server_ids: std::collections::BTreeSet<&str> =
        rivals.iter().map(|rival| rival.player_id.as_str()).collect();
    let active_will_be_removed = profile.rival.entries.iter().any(|entry| {
        entry.id == profile.rival.active_rival
            && matches!(entry.source, RivalSourceConfig::Ir)
            && entry.ir_service == provider
            && !server_ids.contains(entry.ir_user_id.as_str())
    });
    let before = profile.rival.entries.len();
    profile.rival.entries.retain(|entry| {
        !(matches!(entry.source, RivalSourceConfig::Ir)
            && entry.ir_service == provider
            && !server_ids.contains(entry.ir_user_id.as_str()))
    });
    changed |= profile.rival.entries.len() != before;
    if active_will_be_removed {
        profile.rival.active_rival.clear();
        changed = true;
    }

    for rival in rivals {
        let display_name = rival
            .profile
            .as_ref()
            .map(|profile| profile.display_name.clone())
            .filter(|name| !name.is_empty())
            .unwrap_or_else(|| rival.player_id.clone());
        if let Some(entry) = profile.rival.entries.iter_mut().find(|entry| {
            matches!(entry.source, RivalSourceConfig::Ir)
                && entry.ir_service == provider
                && entry.ir_user_id == rival.player_id
        }) {
            if entry.display_name != display_name {
                entry.display_name = display_name;
                changed = true;
            }
        } else {
            profile.rival.entries.push(RivalEntry {
                id: format!("ir-{provider}-{}", rival.player_id),
                display_name,
                source: RivalSourceConfig::Ir,
                profile_id: String::new(),
                path: String::new(),
                ir_service: provider.to_string(),
                ir_user_id: rival.player_id.clone(),
            });
            changed = true;
        }
    }
    changed
}

pub(super) fn load_active_profile_with_paths(
    app_paths: &AppPaths,
    profile_id: Option<&str>,
) -> Result<(ProfilePaths, ProfileConfig)> {
    let app_config = load_app_config(&app_paths.config_toml)
        .context("failed to load data/config.toml; run the app once to create it")?;
    let profile_id = profile_id.unwrap_or(&app_config.active_profile);
    let profile_paths = resolve_profile_paths(app_paths, profile_id)?;
    let profile = load_profile_config(&profile_paths.profile_toml).with_context(|| {
        format!("failed to load profile config: {}", profile_paths.profile_toml.display())
    })?;
    crate::ir::secret_store::set_store_mode(&profile_paths.root_dir, profile.ir.credential_store);
    Ok((profile_paths, profile))
}

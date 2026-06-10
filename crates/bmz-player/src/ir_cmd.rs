use std::io::Write as _;

use anyhow::{Context, Result, bail};

use crate::cli::{IrCommand, RivalAction};
use crate::config::load::{load_app_config, load_profile_config};
use crate::config::profile_config::{
    IrProviderConfig, IrProviderRoleConfig, IrSendPolicyConfig, ProfileConfig,
};
use crate::config::save::save_profile_config;
use crate::ir::bmz_official::{BmzOfficialIrClient, IrRankingRequest};
use crate::ir::credentials::{
    IrStoredCredentials, delete_credentials, load_credentials, save_credentials,
};
use crate::ir::sync::{ensure_fresh_credentials, sync_pending_ir_jobs};
use crate::ir::types::IrRankingScope;
use crate::paths::{ProfilePaths, resolve_app_paths, resolve_profile_paths};
use crate::storage::score_db::ScoreDatabase;

pub async fn run_ir_command(cmd: IrCommand) -> Result<()> {
    let (profile_paths, mut profile) = load_active_profile()?;
    match cmd {
        IrCommand::Login { email, password, base_url, provider } => {
            login(&profile_paths, &mut profile, &provider, &email, password, base_url).await
        }
        IrCommand::Logout { provider } => logout(&profile_paths, &mut profile, &provider),
        IrCommand::Status => status(&profile_paths, &profile).await,
        IrCommand::Ranking { sha256, gauge, ln_policy, scope, limit } => {
            ranking(&profile_paths, &profile, &sha256, &gauge, &ln_policy, &scope, limit).await
        }
        IrCommand::Sync => sync(&profile_paths, &profile).await,
        IrCommand::Rivals { action } => rivals(&profile_paths, &profile, action).await,
    }
}

async fn rivals(
    profile_paths: &ProfilePaths,
    profile: &ProfileConfig,
    action: Option<RivalAction>,
) -> Result<()> {
    let provider = primary_provider(profile)?;
    let credentials = ensure_fresh_credentials(
        profile_paths.root_dir.as_path(),
        &provider.provider,
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
    if response.rivals.is_empty() {
        println!("no rivals registered");
        return Ok(());
    }
    for rival in &response.rivals {
        let name = rival
            .profile
            .as_ref()
            .map(|profile| profile.display_name.as_str())
            .filter(|name| !name.is_empty())
            .unwrap_or("(no name)");
        println!("- {name} ({})", rival.player_id);
    }
    Ok(())
}

fn load_active_profile() -> Result<(ProfilePaths, ProfileConfig)> {
    let app_paths = resolve_app_paths()?;
    let app_config = load_app_config(&app_paths.config_toml)
        .context("failed to load data/config.toml; run the app once to create it")?;
    let profile_paths = resolve_profile_paths(&app_paths, &app_config.active_profile)?;
    let profile = load_profile_config(&profile_paths.profile_toml).with_context(|| {
        format!("failed to load profile config: {}", profile_paths.profile_toml.display())
    })?;
    Ok((profile_paths, profile))
}

async fn login(
    profile_paths: &ProfilePaths,
    profile: &mut ProfileConfig,
    provider: &str,
    email: &str,
    password: Option<String>,
    base_url: Option<String>,
) -> Result<()> {
    let existing_base_url = profile
        .ir
        .providers
        .iter()
        .find(|entry| entry.provider == provider)
        .map(|entry| entry.base_url.clone())
        .filter(|url| !url.is_empty());
    let Some(base_url) = base_url.or(existing_base_url) else {
        bail!("IR base URL is not configured. Pass --base-url <URL> on first login.");
    };

    let password = match password {
        Some(password) => password,
        None => prompt_password()?,
    };

    let client = BmzOfficialIrClient::anonymous(&base_url)?;
    let tokens = client.login(email, &password).await?;
    let display_name = tokens.player.display_name.clone().unwrap_or_default();
    let now = now_unix_seconds();

    save_credentials(
        profile_paths.root_dir.as_path(),
        &IrStoredCredentials {
            provider: provider.to_string(),
            account_id: tokens.player.id.clone(),
            display_name: display_name.clone(),
            access_token: tokens.access_token,
            refresh_token: tokens.refresh_token,
            expires_at: tokens.expires_at,
        },
    )?;

    let entry = match profile.ir.providers.iter_mut().find(|entry| entry.provider == provider) {
        Some(entry) => entry,
        None => {
            profile.ir.providers.push(IrProviderConfig {
                provider: provider.to_string(),
                base_url: String::new(),
                enabled: false,
                account_display_name: String::new(),
                account_id: String::new(),
                send_policy: IrSendPolicyConfig::default(),
                role: IrProviderRoleConfig::default(),
                last_login_at: None,
                last_success_at: None,
            });
            profile.ir.providers.last_mut().unwrap()
        }
    };
    entry.base_url = base_url;
    entry.enabled = true;
    entry.account_id = tokens.player.id;
    entry.account_display_name = display_name.clone();
    entry.last_login_at = Some(now);
    if profile.ir.primary_provider.is_empty() {
        profile.ir.primary_provider = provider.to_string();
        entry.role = IrProviderRoleConfig::Primary;
    }
    profile.updated_at = now;
    save_profile_config(&profile_paths.profile_toml, profile)?;

    println!(
        "Signed in to {provider} as {}",
        if display_name.is_empty() { email } else { &display_name }
    );
    Ok(())
}

fn logout(profile_paths: &ProfilePaths, profile: &mut ProfileConfig, provider: &str) -> Result<()> {
    let removed = delete_credentials(profile_paths.root_dir.as_path(), provider)?;
    if let Some(entry) = profile.ir.providers.iter_mut().find(|entry| entry.provider == provider) {
        entry.enabled = false;
        profile.updated_at = now_unix_seconds();
        save_profile_config(&profile_paths.profile_toml, profile)?;
    }
    if removed {
        println!("Signed out from {provider}.");
    } else {
        println!("No stored credentials for {provider}.");
    }
    Ok(())
}

async fn status(profile_paths: &ProfilePaths, profile: &ProfileConfig) -> Result<()> {
    if profile.ir.providers.is_empty() {
        println!(
            "No IR providers configured. Run `bmz ir login --email <EMAIL> --base-url <URL>`."
        );
        return Ok(());
    }
    println!("primary provider: {}", profile.ir.primary_provider);
    for entry in &profile.ir.providers {
        println!("- {} (enabled: {}, base_url: {})", entry.provider, entry.enabled, entry.base_url);
        match load_credentials(profile_paths.root_dir.as_path(), &entry.provider)? {
            Some(credentials) => {
                println!("  account: {} ({})", credentials.display_name, credentials.account_id);
                if entry.enabled && !entry.base_url.is_empty() {
                    let now = now_unix_seconds();
                    match ensure_fresh_credentials(
                        profile_paths.root_dir.as_path(),
                        &entry.provider,
                        &entry.base_url,
                        now,
                    )
                    .await
                    {
                        Ok(fresh) => {
                            let client =
                                BmzOfficialIrClient::new(&entry.base_url, fresh.access_token)?;
                            match client.me().await {
                                Ok(me) => println!(
                                    "  connection: OK ({})",
                                    me.player.display_name.unwrap_or(me.player.id)
                                ),
                                Err(error) => println!("  connection: NG ({error:#})"),
                            }
                        }
                        Err(error) => println!("  connection: NG ({error:#})"),
                    }
                }
            }
            None => println!("  account: not signed in"),
        }
    }
    Ok(())
}

async fn ranking(
    profile_paths: &ProfilePaths,
    profile: &ProfileConfig,
    sha256: &str,
    gauge: &str,
    ln_policy: &str,
    scope: &str,
    limit: u32,
) -> Result<()> {
    let provider = primary_provider(profile)?;
    let scope = parse_scope(scope)?;
    let now = now_unix_seconds();
    let mut client = BmzOfficialIrClient::anonymous(&provider.base_url)?;
    if let Ok(credentials) = ensure_fresh_credentials(
        profile_paths.root_dir.as_path(),
        &provider.provider,
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
                gauge: gauge.to_string(),
                ln_policy: ln_policy.to_string(),
                limit,
                offset: 0,
            },
        )
        .await?;

    println!("chart: {}", result.chart.sha256);
    if result.ranking.entries.is_empty() {
        println!("no scores for gauge={gauge} ln_policy={ln_policy}");
        return Ok(());
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
    Ok(())
}

async fn sync(profile_paths: &ProfilePaths, profile: &ProfileConfig) -> Result<()> {
    crate::storage::migration::migrate_score_db(&profile_paths.score_db)?;
    let mut score_db = ScoreDatabase::open(&profile_paths.score_db)?;
    let report = sync_pending_ir_jobs(
        &mut score_db,
        profile_paths.root_dir.as_path(),
        &profile.ir,
        now_unix_seconds(),
        50,
    )
    .await?;
    println!("submitted: {}, failed: {}", report.submitted, report.failed);
    for message in &report.messages {
        println!("  {message}");
    }
    Ok(())
}

fn primary_provider(profile: &ProfileConfig) -> Result<&IrProviderConfig> {
    let provider_name = if profile.ir.primary_provider.is_empty() {
        profile
            .ir
            .providers
            .iter()
            .find(|entry| entry.enabled)
            .map(|entry| entry.provider.clone())
            .unwrap_or_default()
    } else {
        profile.ir.primary_provider.clone()
    };
    profile
        .ir
        .providers
        .iter()
        .find(|entry| entry.provider == provider_name && !entry.base_url.is_empty())
        .context("no IR provider configured; run `bmz ir login` first")
}

fn parse_scope(value: &str) -> Result<IrRankingScope> {
    Ok(match value {
        "global" => IrRankingScope::Global,
        "self_and_rivals" => IrRankingScope::SelfAndRivals,
        "rivals" => IrRankingScope::Rivals,
        "self" => IrRankingScope::SelfOnly,
        "around_self" => IrRankingScope::AroundSelf,
        other => bail!("unknown ranking scope: {other}"),
    })
}

fn prompt_password() -> Result<String> {
    print!("password: ");
    std::io::stdout().flush()?;
    let mut line = String::new();
    std::io::stdin().read_line(&mut line)?;
    let password = line.trim_end_matches(['\r', '\n']).to_string();
    if password.is_empty() {
        bail!("password must not be empty");
    }
    Ok(password)
}

fn now_unix_seconds() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

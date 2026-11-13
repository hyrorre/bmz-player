use super::download::{now_unix_seconds, prompt_password};
use super::*;
use crate::ir::credentials::{credential_origin, validate_login_origin};

fn prompt_api_key() -> Result<String> {
    let key = rpassword::prompt_password("Bokutachi API key (submit_score): ")?;
    anyhow::ensure!(!key.trim().is_empty(), "API key must not be empty");
    Ok(key)
}

pub(super) async fn login(
    profile_paths: &ProfilePaths,
    profile: &mut ProfileConfig,
    provider: &str,
    email: &str,
    password: Option<String>,
    base_url: Option<String>,
) -> Result<()> {
    let is_bms_ir = crate::ir::bms_ir::is_bms_ir_provider(provider);
    let is_bokutachi = crate::ir::bokutachi::is_provider(provider);
    let is_rian_ir = crate::ir::rian_ir::is_rian_ir_provider(provider);
    let base_url = resolve_login_base_url(profile, provider, base_url.as_deref())?;
    let origin = credential_origin(&base_url)?;

    let password = match password {
        Some(password) => password,
        None if is_bokutachi => prompt_api_key()?,
        None => prompt_password()?,
    };

    let tokens = if is_bokutachi {
        crate::ir::bokutachi::BokutachiClient::new(&base_url)?.validate_key(&password).await?
    } else if is_bms_ir {
        crate::ir::bms_ir::BmsIrClient::new(&base_url)?.login(email, &password).await?
    } else if is_rian_ir {
        crate::ir::rian_ir::RianIrClient::new(&base_url)?.login(email, &password).await?
    } else {
        BmzOfficialIrClient::anonymous(&base_url)?.login(email, &password).await?
    };
    let provider_key = tokens.provider_key.clone();
    let account_id = tokens.player.id.clone();
    let display_name = tokens.player.display_name.clone().unwrap_or_default();
    let bms_ir_game_token = tokens.access_token.clone();
    let now = now_unix_seconds();

    let entry_index = profile
        .ir
        .providers
        .iter()
        .position(|entry| {
            same_provider_protocol(&entry.provider, provider)
                && (is_bms_ir || entry.base_url == base_url)
        })
        .or_else(|| {
            profile.ir.providers.iter().position(|entry| {
                same_provider_protocol(&entry.provider, provider) && entry.base_url.is_empty()
            })
        });
    validate_login_origin(&profile.ir, entry_index, &provider_key, &origin)?;
    save_credentials(
        profile_paths.root_dir.as_path(),
        &IrStoredCredentials {
            provider: provider_key.clone(),
            origin,
            account_id: tokens.player.id,
            display_name: display_name.clone(),
            access_token: tokens.access_token,
            refresh_token: tokens.refresh_token,
            expires_at: tokens.expires_at,
        },
    )?;
    let entry = match entry_index {
        Some(index) => &mut profile.ir.providers[index],
        None => {
            profile.ir.providers.push(IrProviderConfig {
                provider: canonical_provider_protocol(provider).to_string(),
                provider_key: String::new(),
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
    let was_primary = entry.role == IrProviderRoleConfig::Primary
        && profile.ir.primary_provider == entry.provider_key;
    entry.base_url = base_url;
    entry.provider_key = provider_key.clone();
    entry.enabled = true;
    entry.account_id = account_id.clone();
    entry.account_display_name = display_name.clone();
    entry.last_login_at = Some(now);
    if !is_bokutachi && (profile.ir.primary_provider.is_empty() || was_primary) {
        profile.ir.primary_provider = provider_key.clone();
        entry.role = IrProviderRoleConfig::Primary;
    }
    if is_rian_ir {
        match crate::ir::rian_ir::RianIrClient::new(&entry.base_url)?
            .fetch_rivals(&account_id)
            .await
        {
            Ok(rivals) => {
                sync_ir_rivals_into_profile(profile, &provider_key, &rivals);
                tracing::info!(rivals = rivals.len(), "rianIR rivals synced after login");
            }
            Err(error) => {
                tracing::warn!(%error, "rianIR rival sync after login failed; login remains valid");
            }
        }
    } else if is_bms_ir {
        match crate::ir::bms_ir::BmsIrClient::new(&entry.base_url)?
            .get_rivals(&account_id, &bms_ir_game_token)
            .await
        {
            Ok(response) => {
                sync_ir_rivals_into_profile(profile, &provider_key, &response.rivals);
                tracing::info!(rivals = response.rivals.len(), "BMS-IR rivals synced after login");
            }
            Err(error) => {
                tracing::warn!(%error, "BMS-IR rival sync after login failed; login remains valid");
            }
        }
    }
    profile.updated_at = now;
    save_profile_config(&profile_paths.profile_toml, profile)?;

    println!(
        "Signed in to {provider} as {}",
        if display_name.is_empty() { email } else { &display_name }
    );
    Ok(())
}

fn resolve_login_base_url(
    profile: &ProfileConfig,
    provider: &str,
    requested_base_url: Option<&str>,
) -> Result<String> {
    if crate::ir::bms_ir::is_bms_ir_provider(provider) {
        return crate::ir::bms_ir::fixed_base_url(requested_base_url);
    }
    if crate::ir::bokutachi::is_provider(provider) {
        return Ok(requested_base_url
            .unwrap_or(crate::ir::bokutachi::DEFAULT_BASE_URL)
            .to_string());
    }
    let existing_base_url = profile
        .ir
        .providers
        .iter()
        .find(|entry| {
            same_provider_protocol(&entry.provider, provider)
                && requested_base_url.is_none_or(|url| entry.base_url == url)
        })
        .map(|entry| entry.base_url.clone())
        .filter(|url| !url.is_empty());
    requested_base_url
        .map(str::to_string)
        .or(existing_base_url)
        .or_else(|| {
            crate::ir::rian_ir::is_rian_ir_provider(provider)
                .then(|| crate::ir::rian_ir::RIAN_IR_DEFAULT_BASE_URL.to_string())
        })
        .context("IR base URL is not configured. Pass --base-url <URL> on first login.")
}

pub(super) async fn logout(
    profile_paths: &ProfilePaths,
    profile: &mut ProfileConfig,
    provider: &str,
) -> Result<()> {
    let entry_index = profile
        .ir
        .providers
        .iter()
        .position(|entry| crate::ir::provider_key::configured_provider_key(entry) == Some(provider))
        .or_else(|| {
            profile
                .ir
                .providers
                .iter()
                .position(|entry| same_provider_protocol(&entry.provider, provider))
        });
    let entry = entry_index.and_then(|index| profile.ir.providers.get(index));
    let credentials = match entry {
        Some(entry) => crate::ir::provider_key::configured_provider_key(entry)
            .map(|provider_key| {
                load_credentials(profile_paths.root_dir.as_path(), provider_key, &entry.base_url)
            })
            .transpose()?
            .flatten(),
        None => None,
    };
    if let Some(credentials) = &credentials
        && let Some(entry) = entry
        && !crate::ir::rian_ir::is_rian_ir_config(entry)
        && !crate::ir::bms_ir::is_bms_ir_config(entry)
        && !crate::ir::bokutachi::is_config(entry)
    {
        let client = BmzOfficialIrClient::new(&entry.base_url, credentials.access_token.clone())?;
        if let Err(error) = client.logout(&credentials.refresh_token).await {
            eprintln!("warning: failed to revoke remote IR session for {provider}: {error:#}");
        }
    }

    let removed = match entry {
        Some(entry) => crate::ir::provider_key::configured_provider_key(entry)
            .map(|provider_key| {
                delete_credentials(profile_paths.root_dir.as_path(), provider_key, &entry.base_url)
            })
            .transpose()?
            .unwrap_or(false),
        None => false,
    };
    if let Some(index) = entry_index
        && let Some(entry) = profile.ir.providers.get_mut(index)
    {
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

pub(super) fn canonical_provider_protocol(provider: &str) -> &'static str {
    if crate::ir::bokutachi::is_provider(provider) {
        crate::ir::bokutachi::PROVIDER
    } else if crate::ir::bms_ir::is_bms_ir_provider(provider) {
        crate::ir::bms_ir::BMS_IR_PROVIDER
    } else if crate::ir::rian_ir::is_rian_ir_provider(provider) {
        crate::ir::rian_ir::RIAN_IR_PROVIDER
    } else {
        crate::ir::bmz_official::BMZ_IR_PROVIDER
    }
}

pub(super) fn same_provider_protocol(left: &str, right: &str) -> bool {
    canonical_provider_protocol(left) == canonical_provider_protocol(right)
}

pub(super) async fn status(profile_paths: &ProfilePaths, profile: &ProfileConfig) -> Result<()> {
    if profile.ir.providers.is_empty() {
        println!(
            "No IR providers configured. Run `bmz ir login --email <EMAIL> --base-url <URL>`."
        );
        return Ok(());
    }
    println!("primary provider: {}", profile.ir.primary_provider);
    for entry in &profile.ir.providers {
        let provider_key = crate::ir::provider_key::configured_provider_key(entry);
        println!(
            "- {} (key: {}, enabled: {}, base_url: {})",
            entry.provider,
            provider_key.unwrap_or("(not signed in)"),
            entry.enabled,
            entry.base_url
        );
        match provider_key
            .map(|provider_key| {
                load_credentials(profile_paths.root_dir.as_path(), provider_key, &entry.base_url)
            })
            .transpose()?
            .flatten()
        {
            Some(credentials) => {
                println!("  account: {} ({})", credentials.display_name, credentials.account_id);
                if entry.enabled && !entry.base_url.is_empty() {
                    let now = now_unix_seconds();
                    let fresh = if crate::ir::bokutachi::is_config(entry) {
                        Ok(credentials)
                    } else {
                        ensure_fresh_credentials(
                            profile_paths.root_dir.as_path(),
                            provider_key.unwrap_or(""),
                            &entry.base_url,
                            now,
                        )
                        .await
                    };
                    match fresh {
                        Ok(fresh) => {
                            if crate::ir::bokutachi::is_config(entry) {
                                match crate::ir::bokutachi::BokutachiClient::new(&entry.base_url)?
                                    .validate_key(&fresh.access_token)
                                    .await
                                {
                                    Ok(tokens) if tokens.player.id == fresh.account_id => println!(
                                        "  connection: OK ({}; submission only)",
                                        fresh.account_id
                                    ),
                                    Ok(_) => println!(
                                        "  connection: NG (account changed; validate the API key again)"
                                    ),
                                    Err(error) => println!("  connection: NG ({error:#})"),
                                }
                            } else if crate::ir::bms_ir::is_bms_ir_config(entry) {
                                match crate::ir::bms_ir::BmsIrClient::new(&entry.base_url)?
                                    .login(&fresh.account_id, &fresh.access_token)
                                    .await
                                {
                                    Ok(_) => println!("  connection: OK ({})", fresh.account_id),
                                    Err(error) => println!("  connection: NG ({error:#})"),
                                }
                            } else if crate::ir::rian_ir::is_rian_ir_config(entry) {
                                println!(
                                    "  connection: credentials stored ({})",
                                    fresh.display_name
                                );
                            } else {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn profile() -> ProfileConfig {
        ProfileConfig::new_default("test", "Test", 0)
    }

    #[test]
    fn bms_ir_login_rejects_a_different_credential_origin() {
        let error = resolve_login_base_url(
            &profile(),
            crate::ir::bms_ir::BMS_IR_PROVIDER,
            Some("https://attacker.example/collect"),
        )
        .unwrap_err();
        assert!(error.to_string().contains("does not allow a custom base URL"));
    }

    #[test]
    fn bms_ir_login_uses_the_compiled_origin_and_ignores_legacy_profile_urls() {
        let mut profile = profile();
        let mut entry = IrProviderConfig::bms_ir();
        entry.base_url = "https://attacker.example/legacy".to_string();
        profile.ir.providers.push(entry);

        assert_eq!(
            resolve_login_base_url(&profile, crate::ir::bms_ir::BMS_IR_PROVIDER, None).unwrap(),
            crate::ir::bms_ir::BMS_IR_DEFAULT_BASE_URL
        );
    }

    #[test]
    fn custom_provider_login_keeps_an_explicit_base_url() {
        let requested = "https://self-hosted.example/ir";
        assert_eq!(
            resolve_login_base_url(&profile(), "custom-provider", Some(requested)).unwrap(),
            requested
        );
    }

    #[tokio::test]
    async fn custom_login_cannot_store_a_builtin_provider_key() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base_url = format!("http://{}", listener.local_addr().unwrap());
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut request = [0; 4096];
            assert!(stream.read(&mut request).await.unwrap() > 0);
            let body = r#"{"provider_key":"bmz-official","access_token":"a","refresh_token":"r","player":{"id":"test"}}"#;
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            stream.write_all(response.as_bytes()).await.unwrap();
        });
        let data = crate::bootstrap::profile_tests::ProfileTestDir::new();
        let mut boot = data.boot();
        let origin = credential_origin(crate::ir::bmz_official::BMZ_IR_DEFAULT_BASE_URL).unwrap();
        let original = IrStoredCredentials {
            provider: "bmz-official".into(),
            origin: origin.clone(),
            account_id: "official".into(),
            display_name: "Official".into(),
            access_token: "original".into(),
            refresh_token: "original-refresh".into(),
            expires_at: None,
        };
        save_credentials(&boot.profile_paths.root_dir, &original).unwrap();
        let error = login(
            &boot.profile_paths,
            &mut boot.profile_config,
            "custom",
            "test@example.test",
            Some("dummy".into()),
            Some(base_url),
        )
        .await
        .unwrap_err();
        assert!(error.to_string().contains("reserved for the built-in origin"), "{error:#}");
        server.await.unwrap();
        assert_eq!(
            load_credentials(&boot.profile_paths.root_dir, "bmz-official", &origin).unwrap(),
            Some(original)
        );
    }
}

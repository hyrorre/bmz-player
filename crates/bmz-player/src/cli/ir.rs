use super::*;

pub(super) fn parse_ir_command(rest: &[String]) -> Result<Command> {
    match rest.first().map(|s| s.as_str()) {
        Some("login") => {
            let mut email = None;
            let mut password = None;
            let mut base_url = None;
            let mut provider = "bmz-official".to_string();
            let mut iter = rest[1..].iter();
            while let Some(flag) = iter.next() {
                match flag.as_str() {
                    "--id" | "--email" => email = iter.next().cloned(),
                    "--password" => password = iter.next().cloned(),
                    "--base-url" => base_url = iter.next().cloned(),
                    "--provider" => {
                        provider = iter
                            .next()
                            .cloned()
                            .ok_or_else(|| anyhow::anyhow!("--provider requires a value"))?;
                    }
                    other => bail!("unknown flag for ir login: {other}"),
                }
            }
            let email = if crate::ir::bokutachi::is_provider(&provider) {
                anyhow::ensure!(
                    email.is_none() && password.is_none(),
                    "Bokutachi uses an API key prompted on stdin; omit --id, --email and --password"
                );
                String::new()
            } else {
                email.ok_or_else(|| anyhow::anyhow!("ir login requires --id <ID>"))?
            };
            Ok(Command::Ir(IrCommand::Login { email, password, base_url, provider }))
        }
        Some("logout") => {
            let provider = match rest.get(1).map(|s| s.as_str()) {
                Some("--provider") => rest
                    .get(2)
                    .cloned()
                    .ok_or_else(|| anyhow::anyhow!("--provider requires a value"))?,
                Some(other) => bail!("unknown flag for ir logout: {other}"),
                None => "bmz-official".to_string(),
            };
            Ok(Command::Ir(IrCommand::Logout { provider }))
        }
        Some("status") => Ok(Command::Ir(IrCommand::Status)),
        Some("ranking") => {
            let sha256 = rest
                .get(1)
                .filter(|s| !s.starts_with('-'))
                .ok_or_else(|| anyhow::anyhow!("ir ranking requires a chart SHA256"))?
                .clone();
            let mut ln_policy = "ForceLn".to_string();
            let mut scope = "global".to_string();
            let mut limit = 20u32;
            let mut iter = rest[2..].iter();
            while let Some(flag) = iter.next() {
                let value = |iter: &mut std::slice::Iter<'_, String>| {
                    iter.next().cloned().ok_or_else(|| anyhow::anyhow!("{flag} requires a value"))
                };
                match flag.as_str() {
                    "--ln-policy" => ln_policy = value(&mut iter)?,
                    "--scope" => scope = value(&mut iter)?,
                    "--limit" => {
                        limit = value(&mut iter)?
                            .parse()
                            .map_err(|_| anyhow::anyhow!("--limit must be an integer"))?;
                    }
                    other => bail!("unknown flag for ir ranking: {other}"),
                }
            }
            Ok(Command::Ir(IrCommand::Ranking { sha256, ln_policy, scope, limit }))
        }
        Some("sync") => Ok(Command::Ir(IrCommand::Sync)),
        Some("upload-local") => Ok(Command::Ir(parse_ir_upload_local_flags(&rest[1..])?)),
        Some("download-scores") => Ok(Command::Ir(parse_ir_download_scores_flags(&rest[1..])?)),
        Some("attest-submitted") => Ok(Command::Ir(parse_ir_attest_submitted_flags(&rest[1..])?)),
        Some("cleanup-imported") => Ok(Command::Ir(parse_ir_cleanup_imported_flags(&rest[1..])?)),
        Some("cleanup-duplicate") => {
            Ok(Command::Ir(parse_ir_cleanup_duplicate_command(&rest[1..])?))
        }
        Some("rivals") => {
            let action = match rest.get(1).map(|s| s.as_str()) {
                Some("add") => Some(RivalAction::Add {
                    player_id: rest
                        .get(2)
                        .cloned()
                        .ok_or_else(|| anyhow::anyhow!("ir rivals add requires a PLAYER_ID"))?,
                }),
                Some("remove") => Some(RivalAction::Remove {
                    player_id: rest
                        .get(2)
                        .cloned()
                        .ok_or_else(|| anyhow::anyhow!("ir rivals remove requires a PLAYER_ID"))?,
                }),
                Some(other) => bail!("unknown ir rivals subcommand: {other}. Use: add, remove"),
                None => None,
            };
            Ok(Command::Ir(IrCommand::Rivals { action }))
        }
        Some("replay") => {
            let score_id = rest
                .get(1)
                .filter(|value| !value.starts_with('-'))
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("ir replay requires a SCORE_ID"))?;
            Ok(Command::Ir(IrCommand::Replay { score_id }))
        }
        Some("device-key") => {
            let rotate = match rest.get(1).map(|s| s.as_str()) {
                Some("rotate") => true,
                Some(other) => bail!("unknown ir device-key subcommand: {other}. Use: rotate"),
                None => false,
            };
            Ok(Command::Ir(IrCommand::DeviceKey { rotate }))
        }
        Some(sub) => {
            bail!(
                "unknown ir subcommand: {sub}. Use: login, logout, status, ranking, sync, upload-local, download-scores, attest-submitted, cleanup-imported, cleanup-duplicate, rivals, device-key, replay"
            )
        }
        None => {
            bail!(
                "ir requires a subcommand: login, logout, status, ranking, sync, upload-local, download-scores, attest-submitted, cleanup-imported, cleanup-duplicate, rivals, device-key"
            )
        }
    }
}

fn parse_ir_upload_local_flags(flags: &[String]) -> Result<IrCommand> {
    let mut provider = None;
    let mut limit = crate::ir::backfill::DEFAULT_UPLOAD_LOCAL_LIMIT;
    let mut dry_run = false;
    let mut sync = false;
    let mut all = false;
    let mut no_sync = false;
    let mut resend = false;
    let mut include_course_stages = false;
    let mut include_replay = false;
    let mut iter = flags.iter();

    while let Some(flag) = iter.next() {
        match flag.as_str() {
            "--dry-run" => dry_run = true,
            "--sync" => sync = true,
            "--no-sync" => {
                sync = false;
                no_sync = true;
            }
            "--all" => all = true,
            "--resend" => resend = true,
            "--include-course-stages" => include_course_stages = true,
            "--include-replay" => include_replay = true,
            "--provider" => {
                provider = Some(
                    iter.next()
                        .cloned()
                        .ok_or_else(|| anyhow::anyhow!("--provider requires a value"))?,
                );
            }
            "--limit" => {
                let value = iter
                    .next()
                    .ok_or_else(|| anyhow::anyhow!("--limit requires a positive integer"))?;
                limit = parse_ir_upload_limit(value)?;
            }
            _ if flag.starts_with("--provider=") => {
                provider = Some(flag["--provider=".len()..].to_string());
            }
            _ if flag.starts_with("--limit=") => {
                limit = parse_ir_upload_limit(&flag["--limit=".len()..])?;
            }
            other => bail!("unknown flag for ir upload-local: {other}"),
        }
    }

    if all && dry_run {
        bail!("ir upload-local --all cannot be combined with --dry-run");
    }
    if all && no_sync {
        bail!("ir upload-local --all cannot be combined with --no-sync");
    }
    if all {
        sync = true;
    }

    Ok(IrCommand::UploadLocal {
        provider,
        limit,
        dry_run,
        sync,
        all,
        resend,
        include_course_stages,
        include_replay,
    })
}

fn parse_ir_attest_submitted_flags(flags: &[String]) -> Result<IrCommand> {
    let mut provider = None;
    let mut sync = true;
    let mut all = false;
    let mut iter = flags.iter();

    while let Some(flag) = iter.next() {
        match flag.as_str() {
            "--sync" => sync = true,
            "--no-sync" => sync = false,
            "--all" => all = true,
            "--provider" => {
                provider = Some(
                    iter.next()
                        .cloned()
                        .ok_or_else(|| anyhow::anyhow!("--provider requires a value"))?,
                );
            }
            _ if flag.starts_with("--provider=") => {
                provider = Some(flag["--provider=".len()..].to_string());
            }
            other => bail!("unknown flag for ir attest-submitted: {other}"),
        }
    }

    if all && !sync {
        bail!("ir attest-submitted --all cannot be combined with --no-sync");
    }

    Ok(IrCommand::AttestSubmitted { provider, sync, all })
}

fn parse_ir_cleanup_imported_flags(flags: &[String]) -> Result<IrCommand> {
    let mut provider = None;
    let mut apply = false;
    let mut iter = flags.iter();

    while let Some(flag) = iter.next() {
        match flag.as_str() {
            "--apply" => apply = true,
            "--provider" => {
                provider = Some(
                    iter.next()
                        .cloned()
                        .ok_or_else(|| anyhow::anyhow!("--provider requires a value"))?,
                );
            }
            _ if flag.starts_with("--provider=") => {
                provider = Some(flag["--provider=".len()..].to_string());
            }
            other => bail!("unknown flag for ir cleanup-imported: {other}"),
        }
    }

    Ok(IrCommand::CleanupImported { provider, apply })
}

fn parse_ir_cleanup_duplicate_command(args: &[String]) -> Result<IrCommand> {
    let history_id = args
        .first()
        .filter(|value| !value.starts_with('-'))
        .ok_or_else(|| anyhow::anyhow!("ir cleanup-duplicate requires a HISTORY_ID"))?;
    let history_id: i64 = history_id
        .parse()
        .with_context(|| format!("invalid HISTORY_ID for ir cleanup-duplicate: {history_id}"))?;
    if history_id <= 0 {
        bail!("ir cleanup-duplicate HISTORY_ID must be positive (got {history_id})");
    }

    let mut provider = None;
    let mut apply = false;
    let mut iter = args[1..].iter();
    while let Some(flag) = iter.next() {
        match flag.as_str() {
            "--apply" => apply = true,
            "--provider" => {
                provider = Some(
                    iter.next()
                        .cloned()
                        .ok_or_else(|| anyhow::anyhow!("--provider requires a value"))?,
                );
            }
            _ if flag.starts_with("--provider=") => {
                provider = Some(flag["--provider=".len()..].to_string());
            }
            other => bail!("unknown flag for ir cleanup-duplicate: {other}"),
        }
    }

    if !apply {
        bail!("ir cleanup-duplicate requires --apply");
    }

    Ok(IrCommand::CleanupDuplicate { history_id, provider, apply })
}

fn parse_ir_upload_limit(value: &str) -> Result<u32> {
    let value = value.trim();
    if value.is_empty() {
        bail!("--limit requires a positive integer");
    }
    let limit: u32 = value.parse().with_context(|| format!("invalid --limit value: {value}"))?;
    if limit == 0 {
        bail!("--limit must be positive");
    }
    Ok(limit)
}

fn parse_ir_download_scores_flags(flags: &[String]) -> Result<IrCommand> {
    let mut provider = None;
    let mut limit = crate::ir::download::DEFAULT_DOWNLOAD_SCORES_LIMIT;
    let mut dry_run = false;
    let mut iter = flags.iter();

    while let Some(flag) = iter.next() {
        match flag.as_str() {
            "--dry-run" => dry_run = true,
            "--provider" => {
                provider = Some(
                    iter.next()
                        .cloned()
                        .ok_or_else(|| anyhow::anyhow!("--provider requires a value"))?,
                );
            }
            "--limit" => {
                let value = iter
                    .next()
                    .ok_or_else(|| anyhow::anyhow!("--limit requires a positive integer"))?;
                limit = parse_ir_upload_limit(value)?;
            }
            _ if flag.starts_with("--provider=") => {
                provider = Some(flag["--provider=".len()..].to_string());
            }
            _ if flag.starts_with("--limit=") => {
                limit = parse_ir_upload_limit(&flag["--limit=".len()..])?;
            }
            other => bail!("unknown flag for ir download-scores: {other}"),
        }
    }

    Ok(IrCommand::DownloadScores { provider, limit, dry_run })
}

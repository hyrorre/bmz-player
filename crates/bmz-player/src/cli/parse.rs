pub fn parse_command<I, S>(args: I) -> Result<Command>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    Ok(parse_cli_command(args)?.command)
}

pub fn parse_cli_command<I, S>(args: I) -> Result<ParsedCommand>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let args: Vec<String> = args.into_iter().map(|s| s.as_ref().to_string()).collect();
    let (args, profile_id, mut warnings) = extract_global_profile(args)?;
    let command = match args.first().map(|s| s.as_str()) {
        Some("export") => super::export::parse(&args[1..]).map(Command::Export),
        Some("table") => {
            let rest = &args[1..];
            match rest.first().map(|s| s.as_str()) {
                Some("add") => {
                    let url = rest
                        .get(1)
                        .ok_or_else(|| anyhow::anyhow!("table add requires a URL"))?
                        .clone();
                    Ok(Command::Table(TableCommand::Add { url }))
                }
                Some("list") => Ok(Command::Table(TableCommand::List)),
                Some("fetch") => {
                    let url = rest.get(1).cloned();
                    Ok(Command::Table(TableCommand::Fetch { url }))
                }
                Some(sub) => bail!("unknown table subcommand: {sub}. Use: add, list, fetch"),
                None => bail!("table requires a subcommand: add, list, fetch"),
            }
        }
        Some("songs") => {
            let rest = &args[1..];
            match rest.first().map(|s| s.as_str()) {
                Some("add") => {
                    let flags = &rest[1..];
                    let path = flags
                        .iter()
                        .find(|s| !s.starts_with('-'))
                        .ok_or_else(|| anyhow::anyhow!("songs add requires a PATH"))?
                        .clone();
                    let recursive = !flags.iter().any(|s| s == "--no-recursive");
                    let enabled = !flags.iter().any(|s| s == "--disabled");
                    Ok(Command::Songs(SongsCommand::Add { path, recursive, enabled }))
                }
                Some("list") => Ok(Command::Songs(SongsCommand::List)),
                Some("load") => {
                    let (target, use_everything) = parse_songs_scan_args(&rest[1..])?;
                    Ok(Command::Songs(SongsCommand::Load { target, use_everything }))
                }
                Some("reload") => {
                    let (target, use_everything) = parse_songs_scan_args(&rest[1..])?;
                    Ok(Command::Songs(SongsCommand::Reload { target, use_everything }))
                }
                Some(sub) => bail!("unknown songs subcommand: {sub}. Use: add, list, load, reload"),
                None => bail!("songs requires a subcommand: add, list, load, reload"),
            }
        }
        Some("course") => {
            let rest = &args[1..];
            match rest.first().map(|s| s.as_str()) {
                Some("import") => {
                    let path = rest
                        .get(1)
                        .ok_or_else(|| anyhow::anyhow!("course import requires a PATH"))?
                        .clone();
                    Ok(Command::Course(CourseCommand::Import { path }))
                }
                Some("list") => Ok(Command::Course(CourseCommand::List)),
                Some("history") => {
                    let id_str = rest
                        .get(1)
                        .ok_or_else(|| anyhow::anyhow!("course history requires a COURSE_ID"))?;
                    let course_id = parse_course_history_id(id_str)?;
                    // Optional `--limit N` flag; default 10.
                    let limit = parse_course_history_limit(&rest[2..])?;
                    Ok(Command::Course(CourseCommand::History { course_id, limit }))
                }
                Some("attempt") => {
                    let id_str = rest
                        .get(1)
                        .ok_or_else(|| anyhow::anyhow!("course attempt requires a SCORE_ID"))?;
                    let score_id = parse_course_attempt_id(id_str)?;
                    if rest.len() > 2 {
                        bail!("unknown flag for course attempt: {}", rest[2]);
                    }
                    Ok(Command::Course(CourseCommand::Attempt { score_id }))
                }
                Some(sub) => {
                    bail!("unknown course subcommand: {sub}. Use: import, list, history, attempt")
                }
                None => bail!("course requires a subcommand: import, list, history, attempt"),
            }
        }
        Some("replay") => {
            let rest = &args[1..];
            match rest.first().map(|s| s.as_str()) {
                Some("import") => {
                    let flags = &rest[1..];
                    let paths =
                        flags.iter().filter(|arg| !arg.starts_with('-')).collect::<Vec<_>>();
                    let path = paths
                        .first()
                        .ok_or_else(|| anyhow::anyhow!("replay import requires a PATH"))?
                        .to_string();
                    if paths.len() > 1 {
                        bail!("replay import accepts only one PATH, got: {}", paths[1]);
                    }
                    for flag in flags.iter().filter(|arg| arg.starts_with('-')) {
                        if !matches!(flag.as_str(), "--overwrite" | "--controller") {
                            bail!("unknown replay import flag: {flag}");
                        }
                    }
                    Ok(Command::Replay(ReplayCommand::Import {
                        path,
                        overwrite: flags.iter().any(|arg| arg == "--overwrite"),
                        controller: flags.iter().any(|arg| arg == "--controller"),
                    }))
                }
                Some(sub) => bail!("unknown replay subcommand: {sub}. Use: import"),
                None => bail!("replay requires a subcommand: import"),
            }
        }
        Some("profile") => parse_profile_command(&args[1..]),
        Some("ir") => parse_ir_command(&args[1..]),
        _ => {
            let (options, option_warnings) = AppOptions::parse_args_with_warnings(args)?;
            warnings.extend(option_warnings);
            Ok(Command::Run(options))
        }
    }?;
    if profile_id.is_some() && !command_uses_profile(&command) {
        warnings.push(format!(
            "{VIEWER_PROFILE_ARG} does not affect this command; ignoring the profile override"
        ));
    }
    Ok(ParsedCommand { command, profile_id, warnings })
}

fn extract_global_profile(args: Vec<String>) -> Result<(Vec<String>, Option<String>, Vec<String>)> {
    let mut remaining = Vec::with_capacity(args.len());
    let mut profile_id = None;
    let mut warnings = Vec::new();
    let mut index = 0;
    while index < args.len() {
        let arg = &args[index];
        let value = if arg == VIEWER_PROFILE_ARG {
            index += 1;
            Some(
                args.get(index)
                    .ok_or_else(|| anyhow::anyhow!("{VIEWER_PROFILE_ARG} requires a profile id"))?
                    .as_str(),
            )
        } else {
            arg.strip_prefix("--profile=")
        };
        if let Some(value) = value {
            crate::paths::validate_profile_id(value)?;
            if profile_id.is_some() {
                warnings.push(format!(
                    "multiple {VIEWER_PROFILE_ARG} options were specified; using the last profile ({value})"
                ));
            }
            profile_id = Some(value.to_string());
        } else {
            remaining.push(arg.clone());
        }
        index += 1;
    }
    Ok((remaining, profile_id, warnings))
}

fn command_uses_profile(command: &Command) -> bool {
    match command {
        Command::Run(options) => !options.viewer_stop,
        Command::Course(CourseCommand::History { .. } | CourseCommand::Attempt { .. })
        | Command::Replay(_)
        | Command::Ir(_) => true,
        Command::Table(_)
        | Command::Songs(_)
        | Command::Course(CourseCommand::Import { .. } | CourseCommand::List)
        | Command::Profile(_)
        | Command::Export(_) => false,
    }
}

fn parse_songs_scan_args(args: &[String]) -> Result<(Option<String>, Option<bool>)> {
    let mut target = None;
    let mut use_everything = None;
    for arg in args {
        match arg.as_str() {
            "--everything" => {
                if use_everything == Some(false) {
                    bail!("--everything conflicts with --no-everything");
                }
                use_everything = Some(true);
            }
            "--no-everything" => {
                if use_everything == Some(true) {
                    bail!("--no-everything conflicts with --everything");
                }
                use_everything = Some(false);
            }
            flag if flag.starts_with('-') => bail!("unknown songs scan flag: {flag}"),
            value if target.is_none() => target = Some(value.to_string()),
            value => bail!("songs scan accepts only one PATH or NAME, got: {value}"),
        }
    }
    Ok((target, use_everything))
}

fn parse_profile_command(rest: &[String]) -> Result<Command> {
    match rest.first().map(|s| s.as_str()) {
        Some("list") => Ok(Command::Profile(ProfileCommand::List)),
        Some("current") => Ok(Command::Profile(ProfileCommand::Current)),
        Some("use") => {
            let id = rest
                .get(1)
                .filter(|value| !value.starts_with('-'))
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("profile use requires a PROFILE_ID"))?;
            if rest.len() > 2 {
                bail!("unknown flag for profile use: {}", rest[2]);
            }
            Ok(Command::Profile(ProfileCommand::Use { id }))
        }
        Some("create") => {
            let id = rest
                .get(1)
                .filter(|value| !value.starts_with('-'))
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("profile create requires a PROFILE_ID"))?;
            let (display_name, activate) = parse_profile_name_and_activate_flags(&rest[2..])?;
            Ok(Command::Profile(ProfileCommand::Create { id, display_name, activate }))
        }
        Some("copy") => {
            let source_id = rest
                .get(1)
                .filter(|value| !value.starts_with('-'))
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("profile copy requires a SOURCE_PROFILE_ID"))?;
            let target_id = rest
                .get(2)
                .filter(|value| !value.starts_with('-'))
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("profile copy requires a TARGET_PROFILE_ID"))?;
            let (display_name, activate) = parse_profile_name_and_activate_flags(&rest[3..])?;
            Ok(Command::Profile(ProfileCommand::Copy {
                source_id,
                target_id,
                display_name,
                activate,
            }))
        }
        Some(sub) => {
            bail!("unknown profile subcommand: {sub}. Use: list, current, use, create, copy")
        }
        None => bail!("profile requires a subcommand: list, current, use, create, copy"),
    }
}

fn parse_profile_name_and_activate_flags(flags: &[String]) -> Result<(Option<String>, bool)> {
    let mut display_name = None;
    let mut activate = false;
    let mut iter = flags.iter();
    while let Some(flag) = iter.next() {
        match flag.as_str() {
            "--display-name" | "--name" => {
                let value = iter
                    .next()
                    .cloned()
                    .ok_or_else(|| anyhow::anyhow!("{flag} requires a value"))?;
                if value.trim().is_empty() {
                    bail!("{flag} requires a non-empty value");
                }
                display_name = Some(value);
            }
            "--activate" => activate = true,
            other => bail!("unknown profile flag: {other}"),
        }
    }
    Ok((display_name, activate))
}
use super::help::{parse_course_attempt_id, parse_course_history_id, parse_course_history_limit};
use super::ir::parse_ir_command;
use super::*;

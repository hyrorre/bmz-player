use crate::config::app_config::RendererBackend;
use anyhow::{Context, Result, bail};

pub const BOOT_PLAY_SAMPLE_ARG: &str = "--boot-play-sample";
pub const BOOT_RESULT_SAMPLE_ARG: &str = "--boot-result-sample";
pub const AUTOPLAY_ON_START_ARG: &str = "--autoplay-on-start";
pub const AUTOPLAY_SHORT_ARG: &str = "-a";
pub const SMOKE_EXIT_AFTER_FRAMES_ARG: &str = "--smoke-exit-after-frames";
pub const SMOKE_EXIT_AFTER_RESULT_FRAMES_ARG: &str = "--smoke-exit-after-result-frames";
pub const SMOKE_EXIT_ON_RESULT_ARG: &str = "--smoke-exit-on-result";
pub const SMOKE_SCREENSHOT_ARG: &str = "--smoke-screenshot";
pub const BOOT_REPLAY_ARG: &str = "--boot-replay";
pub const BOOT_REPLAY_FILE_ARG: &str = "--boot-replay-file";
pub const BOOT_COURSE_REPLAY_ARG: &str = "--boot-course-replay";
pub const BOOT_COURSE_ARG: &str = "--boot-course";
pub const PRACTICE_SHORT_ARG: &str = "-p";
pub const PRACTICE_ARG: &str = "--practice";
pub const PRACTICE_START_MS_ARG: &str = "--practice-start-ms";
pub const PRACTICE_END_MS_ARG: &str = "--practice-end-ms";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Run(AppOptions),
    Table(TableCommand),
    Songs(SongsCommand),
    Course(CourseCommand),
    Ir(IrCommand),
    Profile(ProfileCommand),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IrCommand {
    /// `ir login --email X [--password Y] [--base-url URL] [--provider NAME]`
    Login { email: String, password: Option<String>, base_url: Option<String>, provider: String },
    /// `ir logout [--provider NAME]`
    Logout { provider: String },
    /// `ir status`
    Status,
    /// `ir ranking <SHA256> [--ln-policy P] [--scope S] [--limit N]`
    Ranking { sha256: String, ln_policy: String, scope: String, limit: u32 },
    /// `ir sync` — pending のスコアジョブを送信する。
    Sync,
    /// `ir rivals [add <PLAYER_ID> | remove <PLAYER_ID>]`
    Rivals { action: Option<RivalAction> },
    /// `ir device-key [rotate]` — 署名鍵の表示 / ローテーション。
    DeviceKey { rotate: bool },
    /// `ir replay <SCORE_ID>` — IR リプレイをダウンロードする。
    Replay { score_id: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RivalAction {
    Add { player_id: String },
    Remove { player_id: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TableCommand {
    Add { url: String },
    List,
    Fetch { url: Option<String> },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SongsCommand {
    Add { path: String, recursive: bool, enabled: bool },
    List,
    Load { target: Option<String> },
    Reload { target: Option<String> },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CourseCommand {
    Import {
        path: String,
    },
    List,
    /// `course history <COURSE_ID> [--limit N]` — print recent attempts.
    History {
        course_id: i64,
        limit: u32,
    },
    /// `course attempt <SCORE_ID>` — print per-chart breakdown of a single attempt.
    Attempt {
        score_id: i64,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProfileCommand {
    List,
    Current,
    Use { id: String },
    Create { id: String, display_name: Option<String>, activate: bool },
    Copy { source_id: String, target_id: String, display_name: Option<String>, activate: bool },
}

pub fn parse_command<I, S>(args: I) -> Result<Command>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let args: Vec<String> = args.into_iter().map(|s| s.as_ref().to_string()).collect();
    match args.first().map(|s| s.as_str()) {
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
                    let target = rest.get(1).cloned();
                    Ok(Command::Songs(SongsCommand::Load { target }))
                }
                Some("reload") => {
                    let target = rest.get(1).cloned();
                    Ok(Command::Songs(SongsCommand::Reload { target }))
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
        Some("profile") => parse_profile_command(&args[1..]),
        Some("ir") => parse_ir_command(&args[1..]),
        _ => Ok(Command::Run(AppOptions::parse_args(args)?)),
    }
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

fn parse_ir_command(rest: &[String]) -> Result<Command> {
    match rest.first().map(|s| s.as_str()) {
        Some("login") => {
            let mut email = None;
            let mut password = None;
            let mut base_url = None;
            let mut provider = "bmz-official".to_string();
            let mut iter = rest[1..].iter();
            while let Some(flag) = iter.next() {
                match flag.as_str() {
                    "--email" => email = iter.next().cloned(),
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
            let email =
                email.ok_or_else(|| anyhow::anyhow!("ir login requires --email <EMAIL>"))?;
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
                "unknown ir subcommand: {sub}. Use: login, logout, status, ranking, sync, rivals, device-key, replay"
            )
        }
        None => {
            bail!(
                "ir requires a subcommand: login, logout, status, ranking, sync, rivals, device-key"
            )
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AppOptions {
    pub boot_play_sample: bool,
    /// Debug: start directly on a synthetic result screen.
    pub boot_result_sample: bool,
    /// beatoraja 互換: 譜面ファイル PATH を指定して起動時プレイ。
    pub boot_play_path: Option<String>,
    pub autoplay_on_start: bool,
    pub smoke_exit_after_frames: Option<u32>,
    pub smoke_exit_after_result_frames: Option<u32>,
    pub smoke_exit_on_result: bool,
    pub smoke_screenshot_path: Option<String>,
    /// `--boot-replay <SLOT>` / `-r1..4` で指定された 0-based のスロット index。
    pub boot_replay_slot: Option<u8>,
    /// `--boot-replay-file <PATH>`: リプレイファイルを直接指定して再生する。
    /// `bmz ir replay` でダウンロードした IR リプレイの再生に使う。
    pub boot_replay_file: Option<String>,
    /// `--boot-course-replay <COURSE_ID>` で指定されたコース id。
    /// 指定された場合、そのコースの最新 attempt を replay 再生する。
    pub boot_course_replay_id: Option<i64>,
    /// `--boot-course <COURSE_ID>` で指定されたコース id。
    /// 指定された場合、そのコースを fresh で起動する。
    pub boot_course_id: Option<i64>,
    /// `--renderer <backend>` で指定されたレンダラーバックエンド。
    pub renderer: Option<RendererBackend>,
    /// `-p` / `--practice`: boot into practice mode (CLI only).
    pub boot_practice: bool,
    pub practice_start_ms: Option<u32>,
    pub practice_end_ms: Option<u32>,
}

impl AppOptions {
    pub fn parse_args<I, S>(args: I) -> Result<Self>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let mut options = Self::default();
        let mut args = args.into_iter().peekable();

        while let Some(arg) = args.next() {
            let arg = arg.as_ref();
            if let Some(value) = arg.strip_prefix("--smoke-exit-after-frames=") {
                options.smoke_exit_after_frames = Some(parse_smoke_exit_after_frames_value(value)?);
                continue;
            }
            if let Some(value) = arg.strip_prefix("--smoke-exit-after-result-frames=") {
                options.smoke_exit_after_result_frames =
                    Some(parse_smoke_exit_after_result_frames_value(value)?);
                continue;
            }
            if let Some(value) = arg.strip_prefix("--smoke-screenshot=") {
                options.smoke_screenshot_path = Some(parse_smoke_screenshot_path(value)?);
                options.smoke_exit_after_frames.get_or_insert(3);
                continue;
            }
            if let Some(value) = arg.strip_prefix("--boot-replay-file=") {
                options.boot_replay_file = Some(value.to_string());
                continue;
            }
            if let Some(value) = arg.strip_prefix("--boot-replay=") {
                options.boot_replay_slot = Some(parse_boot_replay_slot(value)?);
                continue;
            }
            if let Some(value) = arg.strip_prefix("--boot-course-replay=") {
                options.boot_course_replay_id = Some(parse_boot_course_replay_id(value)?);
                continue;
            }
            if let Some(value) = arg.strip_prefix("--boot-course=") {
                options.boot_course_id = Some(parse_boot_course_id(value)?);
                continue;
            }
            if let Some(value) = arg.strip_prefix("--renderer=") {
                options.renderer = Some(parse_renderer_backend(value)?);
                continue;
            }
            if let Some(value) = arg.strip_prefix("--practice-start-ms=") {
                options.practice_start_ms = Some(parse_practice_ms(value, PRACTICE_START_MS_ARG)?);
                continue;
            }
            if let Some(value) = arg.strip_prefix("--practice-end-ms=") {
                options.practice_end_ms = Some(parse_practice_ms(value, PRACTICE_END_MS_ARG)?);
                continue;
            }

            match arg {
                BOOT_PLAY_SAMPLE_ARG => options.boot_play_sample = true,
                BOOT_RESULT_SAMPLE_ARG => options.boot_result_sample = true,
                AUTOPLAY_ON_START_ARG | AUTOPLAY_SHORT_ARG => options.autoplay_on_start = true,
                SMOKE_EXIT_ON_RESULT_ARG => options.smoke_exit_on_result = true,
                SMOKE_SCREENSHOT_ARG => {
                    let Some(value) = args.next() else {
                        bail!("{SMOKE_SCREENSHOT_ARG} requires an output path");
                    };
                    options.smoke_screenshot_path =
                        Some(parse_smoke_screenshot_path(value.as_ref())?);
                    options.smoke_exit_after_frames.get_or_insert(3);
                }
                "--help" | "-h" => {}
                SMOKE_EXIT_AFTER_FRAMES_ARG => {
                    let Some(value) = args.next() else {
                        bail!("{SMOKE_EXIT_AFTER_FRAMES_ARG} requires a frame count");
                    };
                    options.smoke_exit_after_frames =
                        Some(parse_smoke_exit_after_frames_value(value.as_ref())?);
                }
                SMOKE_EXIT_AFTER_RESULT_FRAMES_ARG => {
                    let Some(value) = args.next() else {
                        bail!("{SMOKE_EXIT_AFTER_RESULT_FRAMES_ARG} requires a frame count");
                    };
                    options.smoke_exit_after_result_frames =
                        Some(parse_smoke_exit_after_result_frames_value(value.as_ref())?);
                }
                BOOT_REPLAY_FILE_ARG => {
                    let Some(value) = args.next() else {
                        bail!("{BOOT_REPLAY_FILE_ARG} requires a replay file path");
                    };
                    options.boot_replay_file = Some(value.as_ref().to_string());
                }
                BOOT_REPLAY_ARG => {
                    let Some(value) = args.next() else {
                        bail!("{BOOT_REPLAY_ARG} requires a slot number (1..4)");
                    };
                    options.boot_replay_slot = Some(parse_boot_replay_slot(value.as_ref())?);
                }
                BOOT_COURSE_REPLAY_ARG => {
                    let Some(value) = args.next() else {
                        bail!("{BOOT_COURSE_REPLAY_ARG} requires a course id");
                    };
                    options.boot_course_replay_id =
                        Some(parse_boot_course_replay_id(value.as_ref())?);
                }
                BOOT_COURSE_ARG => {
                    let Some(value) = args.next() else {
                        bail!("{BOOT_COURSE_ARG} requires a course id");
                    };
                    options.boot_course_id = Some(parse_boot_course_id(value.as_ref())?);
                }
                "--renderer" => {
                    let Some(value) = args.next() else {
                        bail!("--renderer requires a backend (vulkan, metal, dx12, gl, auto)");
                    };
                    options.renderer = Some(parse_renderer_backend(value.as_ref())?);
                }
                PRACTICE_SHORT_ARG | PRACTICE_ARG => options.boot_practice = true,
                PRACTICE_START_MS_ARG => {
                    let Some(value) = args.next() else {
                        bail!("{PRACTICE_START_MS_ARG} requires milliseconds");
                    };
                    options.practice_start_ms =
                        Some(parse_practice_ms(value.as_ref(), PRACTICE_START_MS_ARG)?);
                }
                PRACTICE_END_MS_ARG => {
                    let Some(value) = args.next() else {
                        bail!("{PRACTICE_END_MS_ARG} requires milliseconds");
                    };
                    options.practice_end_ms =
                        Some(parse_practice_ms(value.as_ref(), PRACTICE_END_MS_ARG)?);
                }
                _ if let Some(slot) = parse_beatoraja_replay_flag(arg) => {
                    options.boot_replay_slot = Some(slot);
                }
                _ if arg.starts_with('-') => bail!("unknown argument: {arg}"),
                _ => options.boot_play_path = Some(arg.to_string()),
            }
        }

        Ok(options)
    }
}

pub fn args_request_help<I, S>(args: I) -> bool
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    args.into_iter().any(|arg| matches!(arg.as_ref(), "--help" | "-h"))
}

pub fn app_help_text() -> &'static str {
    "bmz-player\n\nUsage:\n  bmz-player [OPTIONS] [PATH]\n  bmz-player table <SUBCOMMAND>\n  bmz-player songs <SUBCOMMAND>\n  bmz-player course <SUBCOMMAND>\n  bmz-player profile <SUBCOMMAND>\n\nOptions:\n  [PATH]                                 Start the chart at PATH (beatoraja-style alias)\n  -p | --practice                        Start boot chart in practice mode (CLI only)\n  --practice-start-ms <MS>               Initial practice section start (milliseconds)\n  --practice-end-ms <MS>                 Initial practice section end (milliseconds)\n  -a                                     Enable autoplay for the boot chart (alias of --autoplay-on-start)\n  -r1 | -r2 | -r3 | -r4                  Start replay slot 1..4 for the boot chart\n  --boot-play-sample                     Start the bundled sample chart on boot\n  --boot-result-sample                   Start directly on a synthetic result screen (debug)\n  --autoplay-on-start                    Enable autoplay for started charts\n  --boot-replay <1..4>                   Start replay slot N for the boot chart\n  --boot-course <ID>                     Start course ID fresh on boot\n  --boot-course-replay <ID>              Replay the latest attempt of course ID on boot\n  --smoke-exit-after-frames <N>          Exit after N rendered frames, clamped to 1 or more\n  --smoke-exit-after-result-frames <N>   Exit after N rendered result frames, clamped to 1 or more\n  --smoke-exit-on-result                 Exit when the app reaches the result screen\n  --smoke-screenshot <PATH>              Save a PNG screenshot on smoke exit (defaults to 3 frames)\n  --renderer <backend>                   wgpu renderer backend (vulkan, metal, dx12, gl, auto)\n  -h, --help                             Print this help\n\nTable subcommands:\n  table add <URL>       Add a difficulty table source and fetch it\n  table list            List all stored difficulty tables\n  table fetch [URL]     Fetch/update configured tables, or a single URL\n\nSongs subcommands:\n  songs add <PATH> [--no-recursive] [--disabled]   Add a song root directory\n  songs list                                        List configured song roots\n  songs load [PATH|NAME]                            Scan song roots (incremental)\n  songs reload [PATH|NAME]                          Force rescan song roots\n\nCourse subcommands:\n  course import <PATH>             Import beatoraja course JSON from a file or directory\n  course list                      List stored courses\n  course history <ID> [--limit N]  Show recent attempts of course ID (default limit 10)\n  course attempt <SCORE_ID>        Show per-chart breakdown of a single course attempt\n\nProfile subcommands:\n  profile list                                      List profiles under data/profiles\n  profile current                                   Show the active profile id\n  profile use <ID>                                  Set active_profile in data/config.toml\n  profile create <ID> [--name NAME] [--activate]    Create a new empty profile\n  profile copy <SRC> <ID> [--name NAME] [--activate] Copy an existing profile directory\n\nExamples:\n  cargo run -p bmz-player -- /path/to/chart.bms\n  cargo run -p bmz-player -- -a /path/to/chart.bms\n  cargo run -p bmz-player -- -r2 /path/to/chart.bms\n  cargo run -p bmz-player -- --boot-play-sample --smoke-exit-after-frames 3\n  cargo run -p bmz-player -- --boot-result-sample --smoke-exit-after-result-frames 3\n  cargo run -p bmz-player -- --boot-play-sample --smoke-screenshot /tmp/bmz-play.png\n  cargo run -p bmz-player -- --boot-play-sample --boot-replay 1 --smoke-exit-on-result\n  cargo run -p bmz-player -- table add https://example.com/table.html\n  cargo run -p bmz-player -- table list\n  cargo run -p bmz-player -- table fetch https://example.com/table.html\n  cargo run -p bmz-player -- songs add /path/to/bms\n  cargo run -p bmz-player -- songs list\n  cargo run -p bmz-player -- songs load\n  cargo run -p bmz-player -- songs reload my-bms-folder\n  cargo run -p bmz-player -- course import /path/to/course.json\n  cargo run -p bmz-player -- course list\n  cargo run -p bmz-player -- profile create alt --name Alt --activate"
}

fn parse_practice_ms(value: &str, arg: &str) -> Result<u32> {
    let value = value.trim();
    if value.is_empty() {
        bail!("{arg} requires milliseconds");
    }
    let ms: u64 =
        value.parse().with_context(|| format!("invalid milliseconds for {arg}: {value}"))?;
    u32::try_from(ms).with_context(|| format!("{arg} value out of range: {value}"))
}

fn parse_smoke_exit_after_frames_value(value: &str) -> Result<u32> {
    parse_smoke_frame_count_value(value, SMOKE_EXIT_AFTER_FRAMES_ARG)
}

fn parse_smoke_exit_after_result_frames_value(value: &str) -> Result<u32> {
    parse_smoke_frame_count_value(value, SMOKE_EXIT_AFTER_RESULT_FRAMES_ARG)
}

fn parse_smoke_frame_count_value(value: &str, arg: &str) -> Result<u32> {
    let value = value.trim();
    if value.is_empty() {
        bail!("{arg} requires a frame count");
    }

    let frames =
        value.parse::<u32>().with_context(|| format!("invalid frame count for {arg}: {value}"))?;
    Ok(frames.max(1))
}

fn parse_smoke_screenshot_path(value: &str) -> Result<String> {
    let value = value.trim();
    if value.is_empty() {
        bail!("{SMOKE_SCREENSHOT_ARG} requires an output path");
    }
    Ok(value.to_string())
}

fn parse_boot_course_replay_id(value: &str) -> Result<i64> {
    let value = value.trim();
    if value.is_empty() {
        bail!("{BOOT_COURSE_REPLAY_ARG} requires a course id");
    }
    let id: i64 = value
        .parse()
        .with_context(|| format!("invalid course id for {BOOT_COURSE_REPLAY_ARG}: {value}"))?;
    if id <= 0 {
        bail!("{BOOT_COURSE_REPLAY_ARG} course id must be positive (got {id})");
    }
    Ok(id)
}

fn parse_course_history_id(value: &str) -> Result<i64> {
    let value = value.trim();
    if value.is_empty() {
        bail!("course history requires a COURSE_ID");
    }
    let id: i64 =
        value.parse().with_context(|| format!("invalid course id for course history: {value}"))?;
    if id <= 0 {
        bail!("course history COURSE_ID must be positive (got {id})");
    }
    Ok(id)
}

fn parse_course_history_limit(flags: &[String]) -> Result<u32> {
    // No flags → default limit.
    let Some(flag) = flags.first() else {
        return Ok(10);
    };
    if let Some(value) = flag.strip_prefix("--limit=") {
        // `--limit=N` consumes one token; any extra tokens are unknown.
        if flags.len() > 1 {
            bail!("unknown flag for course history: {}", flags[1]);
        }
        return parse_history_limit_value(value);
    }
    if flag == "--limit" {
        let Some(value) = flags.get(1) else {
            bail!("--limit requires a positive integer");
        };
        if flags.len() > 2 {
            bail!("unknown flag for course history: {}", flags[2]);
        }
        return parse_history_limit_value(value);
    }
    bail!("unknown flag for course history: {flag}");
}

fn parse_course_attempt_id(value: &str) -> Result<i64> {
    let value = value.trim();
    if value.is_empty() {
        bail!("course attempt requires a SCORE_ID");
    }
    let id: i64 =
        value.parse().with_context(|| format!("invalid score id for course attempt: {value}"))?;
    if id <= 0 {
        bail!("course attempt SCORE_ID must be positive (got {id})");
    }
    Ok(id)
}

fn parse_history_limit_value(value: &str) -> Result<u32> {
    let value = value.trim();
    if value.is_empty() {
        bail!("--limit requires a positive integer");
    }
    let n: u32 = value.parse().with_context(|| format!("invalid --limit value: {value}"))?;
    if n == 0 {
        bail!("--limit must be greater than 0");
    }
    Ok(n)
}

fn parse_boot_course_id(value: &str) -> Result<i64> {
    let value = value.trim();
    if value.is_empty() {
        bail!("{BOOT_COURSE_ARG} requires a course id");
    }
    let id: i64 = value
        .parse()
        .with_context(|| format!("invalid course id for {BOOT_COURSE_ARG}: {value}"))?;
    if id <= 0 {
        bail!("{BOOT_COURSE_ARG} course id must be positive (got {id})");
    }
    Ok(id)
}

fn parse_boot_replay_slot(value: &str) -> Result<u8> {
    let value = value.trim();
    if value.is_empty() {
        bail!("{BOOT_REPLAY_ARG} requires a slot number (1..4)");
    }
    let n: u8 =
        value.parse().with_context(|| format!("invalid slot for {BOOT_REPLAY_ARG}: {value}"))?;
    if !(1..=4).contains(&n) {
        bail!("{BOOT_REPLAY_ARG} slot must be 1..4 (got {n})");
    }
    Ok(n - 1)
}

/// beatoraja 互換の `-r1`..`-r4` を 0-based スロット index に変換する。
fn parse_beatoraja_replay_flag(arg: &str) -> Option<u8> {
    let rest = arg.strip_prefix('-')?;
    match rest {
        "r1" => Some(0),
        "r2" => Some(1),
        "r3" => Some(2),
        "r4" => Some(3),
        _ => None,
    }
}

fn parse_renderer_backend(value: &str) -> Result<RendererBackend> {
    match value.trim().to_lowercase().as_str() {
        "auto" => Ok(RendererBackend::Auto),
        "vulkan" => Ok(RendererBackend::Vulkan),
        "metal" => Ok(RendererBackend::Metal),
        "dx12" | "directx12" | "d3d12" => Ok(RendererBackend::Dx12),
        "gl" | "opengl" => Ok(RendererBackend::Gl),
        other => {
            bail!("unknown renderer backend: {other}. Valid options: vulkan, metal, dx12, gl, auto")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_options_parse_beatoraja_style_boot_path() {
        let options = AppOptions::parse_args(["/music/song.bms"]).unwrap();
        assert_eq!(options.boot_play_path.as_deref(), Some("/music/song.bms"));
        assert!(!options.autoplay_on_start);
        assert_eq!(options.boot_replay_slot, None);

        let options = AppOptions::parse_args(["-a", "/music/song.bms"]).unwrap();
        assert!(options.autoplay_on_start);
        assert_eq!(options.boot_play_path.as_deref(), Some("/music/song.bms"));

        let options = AppOptions::parse_args(["-r3", "/music/song.bms"]).unwrap();
        assert_eq!(options.boot_replay_slot, Some(2));
        assert_eq!(options.boot_play_path.as_deref(), Some("/music/song.bms"));

        let options =
            AppOptions::parse_args(["--renderer", "vulkan", "-a", "-r1", "/music/song.bms"])
                .unwrap();
        assert_eq!(options.renderer, Some(RendererBackend::Vulkan));
        assert!(options.autoplay_on_start);
        assert_eq!(options.boot_replay_slot, Some(0));
        assert_eq!(options.boot_play_path.as_deref(), Some("/music/song.bms"));
    }

    #[test]
    fn app_options_parse_practice_flags() {
        let options =
            AppOptions::parse_args(["-p", "--practice-start-ms=5000", "/music/song.bms"]).unwrap();
        assert!(options.boot_practice);
        assert_eq!(options.practice_start_ms, Some(5000));
        assert_eq!(options.practice_end_ms, None);
        assert_eq!(options.boot_play_path.as_deref(), Some("/music/song.bms"));

        let options = AppOptions::parse_args([
            "--practice",
            "--practice-end-ms",
            "120000",
            "/music/song.bms",
        ])
        .unwrap();
        assert!(options.boot_practice);
        assert_eq!(options.practice_end_ms, Some(120_000));
    }

    #[test]
    fn parse_beatoraja_replay_flag_maps_slots() {
        assert_eq!(parse_beatoraja_replay_flag("-r1"), Some(0));
        assert_eq!(parse_beatoraja_replay_flag("-r4"), Some(3));
        assert_eq!(parse_beatoraja_replay_flag("-a"), None);
        assert_eq!(parse_beatoraja_replay_flag("-r5"), None);
    }

    #[test]
    fn app_options_parse_flags() {
        let options = AppOptions::parse_args([
            "--boot-play-sample",
            "--boot-result-sample",
            "--autoplay-on-start",
            "--smoke-exit-after-frames",
            "12",
            "--smoke-exit-after-result-frames",
            "120",
            "--smoke-exit-on-result",
        ])
        .unwrap();

        assert!(options.boot_play_sample);
        assert!(options.boot_result_sample);
        assert!(options.autoplay_on_start);
        assert_eq!(options.smoke_exit_after_frames, Some(12));
        assert_eq!(options.smoke_exit_after_result_frames, Some(120));
        assert!(options.smoke_exit_on_result);
    }

    #[test]
    fn app_options_parse_equals_form() {
        let options = AppOptions::parse_args([
            "--smoke-exit-after-frames=3",
            "--smoke-exit-after-result-frames=60",
        ])
        .unwrap();

        assert_eq!(options.smoke_exit_after_frames, Some(3));
        assert_eq!(options.smoke_exit_after_result_frames, Some(60));
    }

    #[test]
    fn app_options_parse_smoke_screenshot_defaults_to_three_frames() {
        let options = AppOptions::parse_args(["--smoke-screenshot", "/tmp/bmz.png"]).unwrap();

        assert_eq!(options.smoke_screenshot_path.as_deref(), Some("/tmp/bmz.png"));
        assert_eq!(options.smoke_exit_after_frames, Some(3));
    }

    #[test]
    fn app_options_parse_smoke_screenshot_keeps_explicit_frame_count() {
        let options = AppOptions::parse_args([
            "--smoke-exit-after-frames=8",
            "--smoke-screenshot=/tmp/bmz.png",
        ])
        .unwrap();

        assert_eq!(options.smoke_screenshot_path.as_deref(), Some("/tmp/bmz.png"));
        assert_eq!(options.smoke_exit_after_frames, Some(8));
    }

    #[test]
    fn app_options_clamps_zero_frame_count_to_one() {
        let options = AppOptions::parse_args([
            "--smoke-exit-after-frames",
            "0",
            "--smoke-exit-after-result-frames",
            "0",
        ])
        .unwrap();

        assert_eq!(options.smoke_exit_after_frames, Some(1));
        assert_eq!(options.smoke_exit_after_result_frames, Some(1));
    }

    #[test]
    fn app_options_reject_invalid_arguments() {
        assert!(AppOptions::parse_args(["--unknown"]).is_err());
        assert!(AppOptions::parse_args(["--smoke-exit-after-frames"]).is_err());
        assert!(AppOptions::parse_args(["--smoke-exit-after-frames", "abc"]).is_err());
        assert!(AppOptions::parse_args(["--smoke-exit-after-result-frames"]).is_err());
        assert!(AppOptions::parse_args(["--smoke-exit-after-result-frames", "abc"]).is_err());
        assert!(AppOptions::parse_args(["--smoke-screenshot"]).is_err());
        assert!(AppOptions::parse_args(["--smoke-screenshot", ""]).is_err());
    }

    #[test]
    fn help_args_are_detected() {
        assert!(args_request_help(["--help"]));
        assert!(args_request_help(["-h"]));
        assert!(args_request_help(["--boot-play-sample", "--help"]));
        assert!(!args_request_help(["--boot-play-sample"]));
    }

    #[test]
    fn help_text_lists_supported_options() {
        let help = app_help_text();

        assert!(help.contains("--boot-play-sample"));
        assert!(help.contains("--boot-result-sample"));
        assert!(help.contains("--autoplay-on-start"));
        assert!(help.contains("--smoke-exit-after-frames"));
        assert!(help.contains("--smoke-exit-after-result-frames"));
        assert!(help.contains("--smoke-exit-on-result"));
        assert!(help.contains("--smoke-screenshot"));
        assert!(help.contains("--renderer"));
        assert!(help.contains("table add"));
        assert!(help.contains("table list"));
        assert!(help.contains("table fetch"));
        assert!(help.contains("course import"));
        assert!(help.contains("course list"));
        assert!(help.contains("profile create"));
        assert!(help.contains("profile copy"));
    }

    #[test]
    fn parse_command_routes_table_subcommands() {
        assert_eq!(
            parse_command(["table", "add", "https://example.com/"]).unwrap(),
            Command::Table(TableCommand::Add { url: "https://example.com/".to_string() })
        );
        assert_eq!(parse_command(["table", "list"]).unwrap(), Command::Table(TableCommand::List));
        assert_eq!(
            parse_command(["table", "fetch"]).unwrap(),
            Command::Table(TableCommand::Fetch { url: None })
        );
        assert_eq!(
            parse_command(["table", "fetch", "https://example.com/"]).unwrap(),
            Command::Table(TableCommand::Fetch { url: Some("https://example.com/".to_string()) })
        );
    }

    #[test]
    fn parse_command_routes_app_flags() {
        assert!(matches!(
            parse_command(["--boot-play-sample"]).unwrap(),
            Command::Run(opts) if opts.boot_play_sample
        ));
        assert!(matches!(
            parse_command(["--boot-result-sample"]).unwrap(),
            Command::Run(opts) if opts.boot_result_sample
        ));
        assert!(matches!(parse_command([] as [&str; 0]).unwrap(), Command::Run(_)));
    }

    #[test]
    fn parse_command_rejects_unknown_table_subcommand() {
        assert!(parse_command(["table", "remove"]).is_err());
        assert!(parse_command(["table"]).is_err());
        assert!(parse_command(["table", "add"]).is_err());
    }

    #[test]
    fn parse_command_routes_songs_subcommands() {
        assert_eq!(
            parse_command(["songs", "add", "/bms"]).unwrap(),
            Command::Songs(SongsCommand::Add {
                path: "/bms".to_string(),
                recursive: true,
                enabled: true,
            })
        );
        assert_eq!(
            parse_command(["songs", "add", "/bms", "--no-recursive", "--disabled"]).unwrap(),
            Command::Songs(SongsCommand::Add {
                path: "/bms".to_string(),
                recursive: false,
                enabled: false,
            })
        );
        assert_eq!(parse_command(["songs", "list"]).unwrap(), Command::Songs(SongsCommand::List));
        assert_eq!(
            parse_command(["songs", "load"]).unwrap(),
            Command::Songs(SongsCommand::Load { target: None })
        );
        assert_eq!(
            parse_command(["songs", "load", "my-folder"]).unwrap(),
            Command::Songs(SongsCommand::Load { target: Some("my-folder".to_string()) })
        );
        assert_eq!(
            parse_command(["songs", "reload"]).unwrap(),
            Command::Songs(SongsCommand::Reload { target: None })
        );
        assert_eq!(
            parse_command(["songs", "reload", "/bms"]).unwrap(),
            Command::Songs(SongsCommand::Reload { target: Some("/bms".to_string()) })
        );
    }

    #[test]
    fn parse_command_routes_course_subcommands() {
        assert_eq!(
            parse_command(["course", "import", "/course"]).unwrap(),
            Command::Course(CourseCommand::Import { path: "/course".to_string() })
        );
        assert_eq!(
            parse_command(["course", "list"]).unwrap(),
            Command::Course(CourseCommand::List)
        );
    }

    #[test]
    fn parse_command_routes_profile_subcommands() {
        assert_eq!(
            parse_command(["profile", "list"]).unwrap(),
            Command::Profile(ProfileCommand::List)
        );
        assert_eq!(
            parse_command(["profile", "current"]).unwrap(),
            Command::Profile(ProfileCommand::Current)
        );
        assert_eq!(
            parse_command(["profile", "use", "alt"]).unwrap(),
            Command::Profile(ProfileCommand::Use { id: "alt".to_string() })
        );
        assert_eq!(
            parse_command(["profile", "create", "alt", "--name", "Alt", "--activate"]).unwrap(),
            Command::Profile(ProfileCommand::Create {
                id: "alt".to_string(),
                display_name: Some("Alt".to_string()),
                activate: true,
            })
        );
        assert_eq!(
            parse_command(["profile", "copy", "default", "alt", "--display-name", "Alt Copy",])
                .unwrap(),
            Command::Profile(ProfileCommand::Copy {
                source_id: "default".to_string(),
                target_id: "alt".to_string(),
                display_name: Some("Alt Copy".to_string()),
                activate: false,
            })
        );
    }

    #[test]
    fn parse_command_rejects_invalid_profile_subcommands() {
        assert!(parse_command(["profile"]).is_err());
        assert!(parse_command(["profile", "create"]).is_err());
        assert!(parse_command(["profile", "copy", "default"]).is_err());
        assert!(parse_command(["profile", "use"]).is_err());
        assert!(parse_command(["profile", "delete", "default"]).is_err());
    }

    #[test]
    fn parse_command_rejects_unknown_course_subcommand() {
        assert!(parse_command(["course", "remove"]).is_err());
        assert!(parse_command(["course"]).is_err());
        assert!(parse_command(["course", "import"]).is_err());
    }

    #[test]
    fn parse_command_rejects_unknown_songs_subcommand() {
        assert!(parse_command(["songs", "remove"]).is_err());
        assert!(parse_command(["songs"]).is_err());
        assert!(parse_command(["songs", "add"]).is_err());
    }

    #[test]
    fn help_text_lists_songs_subcommands() {
        let help = app_help_text();
        assert!(help.contains("songs add"));
        assert!(help.contains("songs list"));
        assert!(help.contains("songs load"));
        assert!(help.contains("songs reload"));
    }

    #[test]
    fn app_options_parse_renderer_arg() {
        let options = AppOptions::parse_args(["--renderer", "vulkan"]).unwrap();
        assert_eq!(options.renderer, Some(RendererBackend::Vulkan));

        let options = AppOptions::parse_args(["--renderer=metal"]).unwrap();
        assert_eq!(options.renderer, Some(RendererBackend::Metal));

        assert!(AppOptions::parse_args(["--renderer", "invalid"]).is_err());
    }

    #[test]
    fn app_options_parse_boot_replay_slot_arg() {
        let options = AppOptions::parse_args(["--boot-replay", "2"]).unwrap();
        assert_eq!(options.boot_replay_slot, Some(1));

        let options = AppOptions::parse_args(["--boot-replay", "4"]).unwrap();
        assert_eq!(options.boot_replay_slot, Some(3));
    }

    #[test]
    fn app_options_parse_boot_replay_equals_form() {
        let options = AppOptions::parse_args(["--boot-replay=1"]).unwrap();
        assert_eq!(options.boot_replay_slot, Some(0));
    }

    #[test]
    fn app_options_reject_boot_replay_out_of_range() {
        assert!(AppOptions::parse_args(["--boot-replay", "0"]).is_err());
        assert!(AppOptions::parse_args(["--boot-replay", "5"]).is_err());
        assert!(AppOptions::parse_args(["--boot-replay"]).is_err());
        assert!(AppOptions::parse_args(["--boot-replay", "abc"]).is_err());
    }

    #[test]
    fn help_text_lists_boot_replay() {
        let help = app_help_text();
        assert!(help.contains("--boot-replay"));
    }

    #[test]
    fn app_options_parse_boot_course_replay_id() {
        let options = AppOptions::parse_args(["--boot-course-replay", "42"]).unwrap();
        assert_eq!(options.boot_course_replay_id, Some(42));

        let options = AppOptions::parse_args(["--boot-course-replay=7"]).unwrap();
        assert_eq!(options.boot_course_replay_id, Some(7));
    }

    #[test]
    fn app_options_reject_invalid_boot_course_replay_id() {
        assert!(AppOptions::parse_args(["--boot-course-replay"]).is_err());
        assert!(AppOptions::parse_args(["--boot-course-replay", "0"]).is_err());
        assert!(AppOptions::parse_args(["--boot-course-replay", "-1"]).is_err());
        assert!(AppOptions::parse_args(["--boot-course-replay", "abc"]).is_err());
    }

    #[test]
    fn help_text_lists_boot_course_replay() {
        let help = app_help_text();
        assert!(help.contains("--boot-course-replay"));
    }

    #[test]
    fn app_options_parse_boot_course_id() {
        let options = AppOptions::parse_args(["--boot-course", "42"]).unwrap();
        assert_eq!(options.boot_course_id, Some(42));

        let options = AppOptions::parse_args(["--boot-course=7"]).unwrap();
        assert_eq!(options.boot_course_id, Some(7));
    }

    #[test]
    fn app_options_reject_invalid_boot_course_id() {
        assert!(AppOptions::parse_args(["--boot-course"]).is_err());
        assert!(AppOptions::parse_args(["--boot-course", "0"]).is_err());
        assert!(AppOptions::parse_args(["--boot-course", "-1"]).is_err());
        assert!(AppOptions::parse_args(["--boot-course", "abc"]).is_err());
    }

    #[test]
    fn help_text_lists_boot_course() {
        let help = app_help_text();
        assert!(help.contains("--boot-course "));
    }

    #[test]
    fn parse_command_routes_course_history() {
        assert_eq!(
            parse_command(["course", "history", "42"]).unwrap(),
            Command::Course(CourseCommand::History { course_id: 42, limit: 10 }),
        );
        assert_eq!(
            parse_command(["course", "history", "42", "--limit", "5"]).unwrap(),
            Command::Course(CourseCommand::History { course_id: 42, limit: 5 }),
        );
        assert_eq!(
            parse_command(["course", "history", "42", "--limit=20"]).unwrap(),
            Command::Course(CourseCommand::History { course_id: 42, limit: 20 }),
        );
    }

    #[test]
    fn parse_command_rejects_invalid_course_history() {
        assert!(parse_command(["course", "history"]).is_err());
        assert!(parse_command(["course", "history", "0"]).is_err());
        assert!(parse_command(["course", "history", "-1"]).is_err());
        assert!(parse_command(["course", "history", "abc"]).is_err());
        assert!(parse_command(["course", "history", "1", "--limit"]).is_err());
        assert!(parse_command(["course", "history", "1", "--limit=0"]).is_err());
        assert!(parse_command(["course", "history", "1", "--unknown"]).is_err());
    }

    #[test]
    fn help_text_lists_course_history() {
        let help = app_help_text();
        assert!(help.contains("course history"));
    }

    #[test]
    fn parse_command_routes_course_attempt() {
        assert_eq!(
            parse_command(["course", "attempt", "7"]).unwrap(),
            Command::Course(CourseCommand::Attempt { score_id: 7 }),
        );
    }

    #[test]
    fn parse_command_rejects_invalid_course_attempt() {
        assert!(parse_command(["course", "attempt"]).is_err());
        assert!(parse_command(["course", "attempt", "0"]).is_err());
        assert!(parse_command(["course", "attempt", "-1"]).is_err());
        assert!(parse_command(["course", "attempt", "abc"]).is_err());
        assert!(parse_command(["course", "attempt", "1", "--unknown"]).is_err());
    }

    #[test]
    fn help_text_lists_course_attempt() {
        let help = app_help_text();
        assert!(help.contains("course attempt"));
    }
}

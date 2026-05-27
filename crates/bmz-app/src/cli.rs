use crate::config::app_config::RendererBackend;
use anyhow::{Context, Result, bail};

pub const BOOT_PLAY_SAMPLE_ARG: &str = "--boot-play-sample";
pub const AUTOPLAY_ON_START_ARG: &str = "--autoplay-on-start";
pub const SMOKE_EXIT_AFTER_FRAMES_ARG: &str = "--smoke-exit-after-frames";
pub const SMOKE_EXIT_ON_RESULT_ARG: &str = "--smoke-exit-on-result";
pub const BOOT_REPLAY_ARG: &str = "--boot-replay";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Run(AppOptions),
    Table(TableCommand),
    Songs(SongsCommand),
    Course(CourseCommand),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TableCommand {
    Add { url: String },
    List,
    Fetch,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SongsCommand {
    Add { path: String, recursive: bool, enabled: bool },
    List,
    Reload,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CourseCommand {
    Import { path: String },
    List,
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
                Some("fetch") => Ok(Command::Table(TableCommand::Fetch)),
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
                Some("reload") => Ok(Command::Songs(SongsCommand::Reload)),
                Some(sub) => bail!("unknown songs subcommand: {sub}. Use: add, list, reload"),
                None => bail!("songs requires a subcommand: add, list, reload"),
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
                Some(sub) => bail!("unknown course subcommand: {sub}. Use: import, list"),
                None => bail!("course requires a subcommand: import, list"),
            }
        }
        _ => Ok(Command::Run(AppOptions::parse_args(args)?)),
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AppOptions {
    pub boot_play_sample: bool,
    pub autoplay_on_start: bool,
    pub smoke_exit_after_frames: Option<u32>,
    pub smoke_exit_on_result: bool,
    /// `--boot-replay <SLOT>` で指定された 0-based のスロット index。
    pub boot_replay_slot: Option<u8>,
    /// `--renderer <backend>` で指定されたレンダラーバックエンド。
    pub renderer: Option<RendererBackend>,
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
            if let Some(value) = arg.strip_prefix("--boot-replay=") {
                options.boot_replay_slot = Some(parse_boot_replay_slot(value)?);
                continue;
            }
            if let Some(value) = arg.strip_prefix("--renderer=") {
                options.renderer = Some(parse_renderer_backend(value)?);
                continue;
            }

            match arg {
                BOOT_PLAY_SAMPLE_ARG => options.boot_play_sample = true,
                AUTOPLAY_ON_START_ARG => options.autoplay_on_start = true,
                SMOKE_EXIT_ON_RESULT_ARG => options.smoke_exit_on_result = true,
                "--help" | "-h" => {}
                SMOKE_EXIT_AFTER_FRAMES_ARG => {
                    let Some(value) = args.next() else {
                        bail!("{SMOKE_EXIT_AFTER_FRAMES_ARG} requires a frame count");
                    };
                    options.smoke_exit_after_frames =
                        Some(parse_smoke_exit_after_frames_value(value.as_ref())?);
                }
                BOOT_REPLAY_ARG => {
                    let Some(value) = args.next() else {
                        bail!("{BOOT_REPLAY_ARG} requires a slot number (1..4)");
                    };
                    options.boot_replay_slot = Some(parse_boot_replay_slot(value.as_ref())?);
                }
                "--renderer" => {
                    let Some(value) = args.next() else {
                        bail!("--renderer requires a backend (vulkan, metal, dx12, gl, auto)");
                    };
                    options.renderer = Some(parse_renderer_backend(value.as_ref())?);
                }
                _ => bail!("unknown argument: {arg}"),
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
    "bmz-app\n\nUsage:\n  bmz-app [OPTIONS]\n  bmz-app table <SUBCOMMAND>\n  bmz-app songs <SUBCOMMAND>\n  bmz-app course <SUBCOMMAND>\n\nOptions:\n  --boot-play-sample              Start the bundled sample chart on boot\n  --autoplay-on-start             Enable autoplay for started charts\n  --boot-replay <1..4>            Start the bundled sample chart in replay mode using slot N\n  --smoke-exit-after-frames <N>   Exit after N rendered frames, clamped to 1 or more\n  --smoke-exit-on-result          Exit when the app reaches the result screen\n  --renderer <backend>            wgpu renderer backend (vulkan, metal, dx12, gl, auto)\n  -h, --help                      Print this help\n\nTable subcommands:\n  table add <URL>   Add a difficulty table source and fetch it\n  table list        List all stored difficulty tables\n  table fetch       Fetch/update all configured difficulty tables\n\nSongs subcommands:\n  songs add <PATH> [--no-recursive] [--disabled]   Add a song root directory\n  songs list                                        List configured song roots\n  songs reload                                      Scan all song roots and update the library\n\nCourse subcommands:\n  course import <PATH>   Import beatoraja course JSON from a file or directory\n  course list            List stored courses\n\nExamples:\n  cargo run -p bmz-app -- --boot-play-sample --smoke-exit-after-frames 3\n  cargo run -p bmz-app -- --boot-play-sample --boot-replay 1 --smoke-exit-on-result\n  cargo run -p bmz-app -- table add https://example.com/table.html\n  cargo run -p bmz-app -- table list\n  cargo run -p bmz-app -- songs add /path/to/bms\n  cargo run -p bmz-app -- songs list\n  cargo run -p bmz-app -- songs reload\n  cargo run -p bmz-app -- course import /path/to/course.json\n  cargo run -p bmz-app -- course list"
}

fn parse_smoke_exit_after_frames_value(value: &str) -> Result<u32> {
    let value = value.trim();
    if value.is_empty() {
        bail!("{SMOKE_EXIT_AFTER_FRAMES_ARG} requires a frame count");
    }

    let frames = value.parse::<u32>().with_context(|| {
        format!("invalid frame count for {SMOKE_EXIT_AFTER_FRAMES_ARG}: {value}")
    })?;
    Ok(frames.max(1))
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
    fn app_options_parse_flags() {
        let options = AppOptions::parse_args([
            "--boot-play-sample",
            "--autoplay-on-start",
            "--smoke-exit-after-frames",
            "12",
            "--smoke-exit-on-result",
        ])
        .unwrap();

        assert!(options.boot_play_sample);
        assert!(options.autoplay_on_start);
        assert_eq!(options.smoke_exit_after_frames, Some(12));
        assert!(options.smoke_exit_on_result);
    }

    #[test]
    fn app_options_parse_equals_form() {
        let options = AppOptions::parse_args(["--smoke-exit-after-frames=3"]).unwrap();

        assert_eq!(options.smoke_exit_after_frames, Some(3));
    }

    #[test]
    fn app_options_clamps_zero_frame_count_to_one() {
        let options = AppOptions::parse_args(["--smoke-exit-after-frames", "0"]).unwrap();

        assert_eq!(options.smoke_exit_after_frames, Some(1));
    }

    #[test]
    fn app_options_reject_invalid_arguments() {
        assert!(AppOptions::parse_args(["--unknown"]).is_err());
        assert!(AppOptions::parse_args(["--smoke-exit-after-frames"]).is_err());
        assert!(AppOptions::parse_args(["--smoke-exit-after-frames", "abc"]).is_err());
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
        assert!(help.contains("--autoplay-on-start"));
        assert!(help.contains("--smoke-exit-after-frames"));
        assert!(help.contains("--smoke-exit-on-result"));
        assert!(help.contains("--renderer"));
        assert!(help.contains("table add"));
        assert!(help.contains("table list"));
        assert!(help.contains("table fetch"));
        assert!(help.contains("course import"));
        assert!(help.contains("course list"));
    }

    #[test]
    fn parse_command_routes_table_subcommands() {
        assert_eq!(
            parse_command(["table", "add", "https://example.com/"]).unwrap(),
            Command::Table(TableCommand::Add { url: "https://example.com/".to_string() })
        );
        assert_eq!(parse_command(["table", "list"]).unwrap(), Command::Table(TableCommand::List));
        assert_eq!(parse_command(["table", "fetch"]).unwrap(), Command::Table(TableCommand::Fetch));
    }

    #[test]
    fn parse_command_routes_app_flags() {
        assert!(matches!(
            parse_command(["--boot-play-sample"]).unwrap(),
            Command::Run(opts) if opts.boot_play_sample
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
            parse_command(["songs", "reload"]).unwrap(),
            Command::Songs(SongsCommand::Reload)
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
}

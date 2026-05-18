use anyhow::{Context, Result, bail};

pub const BOOT_PLAY_SAMPLE_ARG: &str = "--boot-play-sample";
pub const AUTOPLAY_ON_START_ARG: &str = "--autoplay-on-start";
pub const SMOKE_EXIT_AFTER_FRAMES_ARG: &str = "--smoke-exit-after-frames";
pub const SMOKE_EXIT_ON_RESULT_ARG: &str = "--smoke-exit-on-result";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Run(AppOptions),
    Table(TableCommand),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TableCommand {
    Add { url: String },
    List,
    Fetch,
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
        _ => Ok(Command::Run(AppOptions::parse_args(args)?)),
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AppOptions {
    pub boot_play_sample: bool,
    pub autoplay_on_start: bool,
    pub smoke_exit_after_frames: Option<u32>,
    pub smoke_exit_on_result: bool,
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
    "bmz-app\n\nUsage:\n  bmz-app [OPTIONS]\n  bmz-app table <SUBCOMMAND>\n\nOptions:\n  --boot-play-sample              Start the bundled sample chart on boot\n  --autoplay-on-start             Enable autoplay for started charts\n  --smoke-exit-after-frames <N>   Exit after N rendered frames, clamped to 1 or more\n  --smoke-exit-on-result          Exit when the app reaches the result screen\n  -h, --help                      Print this help\n\nTable subcommands:\n  table add <URL>   Add a difficulty table source and fetch it\n  table list        List all stored difficulty tables\n  table fetch       Fetch/update all configured difficulty tables\n\nExamples:\n  cargo run -p bmz-app -- --boot-play-sample --smoke-exit-after-frames 3\n  cargo run -p bmz-app -- table add https://example.com/table.html\n  cargo run -p bmz-app -- table list"
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
        assert!(help.contains("table add"));
        assert!(help.contains("table list"));
        assert!(help.contains("table fetch"));
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
}

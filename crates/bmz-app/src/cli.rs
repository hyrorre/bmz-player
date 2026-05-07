use anyhow::{Context, Result, bail};

pub const BOOT_PLAY_SAMPLE_ARG: &str = "--boot-play-sample";
pub const AUTOPLAY_ON_START_ARG: &str = "--autoplay-on-start";
pub const SMOKE_EXIT_AFTER_FRAMES_ARG: &str = "--smoke-exit-after-frames";
pub const SMOKE_EXIT_ON_RESULT_ARG: &str = "--smoke-exit-on-result";

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
    "bmz-app\n\nUsage:\n  bmz-app [OPTIONS]\n\nOptions:\n  --boot-play-sample              Start the bundled sample chart on boot\n  --autoplay-on-start             Enable autoplay for started charts\n  --smoke-exit-after-frames <N>   Exit after N rendered frames, clamped to 1 or more\n  --smoke-exit-on-result          Exit when the app reaches the result screen\n  -h, --help                      Print this help\n\nExamples:\n  cargo run -p bmz-app -- --boot-play-sample --smoke-exit-after-frames 3\n  cargo run -p bmz-app -- --boot-play-sample --autoplay-on-start --smoke-exit-on-result"
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
    }
}

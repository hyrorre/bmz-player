use crate::config::app_config::RendererBackend;
use anyhow::{Context, Result, bail};

pub const BOOT_PLAY_SAMPLE_ARG: &str = "--boot-play-sample";
pub const BOOT_RESULT_SAMPLE_ARG: &str = "--boot-result-sample";
pub const AUTOPLAY_ON_START_ARG: &str = "--autoplay-on-start";
pub const AUTOPLAY_SHORT_ARG: &str = "-a";
pub const VIEWER_PLAY_ARG: &str = "--viewer-play";
pub const VIEWER_PLAY_SHORT_ARG: &str = "-P";
pub const VIEWER_BATTLE_ARG: &str = "--battle";
pub const VIEWER_BATTLE_SHORT_ARG: &str = "-B";
pub const VIEWER_PROFILE_ARG: &str = "--profile";
pub const VIEWER_STOP_ARG: &str = "--viewer-stop";
pub const VIEWER_STOP_SHORT_ARG: &str = "-S";
pub const START_MEASURE_ARG: &str = "--start-measure";
pub const START_MEASURE_SHORT_ARG: &str = "-N";
pub const SKIP_DECIDE_ARG: &str = "--skip-decide";
pub const SKIP_RESULT_ARG: &str = "--skip-result";
pub const SMOKE_EXIT_AFTER_FRAMES_ARG: &str = "--smoke-exit-after-frames";
pub const SMOKE_EXIT_AFTER_PLAY_FRAMES_ARG: &str = "--smoke-exit-after-play-frames";
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
pub const LUA_SKIN_RUNTIME_ARG: &str = "--lua-skin-runtime";

mod export;
mod help;
pub use export::{FrameRate, VideoExportOptions};
mod ir;
mod model;
mod options;
mod parse;

#[cfg(test)]
use help::parse_beatoraja_replay_flag;
pub use help::{app_help_text, args_request_help};
pub use model::{
    Command, CourseCommand, IrCommand, ParsedCommand, ProfileCommand, ReplayCommand, RivalAction,
    SongsCommand, TableCommand,
};
pub use options::AppOptions;
pub use parse::{parse_cli_command, parse_command};

#[cfg(test)]
#[path = "cli/tests.rs"]
mod tests;

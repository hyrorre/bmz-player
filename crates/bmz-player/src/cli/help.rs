pub fn args_request_help<I, S>(args: I) -> bool
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    args.into_iter().any(|arg| matches!(arg.as_ref(), "--help" | "-h"))
}

pub fn app_help_text() -> String {
    "bmz-player\n\nUsage:\n  bmz-player [OPTIONS] [PATH]\n  bmz-player table <SUBCOMMAND>\n  bmz-player songs <SUBCOMMAND>\n  bmz-player course <SUBCOMMAND>\n  bmz-player profile <SUBCOMMAND>\n  bmz-player ir <SUBCOMMAND>\n\nOptions:\n  [PATH]                                 Start the chart at PATH (beatoraja-style alias)\n  -p | --practice                        Start boot chart in practice mode (CLI only)\n  --practice-start-ms <MS>               Initial practice section start (milliseconds)\n  --practice-end-ms <MS>                 Initial practice section end (milliseconds)\n  -a                                     Enable autoplay for the boot chart (alias of --autoplay-on-start)\n  -r1 | -r2 | -r3 | -r4                  Start replay slot 1..4 for the boot chart\n  --boot-play-sample                     Start the bundled sample chart on boot\n  --boot-result-sample                   Start directly on a synthetic result screen (debug)\n  --autoplay-on-start                    Enable autoplay for started charts\n  --boot-replay <1..4>                   Start replay slot N for the boot chart\n  --boot-course <ID>                     Start course ID fresh on boot\n  --boot-course-replay <ID>              Replay the latest attempt of course ID\n  --lua-skin-runtime <auto|compat>       Lua function evaluation mode (developer option)\n  --smoke-exit-after-frames <N>          Exit after N rendered frames, clamped to 1 or more\n  --smoke-exit-after-result-frames <N>   Exit after N rendered result frames, clamped to 1 or more\n  --smoke-exit-on-result                 Exit when the app reaches the result screen\n  --smoke-screenshot <PATH>              Save a PNG screenshot on smoke exit (defaults to 3 frames)\n  --renderer <backend>                   wgpu renderer backend (vulkan, metal, dx12, gl, auto)\n  -h, --help                             Print this help\n\nTable subcommands:\n  table add <URL>       Add a difficulty table source and fetch it\n  table list            List all stored difficulty tables\n  table fetch [URL]     Fetch/update configured tables, or a single URL\n\nSongs subcommands:\n  songs add <PATH> [--no-recursive] [--disabled]   Add a song root directory\n  songs list                                        List configured song roots\n  songs load [PATH|NAME]                            Scan song roots (incremental)\n  songs reload [PATH|NAME]                          Force rescan song roots\n\nCourse subcommands:\n  course import <PATH>             Import beatoraja course JSON from a file or directory\n  course list                      List stored courses\n  course history <ID> [--limit N]  Show recent attempts of course ID (default limit 10)\n  course attempt <SCORE_ID>        Show per-chart breakdown of a single attempt\n\nProfile subcommands:\n  profile list                                      List profiles under data/profiles\n  profile current                                   Show the active profile id\n  profile use <ID>                                  Set active_profile in data/config.toml\n  profile create <ID> [--name NAME] [--activate]    Create a new empty profile\n  profile copy <SRC> <ID> [--name NAME] [--activate] Copy an existing profile directory\n\nIR subcommands:\n  ir login --email <EMAIL> [--password PASS] [--base-url URL]\n  ir login --provider bms-ir --id <BMS_IR_ID> [--password GAME_TOKEN]\n  ir logout [--provider KEY]\n  ir status\n  ir ranking <SHA256> [--ln-policy P] [--scope S] [--limit N]\n  ir sync\n  ir upload-local [--dry-run] [--limit N] [--sync] [--all] [--provider KEY]\n  ir download-scores [--dry-run] [--limit N] [--provider KEY]\n  ir attest-submitted [--provider KEY] [--all]\n  ir cleanup-imported [--provider KEY] [--apply]\n  ir cleanup-duplicate <HISTORY_ID> [--provider KEY] --apply\n  ir rivals [add <PLAYER_ID> | remove <PLAYER_ID>]\n  ir device-key [rotate]\n  ir replay <SCORE_ID>\n\nExamples:\n  cargo run -p bmz-player -- /path/to/chart.bms\n  cargo run -p bmz-player -- -a /path/to/chart.bms\n  cargo run -p bmz-player -- -r2 /path/to/chart.bms\n  cargo run -p bmz-player -- --boot-play-sample --smoke-exit-after-frames 3\n  cargo run -p bmz-player -- --boot-result-sample --smoke-exit-after-result-frames 3\n  cargo run -p bmz-player -- --boot-play-sample --smoke-screenshot /tmp/bmz-play.png\n  cargo run -p bmz-player -- --boot-play-sample --boot-replay 1 --smoke-exit-on-result\n  cargo run -p bmz-player -- table add https://example.com/table.html\n  cargo run -p bmz-player -- table list\n  cargo run -p bmz-player -- table fetch https://example.com/table.html\n  cargo run -p bmz-player -- songs add /path/to/bms\n  cargo run -p bmz-player -- songs list\n  cargo run -p bmz-player -- songs load\n  cargo run -p bmz-player -- songs reload my-bms-folder\n  cargo run -p bmz-player -- course import /path/to/course.json\n  cargo run -p bmz-player -- course list\n  cargo run -p bmz-player -- profile create alt --name Alt --activate\n  cargo run -p bmz-player -- ir upload-local --dry-run\n  cargo run -p bmz-player -- ir upload-local --limit 20 --sync\n  cargo run -p bmz-player -- ir upload-local --all\n  cargo run -p bmz-player -- ir attest-submitted --all\n  cargo run -p bmz-player -- ir cleanup-imported\n  cargo run -p bmz-player -- ir cleanup-imported --apply\n  cargo run -p bmz-player -- ir download-scores --dry-run"
        .replace("\n\nOptions:", "\n  bmz-player [--profile ID] export video PATH -o OUTPUT.mp4\n\nExport video options:\n  --resolution WIDTHxHEIGHT   Default: 1920x1080 (even dimensions)\n  --fps N[/D]                 Default: 60; rational FPS supported\n  --replay-slot 1..4           Default: autoplay\n  --seed N                    Reproducible autoplay arrangement and BMS branches\n  --ffmpeg PATH               FFmpeg with libx264 and AAC (default: ffmpeg)\n  --overwrite                 Replace completed output and metadata\n  Interval: Play skin entry through exit, including fades\n\nOptions:")
        .replace(
            "  bmz-player table <SUBCOMMAND>\n  bmz-player songs <SUBCOMMAND>\n  bmz-player course <SUBCOMMAND>\n  bmz-player profile <SUBCOMMAND>\n  bmz-player ir <SUBCOMMAND>\n",
            "  bmz-player [--profile ID] table <SUBCOMMAND>\n  bmz-player [--profile ID] songs <SUBCOMMAND>\n  bmz-player [--profile ID] course <SUBCOMMAND>\n  bmz-player [--profile ID] replay <SUBCOMMAND>\n  bmz-player [--profile ID] profile <SUBCOMMAND>\n  bmz-player [--profile ID] ir <SUBCOMMAND>\n",
        )
        .replace(
            "  [PATH]                                 Start the chart at PATH (beatoraja-style alias)\n",
            "  [PATH]                                 Start the chart at PATH (beatoraja-style alias)\n  -P | --viewer-play                     Play PATH as an external viewer\n  -B | --battle                          Start 5K/7K with battle skin (combines with -a/-P)\n  --profile <ID>                         Use profile ID for this CLI invocation (global option)\n  -N<N> | -N <N> | --start-measure <N>   Viewer only: start at zero-based measure/bar N\n  -S | --viewer-stop                     Stop playback and leave the viewer waiting\n  --skip-decide                          Enter Play without the Decide screen\n  --skip-result                          Exit after Play without the Result screen\n",
        )
        .replace(
            "  cargo run -p bmz-player -- -a /path/to/chart.bms\n",
            "  cargo run -p bmz-player -- -a /path/to/chart.bms\n  cargo run -p bmz-player -- -P -N12 /path/to/chart.bms\n  cargo run -p bmz-player -- -P -B --profile alt /path/to/chart.bms\n  cargo run -p bmz-player -- -S\n",
        )
        .replace(
            "  --smoke-exit-after-frames <N>          Exit after N rendered frames, clamped to 1 or more\n",
            "  --smoke-exit-after-frames <N>          Exit after N rendered frames, clamped to 1 or more\n  --smoke-exit-after-play-frames <N>     Exit after N rendered Play-scene frames, clamped to 1 or more\n",
        )
        .replace(
            "  songs load [PATH|NAME]                            Scan song roots (incremental)\n  songs reload [PATH|NAME]                          Force rescan song roots\n",
            "  songs load [PATH|NAME] [--everything|--no-everything]    Scan song roots (incremental)\n  songs reload [PATH|NAME] [--everything|--no-everything]  Force rescan song roots\n",
        )
        .replace(
            "  cargo run -p bmz-player -- songs load\n",
            "  cargo run -p bmz-player -- songs load --everything\n",
        )
        .replace(
            "  bmz-player course <SUBCOMMAND>\n",
            "  bmz-player course <SUBCOMMAND>\n  bmz-player replay <SUBCOMMAND>\n",
        )
        .replace(
            "  course attempt <SCORE_ID>        Show per-chart breakdown of a single attempt\n\nProfile subcommands:",
            "  course attempt <SCORE_ID>        Show per-chart breakdown of a single attempt\n\nReplay subcommands:\n  replay import <PATH> [--overwrite] [--controller]\n      Import beatoraja .brd files from a player/replay directory or one file\n\nProfile subcommands:",
        )
        .replace(
            "  cargo run -p bmz-player -- course list\n",
            "  cargo run -p bmz-player -- course list\n  cargo run -p bmz-player -- replay import /path/to/player\n",
        )
}

pub(super) fn parse_lua_skin_runtime_mode(value: &str) -> Result<bmz_skin::LuaSkinRuntimeMode> {
    match value.trim().to_ascii_lowercase().as_str() {
        "auto" => Ok(bmz_skin::LuaSkinRuntimeMode::Auto),
        "compat" => Ok(bmz_skin::LuaSkinRuntimeMode::Compat),
        other => bail!("unknown Lua skin runtime mode: {other}. Valid options: auto, compat"),
    }
}

pub(super) fn parse_practice_ms(value: &str, arg: &str) -> Result<u32> {
    let value = value.trim();
    if value.is_empty() {
        bail!("{arg} requires milliseconds");
    }
    let ms: u64 =
        value.parse().with_context(|| format!("invalid milliseconds for {arg}: {value}"))?;
    u32::try_from(ms).with_context(|| format!("{arg} value out of range: {value}"))
}

pub(super) fn parse_start_measure(value: &str) -> Result<u32> {
    let value = value.strip_prefix('=').unwrap_or(value).trim();
    if value.is_empty() {
        bail!("{START_MEASURE_ARG} requires a measure number");
    }
    value.parse().with_context(|| format!("invalid start measure: {value}"))
}

pub(super) fn parse_smoke_exit_after_frames_value(value: &str) -> Result<u32> {
    parse_smoke_frame_count_value(value, SMOKE_EXIT_AFTER_FRAMES_ARG)
}

pub(super) fn parse_smoke_exit_after_play_frames_value(value: &str) -> Result<u32> {
    parse_smoke_frame_count_value(value, SMOKE_EXIT_AFTER_PLAY_FRAMES_ARG)
}

pub(super) fn parse_smoke_exit_after_result_frames_value(value: &str) -> Result<u32> {
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

pub(super) fn parse_smoke_screenshot_path(value: &str) -> Result<String> {
    let value = value.trim();
    if value.is_empty() {
        bail!("{SMOKE_SCREENSHOT_ARG} requires an output path");
    }
    Ok(value.to_string())
}

pub(super) fn parse_boot_course_replay_id(value: &str) -> Result<i64> {
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

pub(super) fn parse_course_history_id(value: &str) -> Result<i64> {
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

pub(super) fn parse_course_history_limit(flags: &[String]) -> Result<u32> {
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

pub(super) fn parse_course_attempt_id(value: &str) -> Result<i64> {
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

pub(super) fn parse_boot_course_id(value: &str) -> Result<i64> {
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

pub(super) fn parse_boot_replay_slot(value: &str) -> Result<u8> {
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
pub(super) fn parse_beatoraja_replay_flag(arg: &str) -> Option<u8> {
    let rest = arg.strip_prefix('-')?;
    match rest {
        "r1" => Some(0),
        "r2" => Some(1),
        "r3" => Some(2),
        "r4" => Some(3),
        _ => None,
    }
}

pub(super) fn parse_renderer_backend(value: &str) -> Result<RendererBackend> {
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
use super::*;

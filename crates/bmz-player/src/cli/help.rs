pub const HELP_DEV_ARG: &str = "--help-dev";

pub fn args_request_help<I, S>(args: I) -> bool
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    args.into_iter().any(|arg| matches!(arg.as_ref(), "--help" | "-h" | HELP_DEV_ARG))
}

/// `--help-dev`が含まれていれば開発者向けヘルプを返し、それ以外は通常のヘルプを返す。
pub fn help_text_for_args<I, S>(args: I) -> &'static str
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    if args.into_iter().any(|arg| arg.as_ref() == HELP_DEV_ARG) {
        dev_help_text()
    } else {
        app_help_text()
    }
}

pub fn app_help_text() -> &'static str {
    "\
bmz-player

Usage:
  bmz-player [OPTIONS] [PATH]
  bmz-player [--profile ID] <COMMAND> <SUBCOMMAND> [ARGS]

Play:
  [PATH]                       Start the chart at PATH (archives: ARCHIVE!/ENTRY)
  -a, --autoplay-on-start      Autoplay the started chart
  -r1 | -r2 | -r3 | -r4        Play replay slot 1..4 of the chart
  --boot-replay <1..4>         Same as -r1..-r4
  --boot-replay-file <PATH>    Play a replay file (see: ir replay)
  -p, --practice               Start the chart in practice mode
  --practice-start-ms <MS>     Initial practice section start
  --practice-end-ms <MS>       Initial practice section end
  -B, --battle                 Start 5K/7K with the battle skin (with -a or -P)
  --boot-course <ID>           Start course ID
  --boot-course-replay <ID>    Replay the latest attempt of course ID
  --skip-decide                Enter Play without the Decide screen
  --skip-result                Exit after Play without the Result screen
  --profile <ID>               Use profile ID for this invocation (global option)
  --renderer <BACKEND>         vulkan, metal, dx12, gl, auto
  -h, --help                   Print this help
  --help-dev                   Print developer and verification options

Viewer (uBMplay-compatible arguments for BMS editors):
  -P, --viewer-play            Play PATH in the resident viewer (autoplay, no Decide)
  -N<N> | -N <N> | --start-measure <N>
                               Start at zero-based measure N
  -S, --viewer-stop            Stop playback and leave the viewer waiting

Launch overrides (never saved):
  --window-mode windowed|borderless|exclusive
  --window-size WIDTHxHEIGHT   Physical pixels, windowed only
  --monitor ID|primary         See: monitors list
  --arrange MODE               off, mirror, random, r-random, s-random, spiral, h-random,
                               all-scratch, random-ex, s-random-ex, f-random, mf-random
  --arrange-2p MODE            P2 arrangement
  --double-option off|flip|battle|battle-auto-scratch
  --gauge assist-easy|easy|normal|hard|ex-hard|hazard
  --gas off|continue|hard-to-groove|best-clear|select-to-under
  --gas-bottom assist-easy|easy|normal
  --hispeed 0.01..20           Fixed Classic multiplier for this chart
  --hs-fix off|start-bpm|min-bpm|max-bpm|main-bpm
  --auto-scratch on|off        Scratch autoplay; on disables score saving
  --bga on|auto|off
  --guide-se on|off
  --seed N                     Fixed arrangement and BMS branch seed
  --print-effective-options    Print settings without starting playback
  Without a chart, play overrides are ignored. Returning to Select discards them.
  Window overrides last for this process; export rejects them.

Commands:
  table add <URL>                                Add a difficulty table and fetch it
  table list                                     List stored difficulty tables
  table fetch [URL]                              Fetch all tables, or a single URL

  songs add <PATH> [--no-recursive] [--disabled] Add a song root directory
  songs list                                     List song roots
  songs load [PATH|NAME] [--everything|--no-everything]
                                                 Scan song roots (incremental)
  songs reload [PATH|NAME] [--everything|--no-everything]
                                                 Force rescan song roots

  course import <PATH>                           Import beatoraja course JSON (file or directory)
  course list                                    List stored courses
  course history <ID> [--limit N]                Show recent attempts (default limit 10)
  course attempt <SCORE_ID>                      Show per-chart breakdown of an attempt

  replay import <PATH> [--overwrite] [--controller]
                                                 Import beatoraja .brd files (directory or file)

  profile list                                   List profiles under data/profiles
  profile current                                Show the active profile id
  profile use <ID>                               Set active_profile in data/config.toml
  profile create <ID> [--name NAME] [--activate] Create a new empty profile
  profile copy <SRC> <ID> [--name NAME] [--activate]
                                                 Copy an existing profile

  ir login --email <EMAIL> [--password PASS] [--base-url URL]
  ir login --provider bms-ir --id <BMS_IR_ID> [--password GAME_TOKEN]
  ir logout [--provider KEY]
  ir status
  ir ranking <SHA256> [--ln-policy P] [--scope S] [--limit N]
  ir sync
  ir upload-local [--dry-run] [--limit N] [--sync] [--all] [--provider KEY]
  ir download-scores [--dry-run] [--limit N] [--provider KEY]
  ir attest-submitted [--provider KEY] [--all]
  ir cleanup-imported [--provider KEY] [--apply]
  ir cleanup-duplicate <HISTORY_ID> [--provider KEY] --apply
  ir rivals [add <PLAYER_ID> | remove <PLAYER_ID>]
  ir device-key [rotate]
  ir replay <SCORE_ID>

  monitors list                                  List monitor IDs for --monitor

  export video <PATH> -o <OUTPUT.mp4> [OPTIONS]  Render Play (skin entry to exit) to a video
    --resolution WIDTHxHEIGHT  Default: 1920x1080 (even dimensions)
    --fps N[/D]                Default: 60; rational FPS supported
    --replay-slot 1..4         Default: autoplay
    --seed N                   Reproducible autoplay arrangement and BMS branches
    --ffmpeg PATH              FFmpeg with libx264 and AAC (default: ffmpeg)
    --overwrite                Replace completed output and metadata

Examples:
  bmz-player /path/to/chart.bms
  bmz-player -a /path/to/chart.bms
  bmz-player -r2 /path/to/chart.bms
  bmz-player --gauge hard --arrange mirror /path/to/chart.bms
  bmz-player -P -N12 /path/to/chart.bms
  bmz-player -P -B --profile alt /path/to/chart.bms
  bmz-player -S
  bmz-player songs add /path/to/bms
  bmz-player songs load --everything
  bmz-player table add https://example.com/table.html
  bmz-player course import /path/to/course.json
  bmz-player replay import /path/to/player
  bmz-player profile create alt --name Alt --activate
  bmz-player ir upload-local --dry-run
  bmz-player export video /path/to/chart.bms -o out.mp4"
}

pub fn dev_help_text() -> &'static str {
    "\
bmz-player developer options

These options are for development, smoke tests, and latency verification.
See --help for regular options.

Boot:
  --boot-play-sample                     Start the bundled sample chart
  --boot-result-sample                   Start on a synthetic result screen
  --lua-skin-runtime <auto|compat>       Lua function evaluation mode

Smoke tests:
  --smoke-exit-after-frames <N>          Exit after N rendered frames (min 1)
  --smoke-exit-after-play-frames <N>     Exit after N rendered Play-scene frames (min 1)
  --smoke-exit-after-result-frames <N>   Exit after N rendered result frames (min 1)
  --smoke-exit-on-result                 Exit when the app reaches the result screen
  --smoke-screenshot <PATH>              Save a PNG on smoke exit (defaults to 3 frames)

Latency verification:
  audio-probe HOST BUFFER RATE SECONDS   Silent stream only; no config/DB/window
                                         HOST: auto|pipewire|pulse|alsa
                                         BUFFER: auto|frames, SECONDS: 3..60
  --latency-stall-test                   Stall the window thread 100ms every 2s
  --latency-event-loop-probe             Synthetic wake-to-dispatch probe
  --latency-legacy-wayland-present       Compare without Wayland frame callbacks
  BMZ_LATENCY_DIAGNOSTICS=1              Log latency distributions (JSON summaries)
  The --latency-* options disable score and IR saving.

Examples:
  cargo run -p bmz-player -- --boot-play-sample --smoke-exit-after-frames 3
  cargo run -p bmz-player -- --boot-result-sample --smoke-exit-after-result-frames 3
  cargo run -p bmz-player -- --boot-play-sample --smoke-screenshot /tmp/bmz-play.png
  cargo run -p bmz-player -- --boot-play-sample --boot-replay 1 --smoke-exit-on-result"
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

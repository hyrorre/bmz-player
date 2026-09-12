#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

use std::process::ExitCode;
use std::time::Duration;

use bmz_player::cli::Command;
use bmz_player::logging::{
    StartupLoggingConfig, initialize_logging, install_panic_hook, load_startup_logging_config,
    log_session_end, log_session_start,
};

#[tokio::main]
async fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if bmz_player::cli::args_request_help(&args) {
        bmz_player::stdio::stdout_line(format_args!("{}", bmz_player::cli::app_help_text()));
        return ExitCode::SUCCESS;
    }

    // Parse errors and help remain side-effect free: paths/config/log directories are not created.
    let invocation = match bmz_player::cli::parse_cli_command(args) {
        Ok(invocation) => invocation,
        Err(error) => {
            bmz_player::stdio::stderr_line(format_args!("Error: {error:#}"));
            return ExitCode::FAILURE;
        }
    };
    for warning in &invocation.warnings {
        bmz_player::stdio::stderr_line(format_args!("Warning: {warning}"));
    }
    let command = invocation.command;
    let profile_id = invocation.profile_id;
    if let Command::Run(options) = &command {
        if options.viewer_stop {
            return match bmz_player::viewer_ipc::request_stop() {
                Ok(_) => ExitCode::SUCCESS,
                Err(error) => {
                    bmz_player::stdio::stderr_line(format_args!("Error: {error:#}"));
                    ExitCode::FAILURE
                }
            };
        }
        if options.viewer_play {
            let path = options.boot_play_path.as_deref().expect("viewer path validated by CLI");
            let path = match std::path::Path::new(path).canonicalize() {
                Ok(path) => path,
                Err(error) => {
                    bmz_player::stdio::stderr_line(format_args!(
                        "Error: could not resolve viewer chart {path}: {error}"
                    ));
                    return ExitCode::FAILURE;
                }
            };
            if let Some(profile_id) = profile_id.as_deref() {
                let app_paths = match bmz_player::paths::resolve_app_paths() {
                    Ok(paths) => paths,
                    Err(error) => {
                        bmz_player::stdio::stderr_line(format_args!("Error: {error:#}"));
                        return ExitCode::FAILURE;
                    }
                };
                let profile_paths =
                    match bmz_player::paths::resolve_profile_paths(&app_paths, profile_id) {
                        Ok(paths) => paths,
                        Err(error) => {
                            bmz_player::stdio::stderr_line(format_args!("Error: {error:#}"));
                            return ExitCode::FAILURE;
                        }
                    };
                if !profile_paths.profile_toml.is_file() {
                    bmz_player::stdio::stderr_line(format_args!(
                        "Error: profile not found: {profile_id}"
                    ));
                    return ExitCode::FAILURE;
                }
            }
            if options.requires_fresh_viewer_process(profile_id.is_some()) {
                match bmz_player::viewer_ipc::request_quit() {
                    Ok(true) => {
                        // 明示的なone-shot起動は常駐プロセスへ転送できないため、旧viewerが
                        // 名前付きpipeを解放する短い間だけ待ってから新規起動する。
                        for _ in 0..100 {
                            std::thread::sleep(Duration::from_millis(10));
                            if matches!(bmz_player::viewer_ipc::request_quit(), Ok(false)) {
                                break;
                            }
                        }
                    }
                    Ok(false) => {}
                    Err(error) => {
                        bmz_player::stdio::stderr_line(format_args!(
                            "Error: could not close the active viewer: {error:#}"
                        ));
                        return ExitCode::FAILURE;
                    }
                }
            } else {
                match bmz_player::viewer_ipc::request_play(
                    &path,
                    options.start_measure.unwrap_or(0),
                    options.battle_on_start,
                ) {
                    Ok(true) => return ExitCode::SUCCESS,
                    Ok(false) => {}
                    Err(error) => {
                        bmz_player::stdio::stderr_line(format_args!(
                            "Error: could not send play command to the active viewer: {error:#}"
                        ));
                        return ExitCode::FAILURE;
                    }
                }
            }
        }
    }
    let app_paths = match bmz_player::paths::resolve_app_paths() {
        Ok(paths) => paths,
        Err(error) => {
            bmz_player::stdio::stderr_line(format_args!("Error: {error:#}"));
            return ExitCode::FAILURE;
        }
    };
    let (startup_logging, startup_logging_error) =
        match load_startup_logging_config(&app_paths.config_toml) {
            Ok(config) => (config, None),
            Err(error) => {
                bmz_player::stdio::stderr_line(format_args!(
                    "Warning: could not read startup logging settings; using defaults: {error:#}"
                ));
                (StartupLoggingConfig::default(), Some(format!("{error:#}")))
            }
        };
    let rust_log = std::env::var_os("RUST_LOG").map(|value| {
        value
            .into_string()
            // EnvFilter cannot interpret non-Unicode process environment values.
            .unwrap_or_else(|_| "[non-unicode RUST_LOG]".to_string())
    });
    let logging = match initialize_logging(&app_paths, startup_logging, rust_log.as_deref()) {
        Ok(logging) => logging,
        Err(error) => {
            bmz_player::stdio::stderr_line(format_args!("Error: {error:#}"));
            return ExitCode::FAILURE;
        }
    };

    install_panic_hook();
    log_session_start(&logging);
    if let Some(error) = startup_logging_error {
        tracing::warn!(%error, "startup logging settings could not be read; using defaults");
    }

    let result = match command {
        Command::Export(options) => {
            bmz_player::video_export::run(options, &app_paths, profile_id.as_deref())
        }
        Command::Run(options) => {
            bmz_player::app::run_with_options_log_buffer_paths_and_profile(
                options,
                logging.log_buffer.clone(),
                app_paths,
                profile_id.as_deref(),
            )
            .await
        }
        Command::Table(cmd) => {
            bmz_player::table_cmd::run_table_command_with_paths(cmd, &app_paths).await
        }
        Command::Songs(cmd) => bmz_player::songs_cmd::run_songs_command_with_paths(cmd, &app_paths),
        Command::Course(cmd) => bmz_player::course_cmd::run_course_command_with_paths_and_profile(
            cmd,
            &app_paths,
            profile_id.as_deref(),
        ),
        Command::Replay(cmd) => bmz_player::replay_cmd::run_replay_command_with_paths_and_profile(
            cmd,
            &app_paths,
            profile_id.as_deref(),
        ),
        Command::Ir(cmd) => {
            bmz_player::ir_cmd::run_ir_command_with_paths_and_profile(
                cmd,
                &app_paths,
                profile_id.as_deref(),
            )
            .await
        }
        Command::Profile(cmd) => {
            bmz_player::profile_cmd::run_profile_command_with_paths(cmd, &app_paths)
        }
    };

    match result {
        Ok(()) => {
            log_session_end(&logging, true);
            ExitCode::SUCCESS
        }
        Err(error) => {
            tracing::error!(
                error = %format!("{error:#}"),
                status = "error",
                "BMZ Player command failed"
            );
            bmz_player::stdio::stderr_line(format_args!("Error: {error:#}"));
            log_session_end(&logging, false);
            ExitCode::FAILURE
        }
    }
}

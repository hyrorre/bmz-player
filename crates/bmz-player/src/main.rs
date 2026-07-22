#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

use anyhow::Result;
use bmz_player::cli::Command;
use bmz_player::logging::{LogBuffer, LogBufferLayer};
use tracing_subscriber::{EnvFilter, layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() {
    if let Err(error) = run().await {
        bmz_player::stdio::stderr_line(format_args!("Error: {error:#}"));
        std::process::exit(1);
    }
}

async fn run() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if bmz_player::cli::args_request_help(&args) {
        bmz_player::stdio::stdout_line(format_args!("{}", bmz_player::cli::app_help_text()));
        return Ok(());
    }

    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    let log_buffer = LogBuffer::default();
    tracing_subscriber::registry()
        .with(filter)
        .with(tracing_subscriber::fmt::layer().with_writer(bmz_player::stdio::SafeStderr))
        .with(LogBufferLayer::new(log_buffer.clone()))
        .init();

    match bmz_player::cli::parse_command(args)? {
        Command::Run(options) => {
            bmz_player::app::run_with_options_and_log_buffer(options, log_buffer).await
        }
        Command::Table(cmd) => bmz_player::table_cmd::run_table_command(cmd).await,
        Command::Songs(cmd) => bmz_player::songs_cmd::run_songs_command(cmd),
        Command::Course(cmd) => bmz_player::course_cmd::run_course_command(cmd),
        Command::Ir(cmd) => bmz_player::ir_cmd::run_ir_command(cmd).await,
        Command::Profile(cmd) => bmz_player::profile_cmd::run_profile_command(cmd),
    }
}

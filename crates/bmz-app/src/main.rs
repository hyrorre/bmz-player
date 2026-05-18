use anyhow::Result;
use bmz_app::cli::Command;

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if bmz_app::cli::args_request_help(&args) {
        println!("{}", bmz_app::cli::app_help_text());
        return Ok(());
    }

    tracing_subscriber::fmt().with_max_level(tracing::Level::INFO).init();

    match bmz_app::cli::parse_command(args)? {
        Command::Run(options) => bmz_app::app::run_with_options(options).await,
        Command::Table(cmd) => bmz_app::table_cmd::run_table_command(cmd).await,
    }
}

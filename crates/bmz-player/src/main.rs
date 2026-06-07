use anyhow::Result;
use bmz_player::cli::Command;

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if bmz_player::cli::args_request_help(&args) {
        println!("{}", bmz_player::cli::app_help_text());
        return Ok(());
    }

    tracing_subscriber::fmt().with_max_level(tracing::Level::INFO).init();

    match bmz_player::cli::parse_command(args)? {
        Command::Run(options) => bmz_player::app::run_with_options(options).await,
        Command::Table(cmd) => bmz_player::table_cmd::run_table_command(cmd).await,
        Command::Songs(cmd) => bmz_player::songs_cmd::run_songs_command(cmd),
        Command::Course(cmd) => bmz_player::course_cmd::run_course_command(cmd),
    }
}

use anyhow::Result;

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if bmz_app::app::args_request_help(&args) {
        println!("{}", bmz_app::app::app_help_text());
        return Ok(());
    }

    tracing_subscriber::fmt().with_max_level(tracing::Level::INFO).init();
    let options = bmz_app::app::AppOptions::parse_args(&args)?;
    bmz_app::app::run_with_options(options)
}

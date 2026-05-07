use anyhow::Result;

fn main() -> Result<()> {
    tracing_subscriber::fmt().with_max_level(tracing::Level::INFO).init();
    let options = bmz_app::app::AppOptions::parse_args(std::env::args().skip(1))?;
    bmz_app::app::run_with_options(options)
}

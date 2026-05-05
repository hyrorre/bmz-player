use anyhow::Result;

fn main() -> Result<()> {
    tracing_subscriber::fmt().with_max_level(tracing::Level::INFO).init();
    bmz_app::app::run()
}

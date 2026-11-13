//! Bokutachi score submission, dan updates and chart rankings.
mod client;
mod convert;
pub(crate) mod dan;
mod ranking;
pub(crate) mod sync;

pub use client::BokutachiClient;
pub(crate) use convert::context;
pub use convert::{CourseStageContext, Game, PlayContext, ScoringScope};
pub(crate) use ranking::RANKING_LIMIT;

pub const PROVIDER: &str = "bokutachi";
pub const DEFAULT_BASE_URL: &str = "https://boku.tachi.ac";

pub fn is_provider(provider: &str) -> bool {
    provider.trim().eq_ignore_ascii_case(PROVIDER)
}

pub fn is_config(config: &crate::config::profile_config::IrProviderConfig) -> bool {
    is_provider(&config.provider) || is_provider(&config.provider_key)
}

pub fn unsupported() -> anyhow::Error {
    anyhow::anyhow!("Bokutachi: this operation is not supported")
}

#[cfg(test)]
mod tests;

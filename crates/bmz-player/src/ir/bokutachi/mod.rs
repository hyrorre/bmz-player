//! Bokutachi Direct Manual: immutable play context, provider-specific wire data,
//! and a durable, one-request-per-claim import state machine.
mod client;
mod convert;
pub(crate) mod sync;

pub use client::BokutachiClient;
pub(crate) use convert::context;
pub use convert::{Game, PlayContext, ScoringScope};

pub const PROVIDER: &str = "bokutachi";
pub const DEFAULT_BASE_URL: &str = "https://boku.tachi.ac";

pub fn is_provider(provider: &str) -> bool {
    provider.trim().eq_ignore_ascii_case(PROVIDER)
}

pub fn is_config(config: &crate::config::profile_config::IrProviderConfig) -> bool {
    is_provider(&config.provider) || is_provider(&config.provider_key)
}

pub fn unsupported() -> anyhow::Error {
    anyhow::anyhow!("Bokutachi supports new single scores only; this operation is not supported")
}

#[cfg(test)]
mod tests;

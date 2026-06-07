use anyhow::Result;

use super::types::{IrRankingScope, IrScoreSubmission, IrSubmitOptions, IrSubmitResponse};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IrAuthStatus {
    SignedOut,
    SignedIn { account_id: String, display_name: String },
}

#[derive(Debug, Clone)]
pub struct IrRankingQuery {
    pub chart_sha256: [u8; 32],
    pub gauge: String,
    pub ln_policy: crate::ln_policy::LnScorePolicy,
    pub scope: IrRankingScope,
    pub limit: u32,
    pub offset: u32,
}

pub trait IrProvider: Send + Sync {
    fn provider_id(&self) -> &'static str;
    fn display_name(&self) -> &'static str;
    fn auth_status(&self) -> Result<IrAuthStatus>;
    fn submit_score(
        &self,
        request: &IrScoreSubmission,
        options: &IrSubmitOptions,
    ) -> Result<IrSubmitResponse>;
}

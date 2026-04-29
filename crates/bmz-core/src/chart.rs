use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ChartIdentity {
    pub file_md5: [u8; 16],
    pub file_sha256: [u8; 32],
}

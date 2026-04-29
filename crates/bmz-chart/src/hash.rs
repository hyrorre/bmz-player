use bmz_core::chart::ChartIdentity;
use sha2::{Digest, Sha256};

pub fn compute_chart_identity(bytes: &[u8]) -> ChartIdentity {
    let md5_digest = md5::compute(bytes);
    let sha256_digest = Sha256::digest(bytes);

    ChartIdentity { file_md5: md5_digest.0, file_sha256: sha256_digest.into() }
}

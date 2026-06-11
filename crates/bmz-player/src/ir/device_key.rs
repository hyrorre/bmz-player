//! IR tamper evidence 用の Ed25519 device key。
//!
//! docs/ir.md §10 準拠。完全なチート防止ではなく、「提出内容が後から
//! 改変されていない」ことを示すための仕組み。秘密鍵はプロファイル配下の
//! JSON (unix では 0600) に保存し、公開鍵はサーバーの `device_keys` に登録する。

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use ed25519_dalek::{Signer, SigningKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::storage::common::hash_to_hex;

pub const EVIDENCE_SCHEMA: &str = "bmz-score-evidence-v1";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StoredDeviceKey {
    pub provider: String,
    /// Ed25519 秘密鍵 (32 bytes hex)。
    pub private_key: String,
    /// Ed25519 公開鍵 (32 bytes hex)。サーバー登録にも使う。
    pub public_key: String,
    /// サーバー登録済みの device key id。未登録なら None。
    #[serde(default)]
    pub key_id: Option<String>,
}

pub fn device_key_path(profile_root: &Path, provider: &str) -> PathBuf {
    profile_root.join("ir").join(format!("{provider}.device_key.json"))
}

fn device_key_slot<'a>(
    profile_root: &'a Path,
    provider: &'a str,
) -> crate::ir::secret_store::SecretSlot<'a> {
    crate::ir::secret_store::SecretSlot::new(
        "ir-device-key",
        provider,
        profile_root,
        device_key_path(profile_root, provider),
        crate::ir::secret_store::store_mode(),
    )
}

/// device key を読み込む。無ければ生成して保存する。
pub fn load_or_create_device_key(profile_root: &Path, provider: &str) -> Result<StoredDeviceKey> {
    if let Some(raw) = device_key_slot(profile_root, provider).load()? {
        return serde_json::from_str(&raw).context("failed to parse stored IR device key");
    }

    let mut secret = [0u8; 32];
    getrandom::getrandom(&mut secret)
        .map_err(|error| anyhow::anyhow!("failed to generate device key entropy: {error}"))?;
    let signing = SigningKey::from_bytes(&secret);
    let key = StoredDeviceKey {
        provider: provider.to_string(),
        private_key: hash_to_hex(&secret),
        public_key: hash_to_hex(signing.verifying_key().as_bytes()),
        key_id: None,
    };
    save_device_key(profile_root, &key)?;
    Ok(key)
}

/// device key を削除する (ローテーション用)。
pub fn delete_device_key(profile_root: &Path, provider: &str) -> Result<bool> {
    device_key_slot(profile_root, provider).delete()
}

pub fn save_device_key(profile_root: &Path, key: &StoredDeviceKey) -> Result<()> {
    let raw = serde_json::to_string_pretty(key)?;
    device_key_slot(profile_root, &key.provider).save(&raw)
}

/// submission payload の canonical hash と署名から evidence map を作る。
///
/// canonical form は「`evidence` を除いた payload を serde_json の
/// キー昇順 (BTreeMap) で compact 出力した JSON」。サーバー側も同じ
/// 正規化で再計算して検証する。
pub fn build_evidence(
    key: &StoredDeviceKey,
    payload: &super::types::IrScoreSubmission,
) -> Result<std::collections::BTreeMap<String, serde_json::Value>> {
    let canonical = canonical_submission_json(payload)?;
    let canonical_hash = Sha256::digest(canonical.as_bytes());
    let secret: [u8; 32] = decode_hex_32(&key.private_key)?;
    let signing = SigningKey::from_bytes(&secret);
    let signature = signing.sign(&canonical_hash);

    let mut evidence = std::collections::BTreeMap::new();
    evidence.insert("schema".to_string(), serde_json::json!(EVIDENCE_SCHEMA));
    evidence.insert("canonical_hash".to_string(), serde_json::json!(hash_to_hex(&canonical_hash)));
    evidence.insert(
        "client_signature".to_string(),
        serde_json::json!(base64_url(&signature.to_bytes())),
    );
    if let Some(key_id) = &key.key_id {
        evidence.insert("public_key_id".to_string(), serde_json::json!(key_id));
    }
    Ok(evidence)
}

/// `evidence` を除いた payload の canonical JSON。
/// serde_json は preserve_order 無効ビルドなので object キーは昇順になる。
pub fn canonical_submission_json(payload: &super::types::IrScoreSubmission) -> Result<String> {
    let mut value = serde_json::to_value(payload)?;
    if let Some(object) = value.as_object_mut() {
        object.remove("evidence");
    }
    Ok(serde_json::to_string(&value)?)
}

fn decode_hex_32(hex: &str) -> Result<[u8; 32]> {
    let bytes = (0..hex.len())
        .step_by(2)
        .map(|index| u8::from_str_radix(&hex[index..index + 2], 16))
        .collect::<std::result::Result<Vec<_>, _>>()
        .context("invalid device key hex")?;
    bytes.try_into().map_err(|_| anyhow::anyhow!("device key must be 32 bytes"))
}

fn base64_url(bytes: &[u8]) -> String {
    use base64::Engine as _;
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

#[cfg(test)]
mod tests {
    use ed25519_dalek::{Verifier, VerifyingKey};

    use super::*;

    fn temp_root(label: &str) -> PathBuf {
        let stamp =
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let path = std::env::temp_dir()
            .join(format!("bmz-ir-devkey-{label}-{}-{stamp}", std::process::id()));
        std::fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn device_key_is_created_once_and_reloaded() {
        let root = temp_root("create");
        let first = load_or_create_device_key(&root, "bmz-official").unwrap();
        let second = load_or_create_device_key(&root, "bmz-official").unwrap();
        assert_eq!(first, second);
        assert_eq!(first.private_key.len(), 64);
        assert_eq!(first.public_key.len(), 64);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn evidence_signature_verifies_against_public_key() {
        let root = temp_root("sign");
        let key = load_or_create_device_key(&root, "bmz-official").unwrap();
        let payload = sample_payload();

        let evidence = build_evidence(&key, &payload).unwrap();
        let canonical = canonical_submission_json(&payload).unwrap();
        let hash = Sha256::digest(canonical.as_bytes());
        assert_eq!(evidence.get("canonical_hash").unwrap().as_str().unwrap(), hash_to_hex(&hash));

        use base64::Engine as _;
        let signature_bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(evidence.get("client_signature").unwrap().as_str().unwrap())
            .unwrap();
        let public = VerifyingKey::from_bytes(&decode_hex_32(&key.public_key).unwrap()).unwrap();
        let signature =
            ed25519_dalek::Signature::from_bytes(signature_bytes.as_slice().try_into().unwrap());
        public.verify(&hash, &signature).unwrap();
        std::fs::remove_dir_all(root).unwrap();
    }

    fn sample_payload() -> crate::ir::types::IrScoreSubmission {
        serde_json::from_value(serde_json::json!({
            "client": {"name": "BMZ", "version": "0.1.0", "platform": "macos"},
            "chart": {
                "sha256": "ab".repeat(32),
                "ln_profile": {
                    "has_undefined_ln": false,
                    "has_defined_ln": false,
                    "has_defined_cn": false,
                    "has_defined_hcn": false
                },
                "mode": "keys_7",
                "notes": {"total": 1, "ln": 0, "cn": 0, "hcn": 0, "mine": 0},
                "features": {
                    "random": false, "stop": false, "ln": false,
                    "cn": false, "hcn": false, "mine": false
                }
            },
            "rule": {
                "play_mode": "single",
                "key_mode": "keys_7",
                "gauge": "Normal",
                "ln_policy": "ForceLn",
                "effective_ln_mode": "ln",
                "judge_algorithm": "bmz_v1",
                "scoring": "bms_ex_score_v1"
            },
            "result": {
                "clear": "Normal",
                "played_at": 100,
                "judges": {
                    "fast": {"pgreat": 1, "great": 0, "good": 0, "bad": 0, "poor": 0, "empty_poor": 0},
                    "slow": {"pgreat": 0, "great": 0, "good": 0, "bad": 0, "poor": 0, "empty_poor": 0}
                },
                "ex_score": 2,
                "max_combo": 1,
                "notes": 1,
                "pass_notes": 1,
                "min_bp": 0,
                "min_cb": 0
            },
            "play_options": {"device_type": "keyboard"},
            "idempotency_key": "test-1"
        }))
        .unwrap()
    }
}

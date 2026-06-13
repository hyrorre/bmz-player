//! IR tamper evidence 用の Ed25519 device key。
//!
//! docs/ir.md §10 準拠。完全なチート防止ではなく、「提出内容が後から
//! 改変されていない」ことを示すための仕組み。秘密鍵はプロファイル配下の
//! JSON (unix では 0600) に保存し、公開鍵はサーバーの `device_keys` に登録する。

use std::cmp::Ordering;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use ed25519_dalek::{Signer, SigningKey};
use serde::{Deserialize, Serialize};
use serde_json::Value;
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

/// ローカル device key をサーバーで使える状態にする。
///
/// - 未生成なら生成する。
/// - 未登録なら公開鍵を登録する。
/// - 登録済み key がサーバー側で失効済みなら、新しい key pair に rotate する。
/// - 登録済み key がサーバー側に見つからないだけなら、同じ公開鍵を再登録する。
pub async fn ensure_registered_device_key(
    profile_root: &Path,
    provider: &str,
    client: &super::bmz_official::BmzOfficialIrClient,
) -> Result<StoredDeviceKey> {
    let mut key = load_or_create_device_key(profile_root, provider)?;
    let Some(key_id) = key.key_id.clone() else {
        let key_id = client.register_device_key(&key.public_key).await?;
        key.key_id = Some(key_id);
        save_device_key(profile_root, &key)?;
        return Ok(key);
    };

    let keys = client.list_device_keys().await?;
    match keys.device_keys.into_iter().find(|entry| entry.id == key_id) {
        Some(entry) if entry.revoked_at.is_none() && entry.public_key == key.public_key => Ok(key),
        Some(entry) if entry.revoked_at.is_some() => {
            rotate_registered_device_key(profile_root, provider, client).await
        }
        Some(_) | None => {
            let key_id = client.register_device_key(&key.public_key).await?;
            key.key_id = Some(key_id);
            save_device_key(profile_root, &key)?;
            Ok(key)
        }
    }
}

/// 旧 device key を可能ならサーバーで失効し、新しい key pair を生成・登録する。
pub async fn rotate_registered_device_key(
    profile_root: &Path,
    provider: &str,
    client: &super::bmz_official::BmzOfficialIrClient,
) -> Result<StoredDeviceKey> {
    let old_key = load_or_create_device_key(profile_root, provider)?;
    if let Some(old_key_id) = old_key.key_id.as_deref() {
        if let Err(error) = client.revoke_device_key(old_key_id).await {
            tracing::warn!(provider, key_id = old_key_id, %error, "failed to revoke old IR device key");
        }
    }

    delete_device_key(profile_root, provider)?;
    let mut new_key = load_or_create_device_key(profile_root, provider)?;
    let key_id = client.register_device_key(&new_key.public_key).await?;
    new_key.key_id = Some(key_id);
    save_device_key(profile_root, &new_key)?;
    Ok(new_key)
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
    build_evidence_for_value(key, &serde_json::to_value(payload)?, EVIDENCE_SCHEMA)
}

/// 任意の payload JSON (コーススコア等) 向けの evidence。`schema` で
/// `bmz-score-evidence-v1` / `bmz-course-score-evidence-v1` を切り替える。
pub fn build_evidence_for_value(
    key: &StoredDeviceKey,
    payload: &serde_json::Value,
    schema: &str,
) -> Result<std::collections::BTreeMap<String, serde_json::Value>> {
    let canonical = canonical_value_json(payload)?;
    let canonical_hash = Sha256::digest(canonical.as_bytes());
    let secret: [u8; 32] = decode_hex_32(&key.private_key)?;
    let signing = SigningKey::from_bytes(&secret);
    let signature = signing.sign(&canonical_hash);

    let mut evidence = std::collections::BTreeMap::new();
    evidence.insert("schema".to_string(), serde_json::json!(schema));
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
    canonical_value_json(&serde_json::to_value(payload)?)
}

fn canonical_value_json(payload: &Value) -> Result<String> {
    let mut value = payload.clone();
    if let Some(object) = value.as_object_mut() {
        object.remove("evidence");
    }
    canonical_json(&value)
}

fn canonical_json(value: &Value) -> Result<String> {
    match value {
        Value::Null => Ok("null".to_string()),
        Value::Bool(value) => Ok(if *value { "true" } else { "false" }.to_string()),
        Value::Number(value) => canonical_number(value),
        Value::String(value) => {
            serde_json::to_string(value).context("failed to serialize JSON string")
        }
        Value::Array(values) => {
            let mut out = String::from("[");
            for (index, value) in values.iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                out.push_str(&canonical_json(value)?);
            }
            out.push(']');
            Ok(out)
        }
        Value::Object(object) => {
            let mut entries = object.iter().collect::<Vec<_>>();
            entries.sort_by(|(left, _), (right, _)| compare_utf16(left, right));

            let mut out = String::from("{");
            for (index, (key, value)) in entries.into_iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                out.push_str(
                    &serde_json::to_string(key).context("failed to serialize JSON object key")?,
                );
                out.push(':');
                out.push_str(&canonical_json(value)?);
            }
            out.push('}');
            Ok(out)
        }
    }
}

fn canonical_number(value: &serde_json::Number) -> Result<String> {
    if let Some(number) = value.as_i64() {
        return Ok(number.to_string());
    }
    if let Some(number) = value.as_u64() {
        return Ok(number.to_string());
    }
    let number = value.as_f64().context("JSON number is not representable as f64")?;
    if !number.is_finite() {
        anyhow::bail!("JSON number must be finite");
    }
    Ok(format_ecmascript_number(number))
}

fn format_ecmascript_number(number: f64) -> String {
    if number == 0.0 {
        return "0".to_string();
    }

    let negative = number.is_sign_negative();
    let magnitude = number.abs();
    let mut buffer = ryu::Buffer::new();
    let formatted = buffer.format_finite(magnitude);
    let (mut digits, mut decimal_pos) = decimal_parts(formatted);
    let leading_zeros = digits.bytes().take_while(|byte| *byte == b'0').count();
    if leading_zeros > 0 {
        digits.drain(..leading_zeros);
        decimal_pos -= leading_zeros as i32;
    }
    if digits.is_empty() {
        return "0".to_string();
    }
    while digits.ends_with('0') && digits.len() > 1 {
        digits.pop();
    }

    let mut out = String::new();
    if negative {
        out.push('-');
    }
    if decimal_pos > -6 && decimal_pos <= 21 {
        push_fixed_decimal(&mut out, &digits, decimal_pos);
    } else {
        push_exponential_decimal(&mut out, &digits, decimal_pos);
    }
    out
}

fn decimal_parts(formatted: &str) -> (String, i32) {
    let (mantissa, exponent) = match formatted.find(['e', 'E']) {
        Some(index) => {
            let exponent = formatted[index + 1..].parse::<i32>().unwrap_or(0);
            (&formatted[..index], exponent)
        }
        None => (formatted, 0),
    };
    let fraction_len = mantissa.find('.').map(|index| mantissa.len() - index - 1).unwrap_or(0);
    let digits = mantissa.chars().filter(|ch| *ch != '.').collect::<String>();
    let decimal_pos = digits.len() as i32 - fraction_len as i32 + exponent;
    (digits, decimal_pos)
}

fn push_fixed_decimal(out: &mut String, digits: &str, decimal_pos: i32) {
    if decimal_pos <= 0 {
        out.push_str("0.");
        for _ in 0..-decimal_pos {
            out.push('0');
        }
        out.push_str(digits);
        return;
    }
    let decimal_pos = decimal_pos as usize;
    if decimal_pos >= digits.len() {
        out.push_str(digits);
        for _ in 0..decimal_pos - digits.len() {
            out.push('0');
        }
        return;
    }
    out.push_str(&digits[..decimal_pos]);
    out.push('.');
    out.push_str(&digits[decimal_pos..]);
}

fn push_exponential_decimal(out: &mut String, digits: &str, decimal_pos: i32) {
    let exponent = decimal_pos - 1;
    let mut chars = digits.chars();
    out.push(chars.next().expect("digits is not empty"));
    let rest = chars.as_str();
    if !rest.is_empty() {
        out.push('.');
        out.push_str(rest);
    }
    out.push('e');
    if exponent >= 0 {
        out.push('+');
    }
    out.push_str(&exponent.to_string());
}

fn compare_utf16(left: &str, right: &str) -> Ordering {
    left.encode_utf16().cmp(right.encode_utf16())
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

    #[test]
    fn canonical_json_matches_jcs_number_formatting() {
        let value = serde_json::json!({
            "numbers": [
                333333333.33333329,
                1e30,
                4.50,
                2e-3,
                1e-27,
                1e-6,
                1e-7,
                -0.0
            ],
            "chart": {
                "total": 160.0,
                "bpm": {
                    "min": 120.0,
                    "max": 120.5
                }
            },
            "evidence": {
                "canonical_hash": "ignored"
            }
        });

        assert_eq!(
            canonical_value_json(&value).unwrap(),
            "{\"chart\":{\"bpm\":{\"max\":120.5,\"min\":120},\"total\":160},\"numbers\":[333333333.3333333,1e+30,4.5,0.002,1e-27,0.000001,1e-7,0]}"
        );
    }

    #[test]
    fn canonical_json_sorts_keys_by_utf16_code_units() {
        let value = serde_json::json!({
            "\u{e000}": 2,
            "\u{10000}": 1,
        });

        assert_eq!(canonical_value_json(&value).unwrap(), "{\"𐀀\":1,\"\":2}");
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
                "mode": "7K",
                "notes": {"total": 1, "ln": 0, "cn": 0, "hcn": 0, "mine": 0},
                "features": {
                    "random": false, "stop": false, "ln": false,
                    "cn": false, "hcn": false, "mine": false
                }
            },
            "rule": {
                "play_mode": "single",
                "key_mode": "7K",
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
                "min_bp": 0,
                "min_cb": 0
            },
            "play_options": {"device_type": "keyboard"},
            "idempotency_key": "test-1"
        }))
        .unwrap()
    }
}

use super::*;

pub(super) async fn submit_score_attestation_job(
    profile_root: &Path,
    provider: &IrProviderConfig,
    payload_json: &str,
    credentials: IrStoredCredentials,
) -> Result<(String, String, String)> {
    if crate::ir::bokutachi::is_config(provider) {
        return Err(crate::ir::bokutachi::unsupported());
    }
    if crate::ir::bms_ir::is_bms_ir_config(provider) {
        bail!("BMS-IR score attestation is not supported");
    }
    const ATTESTATION_PURPOSE: &str = "score_attestation";
    const ATTESTATION_SCHEMA: &str = "bmz-score-attestation-v1";

    let payload: IrScoreAttestationJobPayload = serde_json::from_str(payload_json)
        .context("failed to parse stored IR attestation payload")?;
    if payload.remote_score_id.is_empty() {
        bail!("stored IR attestation payload has no remote score id");
    }
    let provider_key = crate::ir::provider_key::configured_provider_key(provider)
        .context("IR provider key is not set; log in again")?;
    let client = BmzOfficialIrClient::new(&provider.base_url, credentials.access_token)?;
    let unsigned = serde_json::json!({
        "score_id": &payload.remote_score_id,
        "purpose": ATTESTATION_PURPOSE,
    });
    let key =
        crate::ir::device_key::ensure_registered_device_key(profile_root, provider_key, &client)
            .await?;
    let evidence =
        crate::ir::device_key::build_evidence_for_value(&key, &unsigned, ATTESTATION_SCHEMA)?;
    let request = serde_json::json!({
        "score_id": &payload.remote_score_id,
        "purpose": ATTESTATION_PURPOSE,
        "evidence": evidence,
    });
    let response = client.attest_score(&payload.remote_score_id, &request).await?;
    Ok((
        payload.remote_score_id,
        serde_json::to_string(&request)?,
        serde_json::to_string(&response)?,
    ))
}

/// コーススコアジョブの送信。署名 evidence を付けて
/// `POST /api/v1/course-scores` へ送る。
pub(super) async fn submit_course_job_payload(
    profile_root: &Path,
    provider: &IrProviderConfig,
    payload_json: &str,
    credentials: IrStoredCredentials,
) -> Result<(String, String)> {
    if crate::ir::bokutachi::is_config(provider) {
        return Err(crate::ir::bokutachi::unsupported());
    }
    let mut payload: serde_json::Value =
        serde_json::from_str(payload_json).context("failed to parse stored IR course payload")?;
    normalize_legacy_course_payload(&mut payload);
    crate::ir::kaleid::ensure_course_supported(provider, &payload)?;
    let provider_key = crate::ir::provider_key::configured_provider_key(provider)
        .context("IR provider key is not set; log in again")?;
    if crate::ir::bms_ir::is_bms_ir_config(provider) {
        let client = crate::ir::bms_ir::BmsIrClient::new(&provider.base_url)?;
        let outcome = client
            .submit_course_score(&payload, &credentials.account_id, &credentials.access_token)
            .await?;
        return Ok((outcome.redacted_request_json, outcome.response_json));
    }
    if crate::ir::rian_ir::is_rian_ir_config(provider) {
        let client = crate::ir::rian_ir::RianIrClient::new(&provider.base_url)?;
        let outcome = client
            .submit_course_score(&payload, &credentials.account_id, &credentials.access_token)
            .await?;
        return Ok((outcome.redacted_request_json, outcome.response_json));
    }
    let client = BmzOfficialIrClient::new(&provider.base_url, credentials.access_token)?;
    let evidence = async {
        let key = crate::ir::device_key::ensure_registered_device_key(
            profile_root,
            provider_key,
            &client,
        )
        .await?;
        crate::ir::device_key::build_evidence_for_value(
            &key,
            &payload,
            "bmz-course-score-evidence-v1",
        )
    }
    .await;
    match evidence {
        Ok(evidence) => {
            if let Some(object) = payload.as_object_mut() {
                object.insert("evidence".to_string(), serde_json::json!(evidence));
            }
        }
        Err(error) => {
            tracing::warn!(provider = provider.provider, %error, "failed to attach IR course evidence; sending unsigned");
        }
    }
    let request_json = serde_json::to_string(&payload)?;
    let response = client.submit_course_score(&payload).await?;
    Ok((request_json, serde_json::to_string(&response)?))
}

pub(super) fn normalize_legacy_course_payload(payload: &mut serde_json::Value) {
    normalize_legacy_seed_options_value(payload);
    let Some(rule) = payload.get_mut("rule").and_then(serde_json::Value::as_object_mut) else {
        return;
    };
    let needs_default = match rule.get("rule_mode") {
        Some(serde_json::Value::String(value)) => value.trim().is_empty(),
        Some(serde_json::Value::Null) | None => true,
        Some(_) => false,
    };
    if needs_default {
        rule.insert("rule_mode".to_string(), serde_json::json!("Beatoraja"));
    }
}

pub(super) fn normalize_legacy_score_seed_options(payload: &mut IrScoreSubmission) {
    for key in ["seed", "random_seed"] {
        let Some(value) = payload.play_options.get_mut(key) else {
            continue;
        };
        normalize_integer_value_to_string(value);
    }
}

pub(super) fn normalize_legacy_seed_options_value(payload: &mut serde_json::Value) {
    let Some(play_options) =
        payload.get_mut("play_options").and_then(serde_json::Value::as_object_mut)
    else {
        return;
    };
    for key in ["seed", "random_seed"] {
        if let Some(value) = play_options.get_mut(key) {
            normalize_integer_value_to_string(value);
        }
    }
}

pub(super) fn normalize_integer_value_to_string(value: &mut serde_json::Value) {
    let integer = value
        .as_i64()
        .map(|value| value.to_string())
        .or_else(|| value.as_u64().map(|value| value.to_string()));
    if let Some(integer) = integer {
        *value = serde_json::Value::String(integer);
    }
}

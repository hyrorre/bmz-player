use super::*;

pub(super) enum HttpPayload {
    Metadata(Value),
    Archive(Response, Vec<u8>),
}

pub(super) async fn inspect(mut response: Response) -> Result<HttpPayload> {
    anyhow::ensure!(
        response.content_length().is_none_or(|size| size <= MAX_DOWNLOAD_BYTES),
        "HTTP archive exceeds size limit"
    );
    if response_is_json(&response) {
        return Ok(HttpPayload::Metadata(read_metadata(response).await?));
    }
    let prefix = response.chunk().await?.map(|bytes| bytes.to_vec()).unwrap_or_default();
    if prefix
        .iter()
        .find(|byte| !byte.is_ascii_whitespace())
        .is_some_and(|byte| matches!(byte, b'{' | b'['))
    {
        return Ok(HttpPayload::Metadata(read_metadata_with_prefix(response, prefix).await?));
    }
    Ok(HttpPayload::Archive(response, prefix))
}

pub(super) async fn read_metadata(response: Response) -> Result<Value> {
    read_metadata_with_prefix(response, Vec::new()).await
}

async fn read_metadata_with_prefix(mut response: Response, mut body: Vec<u8>) -> Result<Value> {
    anyhow::ensure!(
        body.len() <= MAX_METADATA_BYTES as usize,
        "HTTP API metadata exceeds size limit"
    );
    anyhow::ensure!(
        response.content_length().is_none_or(|size| size <= MAX_METADATA_BYTES),
        "HTTP API metadata exceeds size limit"
    );
    while let Some(chunk) = response.chunk().await? {
        anyhow::ensure!(
            body.len().saturating_add(chunk.len()) <= MAX_METADATA_BYTES as usize,
            "HTTP API metadata exceeds size limit"
        );
        body.extend_from_slice(&chunk);
    }
    serde_json::from_slice(&body).context("HTTP APIのJSON応答を解析できませんでした")
}

pub(super) fn metadata_download_url(value: &Value) -> Result<&str> {
    let object =
        value.as_object().ok_or_else(|| anyhow::anyhow!("HTTP API response must be an object"))?;
    if let Some(result) = object.get("result") {
        anyhow::ensure!(result.as_str() == Some("success"), "HTTP API reports chart unavailable");
    }
    if let Some(success) = object.get("success") {
        anyhow::ensure!(success.as_bool() == Some(true), "HTTP API reports chart unavailable");
    }
    anyhow::ensure!(
        !object.get("error").is_some_and(|error| !error.is_null()
            && error.as_str() != Some("")
            && error.as_bool() != Some(false)),
        "HTTP API returned an error"
    );
    if let Some(size) = object.get("fileSize")
        && !size.is_null()
    {
        let size = size
            .as_u64()
            .or_else(|| size.as_str().and_then(|size| size.parse().ok()))
            .ok_or_else(|| anyhow::anyhow!("HTTP API returned an invalid file size"))?;
        anyhow::ensure!(size <= MAX_DOWNLOAD_BYTES, "HTTP API archive exceeds size limit");
    }
    for key in ["downloadURL", "downloadUrl", "download_url", "song_url", "url"] {
        if let Some(url) =
            object.get(key).and_then(Value::as_str).filter(|url| !url.trim().is_empty())
        {
            return Ok(url);
        }
    }
    if let Some(data) = object.get("data") {
        return metadata_download_url(data);
    }
    bail!("HTTP API response has no supported download URL")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn api_errors_and_unknown_json_are_never_success() {
        for value in [
            serde_json::json!({"result":"fail","data":{"song_url":"https://archive.test"}}),
            serde_json::json!({"result":"success","data":{"song_url":""}}),
            serde_json::json!({"success":false,"url":"https://archive.test"}),
            serde_json::json!({"error":"unavailable","url":"https://archive.test"}),
            serde_json::json!({"unexpected":{"url":"https://archive.test"}}),
            serde_json::json!({"downloadURL":"https://archive.test","fileSize":MAX_DOWNLOAD_BYTES+1}),
        ] {
            assert!(metadata_download_url(&value).is_err(), "{value}");
        }
        assert_eq!(
            metadata_download_url(
                &serde_json::json!({"result":"success","data":{"song_url":"https://archive.test"}})
            )
            .unwrap(),
            "https://archive.test"
        );
    }

    #[test]
    fn optional_metadata_fields_keep_legacy_custom_responses_usable() {
        for value in [
            serde_json::json!({"error":false,"downloadURL":"/archive","fileSize":null}),
            serde_json::json!({"downloadURL":"/archive","fileSize":"123"}),
            serde_json::json!({"error":"","downloadURL":"/archive"}),
        ] {
            assert_eq!(metadata_download_url(&value).unwrap(), "/archive");
        }
    }
}

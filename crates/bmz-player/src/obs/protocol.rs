use super::*;

pub(super) fn next_reconnect_delay(delay: Duration) -> Duration {
    let next = delay.mul_f64(RECONNECT_MULTIPLIER);
    next.min(MAX_RECONNECT_DELAY)
}

pub(super) fn scene_not_ready_retry_delay(retry_count: u8) -> Duration {
    Duration::from_millis(250 * u64::from(retry_count.min(4)))
}

pub(super) fn publish_status(
    status_tx: &watch::Sender<ObsConnectionStatus>,
    status: ObsConnectionStatus,
) {
    status_tx.send_replace(status);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ObsConnectionFailureKind {
    ServerUnavailable,
    Network,
    Tls,
    Handshake,
    Configuration,
}

impl ObsConnectionFailureKind {
    pub(super) fn log_kind(self) -> &'static str {
        match self {
            Self::ServerUnavailable => "server_unavailable",
            Self::Network => "network",
            Self::Tls => "tls",
            Self::Handshake => "handshake",
            Self::Configuration => "configuration",
        }
    }
}

pub(super) fn classify_connection_failure(error: &TungsteniteError) -> ObsConnectionFailureKind {
    match error {
        TungsteniteError::Io(error) if error.kind() == ErrorKind::ConnectionRefused => {
            ObsConnectionFailureKind::ServerUnavailable
        }
        TungsteniteError::Tls(_) => ObsConnectionFailureKind::Tls,
        TungsteniteError::Http(_) | TungsteniteError::HttpFormat(_) => {
            ObsConnectionFailureKind::Handshake
        }
        TungsteniteError::Url(_) => ObsConnectionFailureKind::Configuration,
        _ => ObsConnectionFailureKind::Network,
    }
}

pub(super) fn report_connect_failure(
    status_tx: &watch::Sender<ObsConnectionStatus>,
    last_reported_issue: &mut Option<&'static str>,
    url: &str,
    error: &TungsteniteError,
    retry_delay: Duration,
    ever_identified: bool,
) -> bool {
    let kind = classify_connection_failure(error);
    let log_kind = kind.log_kind();
    let retry_in_ms = retry_delay.as_millis() as u64;
    if kind == ObsConnectionFailureKind::Configuration {
        let detail = format!("OBS WebSocket の接続先が無効です: {error}");
        publish_status(
            status_tx,
            ObsConnectionStatus::new(
                ObsConnectionStatusKind::ConfigurationError,
                Some("ホストとポートを確認して保存してください。".to_string()),
                Some(detail.clone()),
                None,
            ),
        );
        tracing::error!(
            kind = log_kind,
            url,
            ever_identified,
            error = %error,
            "OBS WebSocket reconnect paused"
        );
        return true;
    }

    let changed = last_reported_issue.replace(log_kind) != Some(log_kind);
    match kind {
        ObsConnectionFailureKind::ServerUnavailable => {
            publish_status(
                status_tx,
                ObsConnectionStatus::new(
                    ObsConnectionStatusKind::WaitingForServer,
                    Some("OBS が起動していません。起動を待機しています。".to_string()),
                    None,
                    Some(retry_in_ms),
                ),
            );
            if changed {
                tracing::info!(
                    kind = log_kind,
                    url,
                    retry_in_ms,
                    ever_identified,
                    "OBS WebSocket unavailable; waiting for server"
                );
            } else {
                tracing::debug!(
                    kind = log_kind,
                    url,
                    retry_in_ms,
                    ever_identified,
                    "OBS WebSocket still unavailable"
                );
            }
        }
        ObsConnectionFailureKind::Network
        | ObsConnectionFailureKind::Tls
        | ObsConnectionFailureKind::Handshake => {
            let detail = match kind {
                ObsConnectionFailureKind::Network => "OBS WebSocket への接続に失敗しました。",
                ObsConnectionFailureKind::Tls => "OBS WebSocket の TLS 接続に失敗しました。",
                ObsConnectionFailureKind::Handshake => {
                    "OBS WebSocket のハンドシェイクに失敗しました。"
                }
                _ => unreachable!(),
            };
            publish_status(
                status_tx,
                ObsConnectionStatus::new(
                    ObsConnectionStatusKind::Reconnecting,
                    Some(detail.to_string()),
                    Some(error.to_string()),
                    Some(retry_in_ms),
                ),
            );
            if changed {
                tracing::warn!(
                    kind = log_kind,
                    url,
                    retry_in_ms,
                    ever_identified,
                    error = %error,
                    "OBS WebSocket connect failed; retrying"
                );
            } else {
                tracing::debug!(
                    kind = log_kind,
                    url,
                    retry_in_ms,
                    ever_identified,
                    error = %error,
                    "OBS WebSocket connect still failing"
                );
            }
        }
        ObsConnectionFailureKind::Configuration => unreachable!(),
    }
    false
}

pub(super) fn report_disconnect(
    status_tx: &watch::Sender<ObsConnectionStatus>,
    last_reported_issue: &mut Option<&'static str>,
    url: &str,
    disconnect: &ObsDisconnect,
    retry_delay: Duration,
    ever_identified: bool,
) {
    let retry_in_ms = retry_delay.as_millis() as u64;
    let log_kind = if disconnect.expected { "server_stopped" } else { "connection_lost" };
    let changed = last_reported_issue.replace(log_kind) != Some(log_kind);
    let status_kind = if disconnect.expected {
        ObsConnectionStatusKind::WaitingForServer
    } else {
        ObsConnectionStatusKind::Reconnecting
    };
    publish_status(
        status_tx,
        ObsConnectionStatus::new(
            status_kind,
            Some(disconnect.detail.clone()),
            (!disconnect.expected).then(|| disconnect.detail.clone()),
            Some(retry_in_ms),
        ),
    );
    if disconnect.expected {
        if changed {
            tracing::info!(
                kind = log_kind,
                url,
                retry_in_ms,
                ever_identified,
                "OBS WebSocket server stopped; waiting for restart"
            );
        } else {
            tracing::debug!(
                kind = log_kind,
                url,
                retry_in_ms,
                ever_identified,
                "OBS WebSocket server is still stopped"
            );
        }
    } else if changed {
        tracing::warn!(
            kind = log_kind,
            url,
            retry_in_ms,
            ever_identified,
            error = %disconnect.detail,
            "OBS WebSocket connection ended; retrying"
        );
    } else {
        tracing::debug!(
            kind = log_kind,
            url,
            retry_in_ms,
            ever_identified,
            error = %disconnect.detail,
            "OBS WebSocket connection remains unavailable"
        );
    }
}

pub(super) fn pause_reconnect(
    status_tx: &watch::Sender<ObsConnectionStatus>,
    kind: ObsConnectionStatusKind,
    url: &str,
    detail: String,
    ever_identified: bool,
) {
    publish_status(status_tx, ObsConnectionStatus::new(kind, None, Some(detail.clone()), None));
    tracing::error!(
        kind = obs_status_log_kind(kind),
        url,
        ever_identified,
        error = %detail,
        "OBS WebSocket reconnect paused"
    );
}

pub(super) fn obs_status_log_kind(kind: ObsConnectionStatusKind) -> &'static str {
    match kind {
        ObsConnectionStatusKind::AuthenticationFailed => "authentication",
        ObsConnectionStatusKind::ConfigurationError => "configuration",
        ObsConnectionStatusKind::Disabled => "disabled",
        ObsConnectionStatusKind::Connecting => "connecting",
        ObsConnectionStatusKind::WaitingForServer => "server_unavailable",
        ObsConnectionStatusKind::Connected => "connected",
        ObsConnectionStatusKind::Reconnecting => "reconnecting",
    }
}

pub(super) fn connection_action_for_close(
    state: &mut ObsConnectionState,
    close: Option<CloseFrame<'_>>,
) -> ConnectionAction {
    let expected = close
        .as_ref()
        .is_some_and(|frame| matches!(frame.code, CloseCode::Normal | CloseCode::Away));
    let (code, reason) = match close {
        Some(frame) => (Some(u16::from(frame.code)), frame.reason.to_string()),
        None => (None, String::new()),
    };
    let detail = match (code, reason.is_empty()) {
        (Some(code), true) => format!("OBS WebSocket が切断されました (code {code})。"),
        (Some(code), false) => format!("OBS WebSocket が切断されました (code {code}): {reason}"),
        (None, _) => "OBS WebSocket が切断されました。".to_string(),
    };
    match code {
        Some(4009) => {
            ConnectionAction::Pause { kind: ObsConnectionStatusKind::AuthenticationFailed, detail }
        }
        Some(4010 | 4011) => {
            ConnectionAction::Pause { kind: ObsConnectionStatusKind::ConfigurationError, detail }
        }
        _ => {
            state.set_disconnect(detail, expected);
            ConnectionAction::Reconnect
        }
    }
}

pub(super) async fn wait_for_shutdown(rx: &mut mpsc::UnboundedReceiver<ObsCommand>) {
    while !matches!(rx.recv().await, Some(ObsCommand::Shutdown | ObsCommand::Close) | None) {}
}

pub(super) async fn send_json(sink: &mut ObsSink, value: Value) -> Result<()> {
    tokio::time::timeout(Duration::from_secs(2), sink.send(Message::Text(value.to_string())))
        .await
        .context("OBS send timeout")?
        .map_err(Into::into)
}

pub(super) fn identify_message(config: &ObsConfig, hello: &Value) -> Value {
    let mut data = json!({ "rpcVersion": 1 });
    if let Some(auth) = hello.get("authentication") {
        let salt = auth.get("salt").and_then(Value::as_str).unwrap_or_default();
        let challenge = auth.get("challenge").and_then(Value::as_str).unwrap_or_default();
        if !salt.is_empty() || !challenge.is_empty() {
            data["authentication"] = json!(obs_authentication(&config.password, salt, challenge));
        }
    }
    json!({ "op": 1, "d": data })
}

pub(super) fn request_message(
    request_type: &str,
    request_id: &str,
    request_data: Option<Value>,
) -> Value {
    let mut data = json!({
        "requestType": request_type,
        "requestId": request_id,
    });
    if let Some(request_data) = request_data {
        data["requestData"] = request_data;
    }
    json!({ "op": 6, "d": data })
}

pub(super) fn obs_authentication(password: &str, salt: &str, challenge: &str) -> String {
    let secret = sha256_base64(&format!("{password}{salt}"));
    sha256_base64(&format!("{secret}{challenge}"))
}

pub(super) fn sha256_base64(input: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(input.as_bytes());
    BASE64.encode(hasher.finalize())
}

pub(super) fn parse_scene_names(data: &Value) -> Vec<String> {
    let mut names: Vec<String> = data
        .get("scenes")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|scene| scene.get("sceneName").and_then(Value::as_str))
        .map(ToOwned::to_owned)
        .collect();
    names.reverse();
    names
}

pub(super) fn recording_mode_matches(
    mode: ObsRecordingMode,
    reason: ObsRecordingSaveReason,
) -> bool {
    matches!(
        (mode, reason),
        (ObsRecordingMode::OnScreenshot, ObsRecordingSaveReason::OnScreenshot)
            | (ObsRecordingMode::OnReplay, ObsRecordingSaveReason::OnReplay)
    )
}

pub(super) fn output_state_started(state: &str) -> bool {
    state == "OBS_WEBSOCKET_OUTPUT_STARTED" || state.ends_with("_STARTED")
}

pub(super) fn output_state_stopped(state: &str) -> bool {
    state == "OBS_WEBSOCKET_OUTPUT_STOPPED" || state.ends_with("_STOPPED")
}

pub(super) fn delete_recording_file(path: PathBuf, mode: ObsRecordingMode, reason: &'static str) {
    if mode == ObsRecordingMode::KeepAll || path.as_os_str().is_empty() {
        return;
    }
    if !path.is_file() {
        tracing::debug!(path = %path.display(), reason, "OBS recording cleanup skipped");
        return;
    }
    match std::fs::remove_file(&path) {
        Ok(()) => tracing::info!(path = %path.display(), reason, "OBS recording deleted"),
        Err(error) => {
            tracing::warn!(path = %path.display(), %error, reason, "failed to delete OBS recording")
        }
    }
}

pub(super) fn obs_ws_url(config: &ObsConfig) -> String {
    let host = config.host.trim();
    if host.starts_with("ws://") || host.starts_with("wss://") {
        host.to_string()
    } else if host.is_empty() {
        format!("ws://localhost:{}", config.port)
    } else {
        format!("ws://{host}:{}", config.port)
    }
}

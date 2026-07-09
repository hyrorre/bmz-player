use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use futures_util::stream::SplitSink;
use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use tokio::net::TcpStream;
use tokio::sync::mpsc;
use tokio::time::{Duration, Instant as TokioInstant};
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream, connect_async};

use crate::config::app_config::{ObsActionConfig, ObsConfig, ObsRecordingMode};

const INITIAL_RECONNECT_DELAY: Duration = Duration::from_millis(2000);
const MAX_RECONNECT_DELAY: Duration = Duration::from_millis(15000);
const RECONNECT_MULTIPLIER: f64 = 1.25;
const RESTART_RECORDING_DELAY: Duration = Duration::from_millis(500);
const LOAD_SCENES_TIMEOUT: Duration = Duration::from_secs(10);

type ObsWebSocket = WebSocketStream<MaybeTlsStream<TcpStream>>;
type ObsSink = SplitSink<ObsWebSocket, Message>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObsEventKey {
    MusicSelect,
    Decide,
    Play,
    PlayEnded,
    Result,
    CourseResult,
}

impl ObsEventKey {
    pub const ALL: [Self; 6] = [
        Self::MusicSelect,
        Self::Decide,
        Self::Play,
        Self::PlayEnded,
        Self::Result,
        Self::CourseResult,
    ];

    pub fn config_key(self) -> &'static str {
        match self {
            Self::MusicSelect => "MUSICSELECT",
            Self::Decide => "DECIDE",
            Self::Play => "PLAY",
            Self::PlayEnded => "PLAY_ENDED",
            Self::Result => "RESULT",
            Self::CourseResult => "COURSERESULT",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::MusicSelect => "選曲",
            Self::Decide => "決定",
            Self::Play => "プレイ",
            Self::PlayEnded => "プレイ終了",
            Self::Result => "リザルト",
            Self::CourseResult => "コースリザルト",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObsRecordingSaveReason {
    OnScreenshot,
    OnReplay,
}

#[derive(Debug, Clone)]
pub struct ObsSceneList {
    pub version: String,
    pub scenes: Vec<String>,
    pub recording_active: bool,
}

#[derive(Clone)]
pub struct ObsController {
    tx: mpsc::UnboundedSender<ObsCommand>,
}

impl ObsController {
    pub fn spawn(config: ObsConfig) -> Option<Self> {
        if !config.enabled {
            return None;
        }
        let (tx, rx) = mpsc::unbounded_channel();
        tokio::spawn(run_obs_client(config, rx, tx.clone()));
        Some(Self { tx })
    }

    pub fn scene(&self, key: ObsEventKey) {
        let _ = self.tx.send(ObsCommand::ApplyEvent(key));
    }

    pub fn play_ended(&self) {
        let _ = self.tx.send(ObsCommand::ApplyEvent(ObsEventKey::PlayEnded));
    }

    pub fn retry_play(&self) {
        let _ = self.tx.send(ObsCommand::RetryPlay);
    }

    pub fn save_last_recording(&self, reason: ObsRecordingSaveReason) {
        let _ = self.tx.send(ObsCommand::SaveLastRecording(reason));
    }
}

impl Drop for ObsController {
    fn drop(&mut self) {
        let _ = self.tx.send(ObsCommand::Shutdown);
    }
}

#[derive(Debug)]
enum ObsCommand {
    ApplyEvent(ObsEventKey),
    RetryPlay,
    SaveLastRecording(ObsRecordingSaveReason),
    Shutdown,
}

struct ObsConnectionState {
    config: ObsConfig,
    request_counter: u64,
    is_recording: bool,
    restart_recording: bool,
    save_requested: bool,
    last_output_path: Option<PathBuf>,
    pending_stop_deadline: Option<TokioInstant>,
}

impl ObsConnectionState {
    fn new(config: ObsConfig) -> Self {
        Self {
            config,
            request_counter: 0,
            is_recording: false,
            restart_recording: false,
            save_requested: false,
            last_output_path: None,
            pending_stop_deadline: None,
        }
    }

    fn next_request_id(&mut self, request_type: &str) -> String {
        self.request_counter = self.request_counter.wrapping_add(1);
        format!("{request_type}-{}", self.request_counter)
    }

    async fn send_request(
        &mut self,
        sink: &mut ObsSink,
        request_type: &str,
        request_data: Option<Value>,
    ) -> Result<()> {
        let request_id = self.next_request_id(request_type);
        send_json(sink, request_message(request_type, &request_id, request_data)).await
    }

    async fn apply_event(&mut self, sink: &mut ObsSink, key: ObsEventKey) -> Result<()> {
        if self.pending_stop_deadline.take().is_some() {
            self.send_request(sink, "StopRecord", None).await?;
        }

        let scene = self
            .config
            .scenes
            .get(key.config_key())
            .map(|scene| scene.trim())
            .filter(|scene| !scene.is_empty())
            .map(ToOwned::to_owned);
        if let Some(scene) = scene {
            self.send_request(sink, "SetCurrentProgramScene", Some(json!({ "sceneName": scene })))
                .await?;
        }

        let action = self.config.actions.get(key.config_key()).copied().unwrap_or_default();
        self.apply_action(sink, action).await
    }

    async fn apply_action(&mut self, sink: &mut ObsSink, action: ObsActionConfig) -> Result<()> {
        match action {
            ObsActionConfig::None => Ok(()),
            ObsActionConfig::StartRecord => self.send_request(sink, "StartRecord", None).await,
            ObsActionConfig::StopRecord => {
                let wait_ms = self.config.record_stop_wait_ms.min(10_000);
                if wait_ms == 0 {
                    self.send_request(sink, "StopRecord", None).await
                } else {
                    self.pending_stop_deadline =
                        Some(TokioInstant::now() + Duration::from_millis(wait_ms));
                    Ok(())
                }
            }
        }
    }

    async fn flush_pending_stop(&mut self, sink: &mut ObsSink) -> Result<()> {
        if self.pending_stop_deadline.take().is_some() {
            self.send_request(sink, "StopRecord", None).await?;
        }
        Ok(())
    }

    async fn retry_play(
        &mut self,
        sink: &mut ObsSink,
        tx: mpsc::UnboundedSender<ObsCommand>,
    ) -> Result<()> {
        self.pending_stop_deadline = None;
        if self.is_recording {
            self.restart_recording = true;
            self.send_request(sink, "StopRecord", None).await?;
        }
        self.apply_event(sink, ObsEventKey::MusicSelect).await?;
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(1000)).await;
            let _ = tx.send(ObsCommand::ApplyEvent(ObsEventKey::Play));
        });
        Ok(())
    }

    fn save_last_recording(&mut self, reason: ObsRecordingSaveReason) {
        if recording_mode_matches(self.config.recording_mode, reason) {
            self.save_requested = true;
            tracing::info!(?reason, "OBS recording keep requested");
        }
    }

    async fn handle_response(&mut self, sink: &mut ObsSink, data: &Value) -> Result<()> {
        let request_type = data.get("requestType").and_then(Value::as_str).unwrap_or_default();
        let status = data.get("requestStatus").unwrap_or(&Value::Null);
        if !status.get("result").and_then(Value::as_bool).unwrap_or(false) {
            let code = status.get("code").and_then(Value::as_i64).unwrap_or_default();
            let comment = status.get("comment").and_then(Value::as_str).unwrap_or_default();
            tracing::warn!(request_type, code, comment, "OBS request failed");
            return Ok(());
        }
        let response_data = data.get("responseData").unwrap_or(&Value::Null);
        match request_type {
            "GetVersion" => {
                let obs_version =
                    response_data.get("obsVersion").and_then(Value::as_str).unwrap_or_default();
                let ws_version = response_data
                    .get("obsWebSocketVersion")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                tracing::info!(obs_version, ws_version, "OBS WebSocket identified");
            }
            "GetSceneList" => {
                let scenes = parse_scene_names(response_data);
                tracing::info!(count = scenes.len(), "OBS scene list loaded");
            }
            "GetRecordStatus" => {
                self.is_recording =
                    response_data.get("outputActive").and_then(Value::as_bool).unwrap_or(false);
            }
            "StopRecord" if self.restart_recording && !self.is_recording => {
                tokio::time::sleep(RESTART_RECORDING_DELAY).await;
                self.send_request(sink, "StartRecord", None).await?;
            }
            _ => {}
        }
        Ok(())
    }

    async fn handle_event(&mut self, sink: &mut ObsSink, data: &Value) -> Result<ConnectionAction> {
        let event_type = data.get("eventType").and_then(Value::as_str).unwrap_or_default();
        let event_data = data.get("eventData").unwrap_or(&Value::Null);
        match event_type {
            "ExitStarted" => {
                tracing::info!("OBS exit started");
                Ok(ConnectionAction::Reconnect)
            }
            "AuthenticationFailure" | "AuthenticationFailed" => {
                tracing::warn!("OBS authentication failed");
                Ok(ConnectionAction::Reconnect)
            }
            "RecordStateChanged" => {
                self.handle_record_state_changed(sink, event_data).await?;
                Ok(ConnectionAction::Continue)
            }
            _ => Ok(ConnectionAction::Continue),
        }
    }

    async fn handle_record_state_changed(
        &mut self,
        sink: &mut ObsSink,
        data: &Value,
    ) -> Result<()> {
        let state = data.get("outputState").and_then(Value::as_str).unwrap_or_default();
        let output_path = data.get("outputPath").and_then(Value::as_str).unwrap_or_default();
        if output_state_started(state) {
            self.is_recording = true;
            if let Some(path) = self.last_output_path.take() {
                if self.save_requested {
                    tracing::info!(path = %path.display(), "OBS recording kept");
                } else {
                    delete_recording_file(path, self.config.recording_mode, "previous recording");
                }
            }
            self.save_requested = false;
            return Ok(());
        }

        if output_state_stopped(state) {
            self.is_recording = false;
            if self.restart_recording {
                self.restart_recording = false;
                delete_recording_file(
                    PathBuf::from(output_path),
                    self.config.recording_mode,
                    "retry recording",
                );
                tokio::time::sleep(RESTART_RECORDING_DELAY).await;
                self.send_request(sink, "StartRecord", None).await?;
                return Ok(());
            }
            if !output_path.is_empty() {
                self.last_output_path = Some(PathBuf::from(output_path));
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ConnectionAction {
    Continue,
    Reconnect,
    Shutdown,
}

pub async fn load_scenes(config: ObsConfig) -> Result<ObsSceneList> {
    let url = obs_ws_url(&config);
    let (ws, _) = connect_async(&url).await.with_context(|| format!("failed to connect {url}"))?;
    let (mut sink, mut stream) = ws.split();
    let mut state = ObsConnectionState::new(config);
    let mut version = None;
    let mut scenes = None;
    let mut recording_active = None;
    let timeout = tokio::time::sleep(LOAD_SCENES_TIMEOUT);
    tokio::pin!(timeout);

    loop {
        tokio::select! {
            _ = &mut timeout => bail!("OBS scene load timed out"),
            message = stream.next() => {
                let Some(message) = message else {
                    bail!("OBS connection closed before scene list was received");
                };
                let message = message.context("failed to read OBS WebSocket message")?;
                match message {
                    Message::Text(text) => {
                        let value: Value = serde_json::from_str(text.as_ref())
                            .context("failed to parse OBS WebSocket message")?;
                        match value.get("op").and_then(Value::as_i64).unwrap_or(-1) {
                            0 => {
                                let hello = value.get("d").unwrap_or(&Value::Null);
                                send_json(&mut sink, identify_message(&state.config, hello)).await?;
                            }
                            2 => {
                                state.send_request(&mut sink, "GetVersion", None).await?;
                                state.send_request(&mut sink, "GetSceneList", None).await?;
                                state.send_request(&mut sink, "GetRecordStatus", None).await?;
                            }
                            7 => {
                                let data = value.get("d").unwrap_or(&Value::Null);
                                let request_type = data
                                    .get("requestType")
                                    .and_then(Value::as_str)
                                    .unwrap_or_default();
                                let status = data.get("requestStatus").unwrap_or(&Value::Null);
                                if !status.get("result").and_then(Value::as_bool).unwrap_or(false) {
                                    let comment = status
                                        .get("comment")
                                        .and_then(Value::as_str)
                                        .unwrap_or("OBS request failed");
                                    bail!("{request_type}: {comment}");
                                }
                                let response_data = data.get("responseData").unwrap_or(&Value::Null);
                                match request_type {
                                    "GetVersion" => {
                                        let obs_version = response_data
                                            .get("obsVersion")
                                            .and_then(Value::as_str)
                                            .unwrap_or_default();
                                        let ws_version = response_data
                                            .get("obsWebSocketVersion")
                                            .and_then(Value::as_str)
                                            .unwrap_or_default();
                                        version = Some(if ws_version.is_empty() {
                                            obs_version.to_string()
                                        } else if obs_version.is_empty() {
                                            ws_version.to_string()
                                        } else {
                                            format!("{obs_version} / obs-websocket {ws_version}")
                                        });
                                    }
                                    "GetSceneList" => scenes = Some(parse_scene_names(response_data)),
                                    "GetRecordStatus" => {
                                        recording_active = Some(response_data
                                            .get("outputActive")
                                            .and_then(Value::as_bool)
                                            .unwrap_or(false));
                                    }
                                    _ => {}
                                }
                                if let (Some(version), Some(scenes), Some(recording_active)) =
                                    (version.clone(), scenes.clone(), recording_active)
                                {
                                    return Ok(ObsSceneList { version, scenes, recording_active });
                                }
                            }
                            _ => {}
                        }
                    }
                    Message::Ping(payload) => sink.send(Message::Pong(payload)).await?,
                    Message::Close(_) => bail!("OBS closed the WebSocket connection"),
                    _ => {}
                }
            }
        }
    }
}

async fn run_obs_client(
    config: ObsConfig,
    mut rx: mpsc::UnboundedReceiver<ObsCommand>,
    tx: mpsc::UnboundedSender<ObsCommand>,
) {
    let mut reconnect_delay = INITIAL_RECONNECT_DELAY;
    loop {
        let url = obs_ws_url(&config);
        match connect_async(&url).await {
            Ok((ws, _)) => {
                tracing::info!(url, "OBS WebSocket connected");
                reconnect_delay = INITIAL_RECONNECT_DELAY;
                match run_connected(config.clone(), ws, &mut rx, tx.clone()).await {
                    Ok(ConnectionAction::Shutdown) => break,
                    Ok(ConnectionAction::Reconnect | ConnectionAction::Continue) => {}
                    Err(error) => tracing::warn!(%error, "OBS WebSocket connection ended"),
                }
            }
            Err(error) => tracing::warn!(url, %error, "OBS WebSocket connect failed"),
        }

        let should_shutdown = wait_for_reconnect_or_shutdown(&mut rx, reconnect_delay).await;
        if should_shutdown {
            break;
        }
        reconnect_delay = next_reconnect_delay(reconnect_delay);
    }
}

async fn run_connected(
    config: ObsConfig,
    ws: ObsWebSocket,
    rx: &mut mpsc::UnboundedReceiver<ObsCommand>,
    tx: mpsc::UnboundedSender<ObsCommand>,
) -> Result<ConnectionAction> {
    let (mut sink, mut stream) = ws.split();
    let mut state = ObsConnectionState::new(config);
    loop {
        if let Some(deadline) = state.pending_stop_deadline {
            tokio::select! {
                _ = tokio::time::sleep_until(deadline) => {
                    state.flush_pending_stop(&mut sink).await?;
                }
                command = rx.recv() => {
                    if handle_command(&mut state, &mut sink, tx.clone(), command).await? == ConnectionAction::Shutdown {
                        let _ = sink.send(Message::Close(None)).await;
                        return Ok(ConnectionAction::Shutdown);
                    }
                }
                message = stream.next() => {
                    match handle_stream_message(&mut state, &mut sink, message).await? {
                        ConnectionAction::Continue => {}
                        action => return Ok(action),
                    }
                }
            }
        } else {
            tokio::select! {
                command = rx.recv() => {
                    if handle_command(&mut state, &mut sink, tx.clone(), command).await? == ConnectionAction::Shutdown {
                        let _ = sink.send(Message::Close(None)).await;
                        return Ok(ConnectionAction::Shutdown);
                    }
                }
                message = stream.next() => {
                    match handle_stream_message(&mut state, &mut sink, message).await? {
                        ConnectionAction::Continue => {}
                        action => return Ok(action),
                    }
                }
            }
        }
    }
}

async fn handle_command(
    state: &mut ObsConnectionState,
    sink: &mut ObsSink,
    tx: mpsc::UnboundedSender<ObsCommand>,
    command: Option<ObsCommand>,
) -> Result<ConnectionAction> {
    let Some(command) = command else {
        return Ok(ConnectionAction::Shutdown);
    };
    match command {
        ObsCommand::ApplyEvent(key) => state.apply_event(sink, key).await?,
        ObsCommand::RetryPlay => state.retry_play(sink, tx).await?,
        ObsCommand::SaveLastRecording(reason) => state.save_last_recording(reason),
        ObsCommand::Shutdown => return Ok(ConnectionAction::Shutdown),
    }
    Ok(ConnectionAction::Continue)
}

async fn handle_stream_message(
    state: &mut ObsConnectionState,
    sink: &mut ObsSink,
    message: Option<Result<Message, tokio_tungstenite::tungstenite::Error>>,
) -> Result<ConnectionAction> {
    let Some(message) = message else {
        return Ok(ConnectionAction::Reconnect);
    };
    match message? {
        Message::Text(text) => handle_text_message(state, sink, text.as_ref()).await,
        Message::Ping(payload) => {
            sink.send(Message::Pong(payload)).await?;
            Ok(ConnectionAction::Continue)
        }
        Message::Close(_) => Ok(ConnectionAction::Reconnect),
        _ => Ok(ConnectionAction::Continue),
    }
}

async fn handle_text_message(
    state: &mut ObsConnectionState,
    sink: &mut ObsSink,
    text: &str,
) -> Result<ConnectionAction> {
    let value: Value = serde_json::from_str(text).context("failed to parse OBS message")?;
    let data = value.get("d").unwrap_or(&Value::Null);
    match value.get("op").and_then(Value::as_i64).unwrap_or(-1) {
        0 => {
            send_json(sink, identify_message(&state.config, data)).await?;
            Ok(ConnectionAction::Continue)
        }
        2 => {
            state.send_request(sink, "GetVersion", None).await?;
            state.send_request(sink, "GetSceneList", None).await?;
            state.send_request(sink, "GetRecordStatus", None).await?;
            Ok(ConnectionAction::Continue)
        }
        5 => state.handle_event(sink, data).await,
        7 => {
            state.handle_response(sink, data).await?;
            Ok(ConnectionAction::Continue)
        }
        _ => Ok(ConnectionAction::Continue),
    }
}

async fn wait_for_reconnect_or_shutdown(
    rx: &mut mpsc::UnboundedReceiver<ObsCommand>,
    delay: Duration,
) -> bool {
    let sleep = tokio::time::sleep(delay);
    tokio::pin!(sleep);
    loop {
        tokio::select! {
            _ = &mut sleep => return false,
            command = rx.recv() => {
                match command {
                    Some(ObsCommand::Shutdown) | None => return true,
                    _ => {}
                }
            }
        }
    }
}

fn next_reconnect_delay(delay: Duration) -> Duration {
    let next = delay.mul_f64(RECONNECT_MULTIPLIER);
    next.min(MAX_RECONNECT_DELAY)
}

async fn send_json(sink: &mut ObsSink, value: Value) -> Result<()> {
    sink.send(Message::Text(value.to_string())).await.map_err(Into::into)
}

fn identify_message(config: &ObsConfig, hello: &Value) -> Value {
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

fn request_message(request_type: &str, request_id: &str, request_data: Option<Value>) -> Value {
    let mut data = json!({
        "requestType": request_type,
        "requestId": request_id,
    });
    if let Some(request_data) = request_data {
        data["requestData"] = request_data;
    }
    json!({ "op": 6, "d": data })
}

fn obs_authentication(password: &str, salt: &str, challenge: &str) -> String {
    let secret = sha256_base64(&format!("{password}{salt}"));
    sha256_base64(&format!("{secret}{challenge}"))
}

fn sha256_base64(input: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(input.as_bytes());
    BASE64.encode(hasher.finalize())
}

fn parse_scene_names(data: &Value) -> Vec<String> {
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

fn recording_mode_matches(mode: ObsRecordingMode, reason: ObsRecordingSaveReason) -> bool {
    matches!(
        (mode, reason),
        (ObsRecordingMode::OnScreenshot, ObsRecordingSaveReason::OnScreenshot)
            | (ObsRecordingMode::OnReplay, ObsRecordingSaveReason::OnReplay)
    )
}

fn output_state_started(state: &str) -> bool {
    state == "OBS_WEBSOCKET_OUTPUT_STARTED" || state.ends_with("_STARTED")
}

fn output_state_stopped(state: &str) -> bool {
    state == "OBS_WEBSOCKET_OUTPUT_STOPPED" || state.ends_with("_STOPPED")
}

fn delete_recording_file(path: PathBuf, mode: ObsRecordingMode, reason: &'static str) {
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

fn obs_ws_url(config: &ObsConfig) -> String {
    let host = config.host.trim();
    if host.starts_with("ws://") || host.starts_with("wss://") {
        host.to_string()
    } else if host.is_empty() {
        format!("ws://localhost:{}", config.port)
    } else {
        format!("ws://{host}:{}", config.port)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn obs_authentication_matches_obs_websocket_v5_algorithm() {
        let auth = obs_authentication("pass", "salt", "challenge");

        assert_eq!(auth, "EabUNw4z9EKKpEOC0yvqBO8dJPSIcTb82eo+adWKOvk=");
    }

    #[test]
    fn request_message_omits_empty_request_data() {
        let message = request_message("GetVersion", "GetVersion-1", None);

        assert_eq!(message["op"], 6);
        assert_eq!(message["d"]["requestType"], "GetVersion");
        assert_eq!(message["d"]["requestId"], "GetVersion-1");
        assert!(message["d"].get("requestData").is_none());
    }

    #[test]
    fn parse_scene_names_matches_lr2oraja_order() {
        let names = parse_scene_names(&json!({
            "scenes": [
                { "sceneName": "Top" },
                { "sceneName": "Play" },
                { "sceneName": "Result" }
            ]
        }));

        assert_eq!(names, ["Result", "Play", "Top"]);
    }

    #[test]
    fn recording_mode_filters_save_reasons() {
        assert!(recording_mode_matches(
            ObsRecordingMode::OnScreenshot,
            ObsRecordingSaveReason::OnScreenshot
        ));
        assert!(recording_mode_matches(
            ObsRecordingMode::OnReplay,
            ObsRecordingSaveReason::OnReplay
        ));
        assert!(!recording_mode_matches(
            ObsRecordingMode::OnReplay,
            ObsRecordingSaveReason::OnScreenshot
        ));
        assert!(!recording_mode_matches(
            ObsRecordingMode::KeepAll,
            ObsRecordingSaveReason::OnReplay
        ));
    }

    #[test]
    fn obs_ws_url_accepts_plain_host_or_full_url() {
        let mut config = ObsConfig::default();
        assert_eq!(obs_ws_url(&config), "ws://localhost:4455");
        config.host = "192.0.2.1".to_string();
        config.port = 4456;
        assert_eq!(obs_ws_url(&config), "ws://192.0.2.1:4456");
        config.host = "ws://example.test:4455".to_string();
        assert_eq!(obs_ws_url(&config), "ws://example.test:4455");
    }
}

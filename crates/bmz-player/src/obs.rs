use std::collections::{HashMap, VecDeque};
use std::io::ErrorKind;
use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use futures_util::stream::SplitSink;
use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use tokio::net::TcpStream;
use tokio::sync::{mpsc, watch};
use tokio::time::{Duration, Instant as TokioInstant};
use tokio_tungstenite::tungstenite::protocol::CloseFrame;
use tokio_tungstenite::tungstenite::protocol::frame::coding::CloseCode;
use tokio_tungstenite::tungstenite::{Error as TungsteniteError, Message};
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream, connect_async};

use crate::config::app_config::{ObsActionConfig, ObsConfig, ObsRecordingMode};

const INITIAL_RECONNECT_DELAY: Duration = Duration::from_millis(2000);
const MAX_RECONNECT_DELAY: Duration = Duration::from_millis(15000);
const RECONNECT_MULTIPLIER: f64 = 1.25;
const RESTART_RECORDING_DELAY: Duration = Duration::from_millis(500);
const LOAD_SCENES_TIMEOUT: Duration = Duration::from_secs(10);
const MAX_SCENE_NOT_READY_RETRIES: u8 = 8;

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObsConnectionStatusKind {
    Disabled,
    Connecting,
    WaitingForServer,
    Connected,
    Reconnecting,
    AuthenticationFailed,
    ConfigurationError,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObsConnectionStatus {
    pub kind: ObsConnectionStatusKind,
    pub detail: Option<String>,
    pub last_error: Option<String>,
    pub retry_in_ms: Option<u64>,
}

impl ObsConnectionStatus {
    fn new(
        kind: ObsConnectionStatusKind,
        detail: Option<String>,
        last_error: Option<String>,
        retry_in_ms: Option<u64>,
    ) -> Self {
        Self { kind, detail, last_error, retry_in_ms }
    }

    pub fn disabled() -> Self {
        Self::new(ObsConnectionStatusKind::Disabled, None, None, None)
    }
}

impl Default for ObsConnectionStatus {
    fn default() -> Self {
        Self::disabled()
    }
}

#[derive(Clone)]
pub struct ObsController {
    tx: mpsc::UnboundedSender<ObsCommand>,
    status: watch::Receiver<ObsConnectionStatus>,
}

impl ObsController {
    pub fn spawn(config: ObsConfig) -> Option<Self> {
        if !config.enabled {
            return None;
        }
        let (tx, rx) = mpsc::unbounded_channel();
        let (status_tx, status) = watch::channel(ObsConnectionStatus {
            kind: ObsConnectionStatusKind::Connecting,
            detail: None,
            last_error: None,
            retry_in_ms: None,
        });
        tokio::spawn(run_obs_client(config, rx, tx.clone(), status_tx));
        Some(Self { tx, status })
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

    pub fn status(&self) -> ObsConnectionStatus {
        self.status.borrow().clone()
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
    RetryScene { key: ObsEventKey, retry_count: u8, generation: u64 },
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
    pending_scene_requests: HashMap<String, PendingSceneRequest>,
    scene_request_generation: u64,
    was_identified: bool,
    identified_this_connection: bool,
    last_disconnect: Option<ObsDisconnect>,
}

#[derive(Debug, Clone)]
struct ObsDisconnect {
    detail: String,
    expected: bool,
}

#[derive(Debug, Clone, Copy)]
struct PendingSceneRequest {
    key: ObsEventKey,
    retry_count: u8,
    generation: u64,
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
            pending_scene_requests: HashMap::new(),
            scene_request_generation: 0,
            was_identified: false,
            identified_this_connection: false,
            last_disconnect: None,
        }
    }

    fn begin_connection(&mut self) {
        self.identified_this_connection = false;
        self.last_disconnect = None;
        self.pending_scene_requests.clear();
    }

    fn mark_identified(&mut self) {
        self.was_identified = true;
        self.identified_this_connection = true;
    }

    fn set_disconnect(&mut self, detail: impl Into<String>, expected: bool) {
        self.last_disconnect = Some(ObsDisconnect { detail: detail.into(), expected });
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

        self.scene_request_generation = self.scene_request_generation.wrapping_add(1);
        self.apply_scene_for_event(sink, key, 0, self.scene_request_generation).await?;

        let action = self.config.actions.get(key.config_key()).copied().unwrap_or_default();
        self.apply_action(sink, action).await
    }

    async fn retry_scene(
        &mut self,
        sink: &mut ObsSink,
        key: ObsEventKey,
        retry_count: u8,
        generation: u64,
    ) -> Result<()> {
        if generation != self.scene_request_generation {
            return Ok(());
        }
        self.apply_scene_for_event(sink, key, retry_count, generation).await
    }

    async fn apply_scene_for_event(
        &mut self,
        sink: &mut ObsSink,
        key: ObsEventKey,
        retry_count: u8,
        generation: u64,
    ) -> Result<()> {
        let scene = self
            .config
            .scenes
            .get(key.config_key())
            .map(|scene| scene.trim())
            .filter(|scene| !scene.is_empty())
            .map(ToOwned::to_owned);
        if let Some(scene) = scene {
            let request_id = self.next_request_id("SetCurrentProgramScene");
            self.pending_scene_requests
                .insert(request_id.clone(), PendingSceneRequest { key, retry_count, generation });
            send_json(
                sink,
                request_message(
                    "SetCurrentProgramScene",
                    &request_id,
                    Some(json!({ "sceneName": scene })),
                ),
            )
            .await?;
        }
        Ok(())
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

    async fn handle_response(
        &mut self,
        sink: &mut ObsSink,
        data: &Value,
        status_tx: &watch::Sender<ObsConnectionStatus>,
        tx: mpsc::UnboundedSender<ObsCommand>,
    ) -> Result<()> {
        let request_type = data.get("requestType").and_then(Value::as_str).unwrap_or_default();
        let request_id = data.get("requestId").and_then(Value::as_str).unwrap_or_default();
        let scene_request = self.pending_scene_requests.remove(request_id);
        let status = data.get("requestStatus").unwrap_or(&Value::Null);
        if !status.get("result").and_then(Value::as_bool).unwrap_or(false) {
            let code = status.get("code").and_then(Value::as_i64).unwrap_or_default();
            let comment = status.get("comment").and_then(Value::as_str).unwrap_or_default();
            if code == 207
                && request_type == "SetCurrentProgramScene"
                && let Some(scene_request) = scene_request
            {
                if scene_request.generation != self.scene_request_generation {
                    tracing::debug!(request_type, code, "ignoring stale OBS scene request");
                    return Ok(());
                }
                if scene_request.retry_count < MAX_SCENE_NOT_READY_RETRIES {
                    let retry_count = scene_request.retry_count + 1;
                    let retry_delay = scene_not_ready_retry_delay(retry_count);
                    let retry_in_ms = retry_delay.as_millis() as u64;
                    publish_status(
                        status_tx,
                        ObsConnectionStatus::new(
                            ObsConnectionStatusKind::Connected,
                            Some(format!(
                                "OBS のシーン切替準備を待機中です。{retry_in_ms} ms 後に再試行します。"
                            )),
                            None,
                            Some(retry_in_ms),
                        ),
                    );
                    tracing::debug!(
                        request_type,
                        code,
                        retry_count,
                        retry_in_ms,
                        "OBS is not ready; retrying scene request"
                    );
                    tokio::spawn(async move {
                        tokio::time::sleep(retry_delay).await;
                        let _ = tx.send(ObsCommand::RetryScene {
                            key: scene_request.key,
                            retry_count,
                            generation: scene_request.generation,
                        });
                    });
                    return Ok(());
                }
            }

            if code == 207 && request_type != "SetCurrentProgramScene" {
                publish_status(
                    status_tx,
                    ObsConnectionStatus::new(
                        ObsConnectionStatusKind::Connected,
                        Some("OBS の準備完了を待機しています。".to_string()),
                        None,
                        None,
                    ),
                );
                tracing::debug!(request_type, code, comment, "OBS is not ready for request");
                return Ok(());
            }
            let error = if comment.is_empty() {
                format!("{request_type} が OBS に拒否されました (code {code})")
            } else {
                format!("{request_type}: {comment} (code {code})")
            };
            publish_status(
                status_tx,
                ObsConnectionStatus::new(
                    ObsConnectionStatusKind::Connected,
                    Some("OBS は接続済みですが、要求が拒否されました。".to_string()),
                    Some(error),
                    None,
                ),
            );
            tracing::error!(kind = "request", request_type, code, comment, "OBS request failed");
            return Ok(());
        }
        publish_status(
            status_tx,
            ObsConnectionStatus::new(ObsConnectionStatusKind::Connected, None, None, None),
        );
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
                self.set_disconnect("OBS が終了しました。", true);
                Ok(ConnectionAction::Reconnect)
            }
            "AuthenticationFailure" | "AuthenticationFailed" => Ok(ConnectionAction::Pause {
                kind: ObsConnectionStatusKind::AuthenticationFailed,
                detail: "OBS WebSocket 認証に失敗しました。パスワードを確認して保存してください。"
                    .to_string(),
            }),
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

#[derive(Debug, Clone, PartialEq, Eq)]
enum ConnectionAction {
    Continue,
    Reconnect,
    Shutdown,
    Pause { kind: ObsConnectionStatusKind, detail: String },
}

pub async fn load_scenes(config: ObsConfig) -> Result<ObsSceneList> {
    let url = obs_ws_url(&config);
    let timeout = tokio::time::sleep(LOAD_SCENES_TIMEOUT);
    tokio::pin!(timeout);
    let (ws, _) = tokio::select! {
        _ = &mut timeout => bail!("OBS scene load timed out"),
        result = connect_async(&url) => result.with_context(|| format!("failed to connect {url}"))?,
    };
    let (mut sink, mut stream) = ws.split();
    let mut state = ObsConnectionState::new(config);
    let mut version = None;
    let mut scenes = None;
    let mut recording_active = None;

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
    status_tx: watch::Sender<ObsConnectionStatus>,
) {
    let mut reconnect_delay = INITIAL_RECONNECT_DELAY;
    // Keep recording retention and queued app events across WebSocket reconnects.
    let mut state = ObsConnectionState::new(config);
    let mut pending_commands = VecDeque::new();
    let mut last_reported_issue = None;
    loop {
        let url = obs_ws_url(&state.config);
        publish_status(
            &status_tx,
            ObsConnectionStatus::new(
                ObsConnectionStatusKind::Connecting,
                Some("OBS WebSocket へ接続しています。".to_string()),
                None,
                None,
            ),
        );
        match connect_with_pending_commands(&url, &mut rx, &mut pending_commands).await {
            Ok(Some(ws)) => {
                state.begin_connection();
                tracing::debug!(url, "OBS WebSocket transport connected");
                match run_connected(
                    &mut state,
                    ws,
                    &mut rx,
                    tx.clone(),
                    &mut pending_commands,
                    &status_tx,
                )
                .await
                {
                    Ok(ConnectionAction::Shutdown) => break,
                    Ok(ConnectionAction::Pause { kind, detail }) => {
                        pause_reconnect(&status_tx, kind, &url, detail, state.was_identified);
                        wait_for_shutdown(&mut rx).await;
                        break;
                    }
                    Ok(ConnectionAction::Reconnect | ConnectionAction::Continue) => {
                        if state.identified_this_connection {
                            reconnect_delay = INITIAL_RECONNECT_DELAY;
                            last_reported_issue = None;
                        }
                        let disconnect = state.last_disconnect.take().unwrap_or(ObsDisconnect {
                            detail: "OBS WebSocket 接続が終了しました。".to_string(),
                            expected: false,
                        });
                        report_disconnect(
                            &status_tx,
                            &mut last_reported_issue,
                            &url,
                            &disconnect,
                            reconnect_delay,
                            state.was_identified,
                        );
                    }
                    Err(error) => {
                        if state.identified_this_connection {
                            reconnect_delay = INITIAL_RECONNECT_DELAY;
                            last_reported_issue = None;
                        }
                        state.set_disconnect(
                            format!("OBS WebSocket 接続中にエラーが発生しました: {error}"),
                            false,
                        );
                        let disconnect = state.last_disconnect.take().expect("disconnect was set");
                        report_disconnect(
                            &status_tx,
                            &mut last_reported_issue,
                            &url,
                            &disconnect,
                            reconnect_delay,
                            state.was_identified,
                        );
                    }
                }
            }
            Ok(None) => break,
            Err(error) => {
                if report_connect_failure(
                    &status_tx,
                    &mut last_reported_issue,
                    &url,
                    &error,
                    reconnect_delay,
                    state.was_identified,
                ) {
                    wait_for_shutdown(&mut rx).await;
                    break;
                }
            }
        }

        let should_shutdown =
            wait_for_reconnect_or_shutdown(&mut rx, &mut pending_commands, reconnect_delay).await;
        if should_shutdown {
            break;
        }
        reconnect_delay = next_reconnect_delay(reconnect_delay);
    }
}

async fn connect_with_pending_commands(
    url: &str,
    rx: &mut mpsc::UnboundedReceiver<ObsCommand>,
    pending_commands: &mut VecDeque<ObsCommand>,
) -> std::result::Result<Option<ObsWebSocket>, TungsteniteError> {
    let connection = connect_async(url);
    tokio::pin!(connection);
    loop {
        tokio::select! {
            result = &mut connection => {
                return result.map(|(ws, _)| Some(ws));
            }
            command = rx.recv() => {
                match command {
                    Some(ObsCommand::Shutdown) | None => return Ok(None),
                    Some(command) => pending_commands.push_back(command),
                }
            }
        }
    }
}

async fn run_connected(
    state: &mut ObsConnectionState,
    ws: ObsWebSocket,
    rx: &mut mpsc::UnboundedReceiver<ObsCommand>,
    tx: mpsc::UnboundedSender<ObsCommand>,
    pending_commands: &mut VecDeque<ObsCommand>,
    status_tx: &watch::Sender<ObsConnectionStatus>,
) -> Result<ConnectionAction> {
    let (mut sink, mut stream) = ws.split();
    match wait_for_identified(state, &mut sink, &mut stream, rx, pending_commands, status_tx)
        .await?
    {
        ConnectionAction::Continue => {}
        action => return Ok(action),
    }
    match flush_pending_commands(state, &mut sink, tx.clone(), pending_commands).await? {
        ConnectionAction::Continue => {}
        action => {
            let _ = sink.send(Message::Close(None)).await;
            return Ok(action);
        }
    }
    loop {
        if let Some(deadline) = state.pending_stop_deadline {
            tokio::select! {
                _ = tokio::time::sleep_until(deadline) => {
                    state.flush_pending_stop(&mut sink).await?;
                }
                command = rx.recv() => {
                    if matches!(handle_command(state, &mut sink, tx.clone(), command).await?, ConnectionAction::Shutdown) {
                        let _ = sink.send(Message::Close(None)).await;
                        return Ok(ConnectionAction::Shutdown);
                    }
                }
                message = stream.next() => {
                    match handle_stream_message(state, &mut sink, message, status_tx, tx.clone()).await? {
                        ConnectionAction::Continue => {}
                        action => return Ok(action),
                    }
                }
            }
        } else {
            tokio::select! {
                command = rx.recv() => {
                    if matches!(handle_command(state, &mut sink, tx.clone(), command).await?, ConnectionAction::Shutdown) {
                        let _ = sink.send(Message::Close(None)).await;
                        return Ok(ConnectionAction::Shutdown);
                    }
                }
                message = stream.next() => {
                    match handle_stream_message(state, &mut sink, message, status_tx, tx.clone()).await? {
                        ConnectionAction::Continue => {}
                        action => return Ok(action),
                    }
                }
            }
        }
    }
}

async fn wait_for_identified(
    state: &mut ObsConnectionState,
    sink: &mut ObsSink,
    stream: &mut futures_util::stream::SplitStream<ObsWebSocket>,
    rx: &mut mpsc::UnboundedReceiver<ObsCommand>,
    pending_commands: &mut VecDeque<ObsCommand>,
    status_tx: &watch::Sender<ObsConnectionStatus>,
) -> Result<ConnectionAction> {
    loop {
        tokio::select! {
            command = rx.recv() => {
                match command {
                    Some(ObsCommand::Shutdown) | None => return Ok(ConnectionAction::Shutdown),
                    Some(command) => pending_commands.push_back(command),
                }
            }
            message = stream.next() => {
                let Some(message) = message else {
                    state.set_disconnect("OBS WebSocket が識別前に切断されました。", false);
                    return Ok(ConnectionAction::Reconnect);
                };
                match message? {
                    Message::Text(text) => {
                        let value: Value = serde_json::from_str(text.as_ref())
                            .context("failed to parse OBS message")?;
                        let data = value.get("d").unwrap_or(&Value::Null);
                        match value.get("op").and_then(Value::as_i64).unwrap_or(-1) {
                            0 => send_json(sink, identify_message(&state.config, data)).await?,
                            2 => {
                                state.mark_identified();
                                publish_status(
                                    status_tx,
                                    ObsConnectionStatus::new(
                                        ObsConnectionStatusKind::Connected,
                                        Some("OBS WebSocket に接続しました。".to_string()),
                                        None,
                                        None,
                                    ),
                                );
                                state.send_request(sink, "GetVersion", None).await?;
                                state.send_request(sink, "GetSceneList", None).await?;
                                state.send_request(sink, "GetRecordStatus", None).await?;
                                return Ok(ConnectionAction::Continue);
                            }
                            _ => {}
                        }
                    }
                    Message::Ping(payload) => sink.send(Message::Pong(payload)).await?,
                    Message::Close(close) => return Ok(connection_action_for_close(state, close)),
                    _ => {}
                }
            }
        }
    }
}

async fn flush_pending_commands(
    state: &mut ObsConnectionState,
    sink: &mut ObsSink,
    tx: mpsc::UnboundedSender<ObsCommand>,
    pending_commands: &mut VecDeque<ObsCommand>,
) -> Result<ConnectionAction> {
    while let Some(command) = pending_commands.pop_front() {
        let action = handle_command(state, sink, tx.clone(), Some(command)).await?;
        if !matches!(action, ConnectionAction::Continue) {
            return Ok(action);
        }
    }
    Ok(ConnectionAction::Continue)
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
        ObsCommand::RetryScene { key, retry_count, generation } => {
            state.retry_scene(sink, key, retry_count, generation).await?
        }
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
    status_tx: &watch::Sender<ObsConnectionStatus>,
    tx: mpsc::UnboundedSender<ObsCommand>,
) -> Result<ConnectionAction> {
    let Some(message) = message else {
        state.set_disconnect("OBS WebSocket が切断されました。", false);
        return Ok(ConnectionAction::Reconnect);
    };
    match message? {
        Message::Text(text) => handle_text_message(state, sink, text.as_ref(), status_tx, tx).await,
        Message::Ping(payload) => {
            sink.send(Message::Pong(payload)).await?;
            Ok(ConnectionAction::Continue)
        }
        Message::Close(close) => Ok(connection_action_for_close(state, close)),
        _ => Ok(ConnectionAction::Continue),
    }
}

async fn handle_text_message(
    state: &mut ObsConnectionState,
    sink: &mut ObsSink,
    text: &str,
    status_tx: &watch::Sender<ObsConnectionStatus>,
    tx: mpsc::UnboundedSender<ObsCommand>,
) -> Result<ConnectionAction> {
    let value: Value = serde_json::from_str(text).context("failed to parse OBS message")?;
    let data = value.get("d").unwrap_or(&Value::Null);
    match value.get("op").and_then(Value::as_i64).unwrap_or(-1) {
        0 => {
            send_json(sink, identify_message(&state.config, data)).await?;
            Ok(ConnectionAction::Continue)
        }
        2 => {
            state.mark_identified();
            publish_status(
                status_tx,
                ObsConnectionStatus::new(
                    ObsConnectionStatusKind::Connected,
                    Some("OBS WebSocket に接続しました。".to_string()),
                    None,
                    None,
                ),
            );
            state.send_request(sink, "GetVersion", None).await?;
            state.send_request(sink, "GetSceneList", None).await?;
            state.send_request(sink, "GetRecordStatus", None).await?;
            Ok(ConnectionAction::Continue)
        }
        5 => state.handle_event(sink, data).await,
        7 => {
            state.handle_response(sink, data, status_tx, tx).await?;
            Ok(ConnectionAction::Continue)
        }
        _ => Ok(ConnectionAction::Continue),
    }
}

async fn wait_for_reconnect_or_shutdown(
    rx: &mut mpsc::UnboundedReceiver<ObsCommand>,
    pending_commands: &mut VecDeque<ObsCommand>,
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
                    Some(command) => pending_commands.push_back(command),
                }
            }
        }
    }
}

fn next_reconnect_delay(delay: Duration) -> Duration {
    let next = delay.mul_f64(RECONNECT_MULTIPLIER);
    next.min(MAX_RECONNECT_DELAY)
}

fn scene_not_ready_retry_delay(retry_count: u8) -> Duration {
    Duration::from_millis(250 * u64::from(retry_count.min(4)))
}

fn publish_status(status_tx: &watch::Sender<ObsConnectionStatus>, status: ObsConnectionStatus) {
    status_tx.send_replace(status);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ObsConnectionFailureKind {
    ServerUnavailable,
    Network,
    Tls,
    Handshake,
    Configuration,
}

impl ObsConnectionFailureKind {
    fn log_kind(self) -> &'static str {
        match self {
            Self::ServerUnavailable => "server_unavailable",
            Self::Network => "network",
            Self::Tls => "tls",
            Self::Handshake => "handshake",
            Self::Configuration => "configuration",
        }
    }
}

fn classify_connection_failure(error: &TungsteniteError) -> ObsConnectionFailureKind {
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

fn report_connect_failure(
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

fn report_disconnect(
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

fn pause_reconnect(
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

fn obs_status_log_kind(kind: ObsConnectionStatusKind) -> &'static str {
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

fn connection_action_for_close(
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

async fn wait_for_shutdown(rx: &mut mpsc::UnboundedReceiver<ObsCommand>) {
    while !matches!(rx.recv().await, Some(ObsCommand::Shutdown) | None) {}
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
    use tokio::net::TcpListener;
    use tokio_tungstenite::accept_async;

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

    #[test]
    fn connection_refused_is_treated_as_waiting_for_obs() {
        let error = TungsteniteError::Io(std::io::Error::new(
            ErrorKind::ConnectionRefused,
            "OBS is not running",
        ));
        let (status_tx, status_rx) = watch::channel(ObsConnectionStatus::default());
        let mut last_reported_issue = None;

        assert_eq!(
            classify_connection_failure(&error),
            ObsConnectionFailureKind::ServerUnavailable
        );
        assert!(!report_connect_failure(
            &status_tx,
            &mut last_reported_issue,
            "ws://localhost:4455",
            &error,
            INITIAL_RECONNECT_DELAY,
            false,
        ));
        assert_eq!(status_rx.borrow().kind, ObsConnectionStatusKind::WaitingForServer);
        assert!(status_rx.borrow().last_error.is_none());
    }

    #[test]
    fn authentication_close_pauses_reconnects() {
        let mut state = ObsConnectionState::new(ObsConfig::default());
        let action = connection_action_for_close(
            &mut state,
            Some(CloseFrame {
                code: CloseCode::Library(4009),
                reason: "authentication failed".into(),
            }),
        );

        assert!(matches!(
            action,
            ConnectionAction::Pause { kind: ObsConnectionStatusKind::AuthenticationFailed, .. }
        ));
    }

    #[tokio::test]
    async fn pending_events_wait_for_identified_before_sending_requests() -> Result<()> {
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let address = listener.local_addr()?;
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await?;
            let ws = accept_async(stream).await?;
            let (mut sink, mut stream) = ws.split();

            sink.send(Message::Text(json!({ "op": 0, "d": {} }).to_string())).await?;
            let identify = stream.next().await.context("OBS client closed before Identify")??;
            let Message::Text(identify) = identify else {
                bail!("expected Identify text message");
            };
            let identify: Value = serde_json::from_str(identify.as_ref())?;
            assert_eq!(identify["op"], 1);

            sink.send(Message::Text(json!({ "op": 2, "d": {} }).to_string())).await?;
            let mut requests = Vec::new();
            for _ in 0..4 {
                let message = stream.next().await.context("OBS client closed before request")??;
                let Message::Text(message) = message else {
                    bail!("expected OBS request text message");
                };
                let message: Value = serde_json::from_str(message.as_ref())?;
                requests.push(
                    message["d"]["requestType"]
                        .as_str()
                        .context("OBS request type missing")?
                        .to_string(),
                );
            }
            assert_eq!(
                requests,
                ["GetVersion", "GetSceneList", "GetRecordStatus", "SetCurrentProgramScene"]
            );

            sink.send(Message::Close(None)).await?;
            Ok::<(), anyhow::Error>(())
        });

        let (client, _) = connect_async(format!("ws://{address}")).await?;
        let mut config = ObsConfig::default();
        config
            .scenes
            .insert(ObsEventKey::MusicSelect.config_key().to_string(), "Select".to_string());
        let mut state = ObsConnectionState::new(config);
        let (tx, mut rx) = mpsc::unbounded_channel();
        tx.send(ObsCommand::ApplyEvent(ObsEventKey::MusicSelect))?;
        let mut pending_commands = VecDeque::new();
        let (status_tx, _) = watch::channel(ObsConnectionStatus::default());

        assert_eq!(
            run_connected(&mut state, client, &mut rx, tx, &mut pending_commands, &status_tx,)
                .await?,
            ConnectionAction::Reconnect
        );
        server.await??;
        Ok(())
    }

    #[tokio::test]
    async fn scene_request_retries_when_obs_is_temporarily_not_ready() -> Result<()> {
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let address = listener.local_addr()?;
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await?;
            let ws = accept_async(stream).await?;
            let (mut sink, mut stream) = ws.split();

            sink.send(Message::Text(json!({ "op": 0, "d": {} }).to_string())).await?;
            let identify = stream.next().await.context("OBS client closed before Identify")??;
            assert!(matches!(identify, Message::Text(_)));

            sink.send(Message::Text(json!({ "op": 2, "d": {} }).to_string())).await?;
            let mut scene_request_id = None;
            for _ in 0..4 {
                let message = stream.next().await.context("OBS client closed before request")??;
                let Message::Text(message) = message else {
                    bail!("expected OBS request text message");
                };
                let message: Value = serde_json::from_str(message.as_ref())?;
                if message["d"]["requestType"] == "SetCurrentProgramScene" {
                    scene_request_id = message["d"]["requestId"].as_str().map(ToOwned::to_owned);
                }
            }
            let scene_request_id = scene_request_id.context("scene request was not sent")?;
            sink.send(Message::Text(
                json!({
                    "op": 7,
                    "d": {
                        "requestType": "SetCurrentProgramScene",
                        "requestId": scene_request_id,
                        "requestStatus": {
                            "result": false,
                            "code": 207,
                            "comment": "OBS is not ready to perform the request."
                        }
                    }
                })
                .to_string(),
            ))
            .await?;

            let retry = tokio::time::timeout(Duration::from_secs(2), stream.next())
                .await
                .context("scene retry timed out")?
                .context("OBS client closed before scene retry")??;
            let Message::Text(retry) = retry else {
                bail!("expected retried scene request text message");
            };
            let retry: Value = serde_json::from_str(retry.as_ref())?;
            assert_eq!(retry["d"]["requestType"], "SetCurrentProgramScene");
            assert_ne!(retry["d"]["requestId"], scene_request_id);

            sink.send(Message::Text(
                json!({
                    "op": 7,
                    "d": {
                        "requestType": "SetCurrentProgramScene",
                        "requestId": retry["d"]["requestId"],
                        "requestStatus": { "result": true, "code": 100 }
                    }
                })
                .to_string(),
            ))
            .await?;
            sink.send(Message::Close(None)).await?;
            Ok::<(), anyhow::Error>(())
        });

        let (client, _) = connect_async(format!("ws://{address}")).await?;
        let mut config = ObsConfig::default();
        config
            .scenes
            .insert(ObsEventKey::MusicSelect.config_key().to_string(), "Select".to_string());
        let mut state = ObsConnectionState::new(config);
        let (tx, mut rx) = mpsc::unbounded_channel();
        tx.send(ObsCommand::ApplyEvent(ObsEventKey::MusicSelect))?;
        let mut pending_commands = VecDeque::new();
        let (status_tx, status_rx) = watch::channel(ObsConnectionStatus::default());

        assert_eq!(
            run_connected(&mut state, client, &mut rx, tx, &mut pending_commands, &status_tx,)
                .await?,
            ConnectionAction::Reconnect
        );
        server.await??;
        assert_eq!(status_rx.borrow().kind, ObsConnectionStatusKind::Connected);
        assert!(status_rx.borrow().last_error.is_none());
        Ok(())
    }

    #[tokio::test]
    async fn reconnect_wait_preserves_pending_commands() {
        let (tx, mut rx) = mpsc::unbounded_channel();
        tx.send(ObsCommand::ApplyEvent(ObsEventKey::Play)).unwrap();
        let mut pending_commands = VecDeque::new();

        let shutdown = wait_for_reconnect_or_shutdown(
            &mut rx,
            &mut pending_commands,
            Duration::from_millis(1),
        )
        .await;

        assert!(!shutdown);
        assert!(matches!(
            pending_commands.pop_front(),
            Some(ObsCommand::ApplyEvent(ObsEventKey::Play))
        ));
    }
}

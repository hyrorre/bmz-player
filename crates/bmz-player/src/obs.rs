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

use crate::config::app_config::{ObsActionConfig, ObsConfig, ObsRecordingMode, ObsTrigger};

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
    lifecycle: lifecycle::ObsLifecycle,
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

mod client;
mod lifecycle;
mod protocol;
mod state;

pub use client::*;
use protocol::*;

#[cfg(test)]
#[path = "obs/tests.rs"]
mod tests;

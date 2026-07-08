use std::io;
use std::process;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use serde::Serialize;
use serde_json::json;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::mpsc::{self, UnboundedSender};

use crate::config::app_config::{DEFAULT_DISCORD_APPLICATION_ID, DiscordConfig};

const OPCODE_HANDSHAKE: u32 = 0;
const OPCODE_FRAME: u32 = 1;
const DISCORD_FIELD_MAX_CHARS: usize = 128;

static NEXT_NONCE: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscordPresenceConfig {
    application_id: String,
    large_image_key: String,
    large_image_text: String,
    show_song_details: bool,
}

impl DiscordPresenceConfig {
    pub fn from_app_config(config: &DiscordConfig) -> Option<Self> {
        if !config.enabled {
            return None;
        }
        let application_id = effective_application_id(&config.application_id);
        Some(Self {
            application_id,
            large_image_key: config.large_image_key.trim().to_string(),
            large_image_text: config.large_image_text.trim().to_string(),
            show_song_details: config.show_song_details,
        })
    }

    pub fn show_song_details(&self) -> bool {
        self.show_song_details
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscordPresence {
    pub state: String,
    pub details: Option<String>,
    pub started_at_unix_seconds: i64,
}

impl DiscordPresence {
    pub fn new(
        state: impl Into<String>,
        details: Option<String>,
        started_at_unix_seconds: i64,
    ) -> Self {
        Self { state: state.into(), details, started_at_unix_seconds }
    }

    pub fn select(started_at_unix_seconds: i64) -> Self {
        Self::new("In Music Select Menu", None, started_at_unix_seconds)
    }

    pub fn decide(started_at_unix_seconds: i64) -> Self {
        Self::new("Decide Screen", None, started_at_unix_seconds)
    }

    pub fn play(
        started_at_unix_seconds: i64,
        key_mode: Option<&str>,
        title: Option<&str>,
        artist: Option<&str>,
        show_song_details: bool,
    ) -> Self {
        let state = key_mode
            .filter(|value| !value.trim().is_empty())
            .map(|value| format!("Playing: {}", value.trim()))
            .unwrap_or_else(|| "Playing".to_string());
        let details = show_song_details.then(|| song_details(title, artist)).flatten();
        Self::new(state, details, started_at_unix_seconds)
    }

    pub fn result(started_at_unix_seconds: i64) -> Self {
        Self::new("Result Screen", None, started_at_unix_seconds)
    }

    pub fn course_result(started_at_unix_seconds: i64) -> Self {
        Self::new("Course Result Screen", None, started_at_unix_seconds)
    }
}

#[derive(Debug)]
pub struct DiscordPresenceHandle {
    tx: UnboundedSender<DiscordPresenceCommand>,
}

impl DiscordPresenceHandle {
    pub fn start(config: DiscordPresenceConfig) -> Self {
        let (tx, rx) = mpsc::unbounded_channel();
        tokio::spawn(discord_presence_worker(config, rx));
        Self { tx }
    }

    pub fn update(&self, presence: DiscordPresence) {
        let _ = self.tx.send(DiscordPresenceCommand::Update(presence));
    }

    pub fn clear(&self) {
        let _ = self.tx.send(DiscordPresenceCommand::Clear);
    }

    pub fn shutdown(&self) {
        let _ = self.tx.send(DiscordPresenceCommand::Shutdown);
    }
}

impl Drop for DiscordPresenceHandle {
    fn drop(&mut self) {
        let _ = self.tx.send(DiscordPresenceCommand::Shutdown);
    }
}

#[derive(Debug)]
enum DiscordPresenceCommand {
    Update(DiscordPresence),
    Clear,
    Shutdown,
}

async fn discord_presence_worker(
    config: DiscordPresenceConfig,
    mut rx: mpsc::UnboundedReceiver<DiscordPresenceCommand>,
) {
    let mut client = DiscordRpcClient::new(config);
    let mut last_sent: Option<DiscordPresence> = None;
    while let Some(command) = rx.recv().await {
        match command {
            DiscordPresenceCommand::Update(presence) => {
                if last_sent.as_ref() == Some(&presence) {
                    continue;
                }
                match client.update(&presence).await {
                    Ok(()) => last_sent = Some(presence),
                    Err(error) => {
                        tracing::debug!(%error, "failed to update Discord Rich Presence");
                    }
                }
            }
            DiscordPresenceCommand::Clear => {
                last_sent = None;
                if let Err(error) = client.clear().await {
                    tracing::debug!(%error, "failed to clear Discord Rich Presence");
                }
            }
            DiscordPresenceCommand::Shutdown => {
                if let Err(error) = client.clear().await {
                    tracing::debug!(%error, "failed to clear Discord Rich Presence on shutdown");
                }
                break;
            }
        }
    }
}

struct DiscordRpcClient {
    config: DiscordPresenceConfig,
    connection: Option<DiscordRpcConnection>,
}

impl DiscordRpcClient {
    fn new(config: DiscordPresenceConfig) -> Self {
        Self { config, connection: None }
    }

    async fn update(&mut self, presence: &DiscordPresence) -> io::Result<()> {
        let payload = build_set_activity_payload(presence, &self.config)?;
        self.send_payload(payload).await
    }

    async fn clear(&mut self) -> io::Result<()> {
        if self.connection.is_none() {
            return Ok(());
        }
        let payload = build_clear_activity_payload()?;
        self.send_payload(payload).await
    }

    async fn send_payload(&mut self, payload: String) -> io::Result<()> {
        let mut last_error = None;
        for _ in 0..2 {
            if self.connection.is_none() {
                match DiscordRpcConnection::connect(&self.config.application_id).await {
                    Ok(connection) => self.connection = Some(connection),
                    Err(error) => {
                        last_error = Some(error);
                        break;
                    }
                }
            }
            let Some(connection) = &mut self.connection else {
                continue;
            };
            match connection.send_command(&payload).await {
                Ok(()) => return Ok(()),
                Err(error) => {
                    last_error = Some(error);
                    self.connection = None;
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
            }
        }
        Err(last_error.unwrap_or_else(|| io::Error::other("Discord IPC update failed")))
    }
}

struct DiscordRpcConnection {
    stream: DiscordIpcStream,
}

impl DiscordRpcConnection {
    async fn connect(application_id: &str) -> io::Result<Self> {
        let mut stream = DiscordIpcStream::connect().await?;
        let handshake = serde_json::to_string(&json!({
            "v": 1,
            "client_id": application_id,
        }))
        .map_err(io::Error::other)?;
        stream.send_packet(OPCODE_HANDSHAKE, &handshake).await?;
        let _ = stream.read_packet().await?;
        tracing::debug!("Discord RPC connected");
        Ok(Self { stream })
    }

    async fn send_command(&mut self, payload: &str) -> io::Result<()> {
        self.stream.send_packet(OPCODE_FRAME, payload).await?;
        let _ = self.stream.read_packet().await?;
        Ok(())
    }
}

enum DiscordIpcStream {
    #[cfg(windows)]
    Windows(tokio::net::windows::named_pipe::NamedPipeClient),
    #[cfg(unix)]
    Unix(tokio::net::UnixStream),
}

impl DiscordIpcStream {
    async fn connect() -> io::Result<Self> {
        let mut last_error = None;
        for path in discord_ipc_candidates() {
            match Self::connect_path(&path).await {
                Ok(stream) => return Ok(stream),
                Err(error) => last_error = Some(error),
            }
        }
        Err(last_error.unwrap_or_else(|| {
            io::Error::new(io::ErrorKind::NotFound, "Discord IPC socket was not found")
        }))
    }

    #[cfg(windows)]
    async fn connect_path(path: &str) -> io::Result<Self> {
        tokio::net::windows::named_pipe::ClientOptions::new().open(path).map(Self::Windows)
    }

    #[cfg(unix)]
    async fn connect_path(path: &str) -> io::Result<Self> {
        tokio::net::UnixStream::connect(path).await.map(Self::Unix)
    }

    #[cfg(not(any(windows, unix)))]
    async fn connect_path(_path: &str) -> io::Result<Self> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "Discord IPC is unsupported on this platform",
        ))
    }

    async fn send_packet(&mut self, opcode: u32, payload: &str) -> io::Result<()> {
        let packet = encode_packet(opcode, payload);
        self.write_all(&packet).await
    }

    async fn read_packet(&mut self) -> io::Result<(u32, Vec<u8>)> {
        let mut header = [0_u8; 8];
        self.read_exact(&mut header).await?;
        let opcode = u32::from_le_bytes(header[0..4].try_into().expect("header opcode"));
        let len = u32::from_le_bytes(header[4..8].try_into().expect("header len")) as usize;
        let mut payload = vec![0_u8; len];
        self.read_exact(&mut payload).await?;
        Ok((opcode, payload))
    }

    async fn write_all(&mut self, bytes: &[u8]) -> io::Result<()> {
        match self {
            #[cfg(windows)]
            Self::Windows(stream) => stream.write_all(bytes).await,
            #[cfg(unix)]
            Self::Unix(stream) => stream.write_all(bytes).await,
        }
    }

    async fn read_exact(&mut self, bytes: &mut [u8]) -> io::Result<()> {
        match self {
            #[cfg(windows)]
            Self::Windows(stream) => stream.read_exact(bytes).await.map(|_| ()),
            #[cfg(unix)]
            Self::Unix(stream) => stream.read_exact(bytes).await.map(|_| ()),
        }
    }
}

fn discord_ipc_candidates() -> Vec<String> {
    #[cfg(windows)]
    {
        (0..10).map(|index| format!(r"\\.\pipe\discord-ipc-{index}")).collect()
    }
    #[cfg(unix)]
    {
        let mut roots: Vec<String> = ["XDG_RUNTIME_DIR", "TMPDIR", "TMP", "TEMP"]
            .into_iter()
            .filter_map(std::env::var_os)
            .map(|value| value.to_string_lossy().into_owned())
            .collect();
        roots.push("/tmp".to_string());
        roots
            .into_iter()
            .flat_map(|root| {
                (0..10).map(move |index| {
                    format!("{}/discord-ipc-{}", root.trim_end_matches('/'), index)
                })
            })
            .collect()
    }
    #[cfg(not(any(windows, unix)))]
    {
        Vec::new()
    }
}

fn build_set_activity_payload(
    presence: &DiscordPresence,
    config: &DiscordPresenceConfig,
) -> io::Result<String> {
    let activity = Activity::from_presence(presence, config);
    serde_json::to_string(&SetActivityPayload {
        cmd: "SET_ACTIVITY",
        nonce: next_nonce(),
        args: ActivityArgs { pid: process::id(), activity: Some(activity) },
    })
    .map_err(io::Error::other)
}

fn build_clear_activity_payload() -> io::Result<String> {
    serde_json::to_string(&SetActivityPayload {
        cmd: "SET_ACTIVITY",
        nonce: next_nonce(),
        args: ActivityArgs { pid: process::id(), activity: None },
    })
    .map_err(io::Error::other)
}

fn encode_packet(opcode: u32, payload: &str) -> Vec<u8> {
    let payload = payload.as_bytes();
    let mut packet = Vec::with_capacity(8 + payload.len());
    packet.extend_from_slice(&opcode.to_le_bytes());
    packet.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    packet.extend_from_slice(payload);
    packet
}

fn next_nonce() -> String {
    let nonce = NEXT_NONCE.fetch_add(1, Ordering::Relaxed);
    format!("bmz-{}-{nonce}", process::id())
}

#[derive(Debug, Serialize)]
struct SetActivityPayload {
    cmd: &'static str,
    nonce: String,
    args: ActivityArgs,
}

#[derive(Debug, Serialize)]
struct ActivityArgs {
    pid: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    activity: Option<Activity>,
}

#[derive(Debug, Serialize)]
struct Activity {
    #[serde(skip_serializing_if = "Option::is_none")]
    state: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    details: Option<String>,
    timestamps: Timestamps,
    #[serde(skip_serializing_if = "Option::is_none")]
    assets: Option<Assets>,
    instance: bool,
}

impl Activity {
    fn from_presence(presence: &DiscordPresence, config: &DiscordPresenceConfig) -> Self {
        let assets = (!config.large_image_key.is_empty()).then(|| Assets {
            large_image: config.large_image_key.clone(),
            large_text: non_empty_string(&config.large_image_text),
        });
        Self {
            state: non_empty_string(&truncate_discord_field(&presence.state)),
            details: presence
                .details
                .as_deref()
                .map(truncate_discord_field)
                .and_then(|value| if value.is_empty() { None } else { Some(value) }),
            timestamps: Timestamps { start: presence.started_at_unix_seconds },
            assets,
            instance: true,
        }
    }
}

#[derive(Debug, Serialize)]
struct Timestamps {
    start: i64,
}

#[derive(Debug, Serialize)]
struct Assets {
    large_image: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    large_text: Option<String>,
}

fn song_details(title: Option<&str>, artist: Option<&str>) -> Option<String> {
    let title = title.map(str::trim).filter(|value| !value.is_empty());
    let artist = artist.map(str::trim).filter(|value| !value.is_empty());
    match (title, artist) {
        (Some(title), Some(artist)) => Some(format!("{title} / {artist}")),
        (Some(title), None) => Some(title.to_string()),
        (None, Some(artist)) => Some(artist.to_string()),
        (None, None) => None,
    }
}

fn non_empty_string(value: &str) -> Option<String> {
    let value = value.trim();
    (!value.is_empty()).then(|| truncate_discord_field(value))
}

fn truncate_discord_field(value: &str) -> String {
    value.chars().take(DISCORD_FIELD_MAX_CHARS).collect()
}

fn effective_application_id(configured_application_id: &str) -> String {
    let application_id = configured_application_id.trim();
    if application_id.is_empty() {
        DEFAULT_DISCORD_APPLICATION_ID.to_string()
    } else {
        application_id.to_string()
    }
}

#[cfg(test)]
mod tests {
    use serde_json::Value;

    use super::*;

    #[test]
    fn play_presence_matches_beatoraja_shape() {
        let presence =
            DiscordPresence::play(123, Some("7Keys"), Some("Song Title"), Some("Artist"), true);

        assert_eq!(presence.state, "Playing: 7Keys");
        assert_eq!(presence.details.as_deref(), Some("Song Title / Artist"));
        assert_eq!(presence.started_at_unix_seconds, 123);
    }

    #[test]
    fn play_presence_can_hide_song_details() {
        let presence =
            DiscordPresence::play(123, Some("14Keys"), Some("Song Title"), Some("Artist"), false);

        assert_eq!(presence.state, "Playing: 14Keys");
        assert_eq!(presence.details, None);
    }

    #[test]
    fn encode_packet_prefixes_little_endian_header() {
        let packet = encode_packet(OPCODE_FRAME, "{\"cmd\":\"SET_ACTIVITY\"}");

        assert_eq!(&packet[0..4], &OPCODE_FRAME.to_le_bytes());
        assert_eq!(&packet[4..8], &(22_u32).to_le_bytes());
        assert_eq!(&packet[8..], br#"{"cmd":"SET_ACTIVITY"}"#);
    }

    #[test]
    fn set_activity_payload_contains_expected_activity_fields() {
        let config = DiscordPresenceConfig {
            application_id: "app".to_string(),
            large_image_key: "bmz".to_string(),
            large_image_text: "BMZ Player".to_string(),
            show_song_details: true,
        };
        let presence =
            DiscordPresence::play(123, Some("7Keys"), Some("Song Title"), Some("Artist"), true);
        let value: Value =
            serde_json::from_str(&build_set_activity_payload(&presence, &config).unwrap()).unwrap();

        assert_eq!(value["cmd"], "SET_ACTIVITY");
        assert_eq!(value["args"]["pid"], process::id());
        assert_eq!(value["args"]["activity"]["state"], "Playing: 7Keys");
        assert_eq!(value["args"]["activity"]["details"], "Song Title / Artist");
        assert_eq!(value["args"]["activity"]["timestamps"]["start"], 123);
        assert_eq!(value["args"]["activity"]["assets"]["large_image"], "bmz");
        assert_eq!(value["args"]["activity"]["assets"]["large_text"], "BMZ Player");
        assert_eq!(value["args"]["activity"]["instance"], true);
    }

    #[test]
    fn clear_payload_uses_null_activity() {
        let value: Value = serde_json::from_str(&build_clear_activity_payload().unwrap()).unwrap();

        assert_eq!(value["cmd"], "SET_ACTIVITY");
        assert!(value["args"]["activity"].is_null());
    }

    #[test]
    fn config_requires_enabled_and_uses_builtin_application_id_by_default() {
        let mut config = DiscordConfig::default();
        assert_eq!(DiscordPresenceConfig::from_app_config(&config), None);

        config.enabled = true;
        let presence_config = DiscordPresenceConfig::from_app_config(&config).unwrap();
        assert_eq!(presence_config.application_id, DEFAULT_DISCORD_APPLICATION_ID);

        config.application_id = " 123 ".to_string();
        let presence_config = DiscordPresenceConfig::from_app_config(&config).unwrap();
        assert_eq!(presence_config.application_id, "123");
        assert!(presence_config.show_song_details);
    }
}

//! 外部BMSエディタが別プロセスで起動する `-P` / `-S` を、実行中ビューワーへ中継する。

use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

#[cfg(windows)]
const WINDOWS_PIPE_NAME: &str = r"\\.\pipe\bmz-player-viewer-v3";
#[cfg(not(windows))]
const LOOPBACK_ADDRESS: &str = "127.0.0.1:39078";
const MAX_COMMAND_BYTES: usize = 1024 * 1024;
const COMMAND_ACK: u8 = 0x06;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ViewerCommand {
    Stop,
    Play {
        path: PathBuf,
        measure: u32,
        #[serde(default)]
        battle: bool,
        #[serde(default)]
        play_overrides: crate::cli::PlayOverrides,
        #[serde(default)]
        window_overrides: crate::cli::WindowOverrides,
    },
    /// `-P --skip-result` で常駐viewerをone-shot起動へ入れ替えるための内部命令。
    Quit,
}

/// 実行中のビューワーへ停止要求を送る。未起動なら `false`。
pub fn request_stop() -> Result<bool> {
    request_command(&ViewerCommand::Stop)
}

/// 実行中のビューワーへ譜面再生要求を送る。未起動なら `false`。
pub fn request_play(path: &Path, measure: u32, battle: bool) -> Result<bool> {
    request_play_with_overrides(path, measure, battle, Default::default(), Default::default())
}

pub fn request_play_with_overrides(
    path: &Path,
    measure: u32,
    battle: bool,
    play_overrides: crate::cli::PlayOverrides,
    window_overrides: crate::cli::WindowOverrides,
) -> Result<bool> {
    request_command(&ViewerCommand::Play {
        path: path.to_path_buf(),
        measure,
        battle,
        play_overrides,
        window_overrides,
    })
}

/// 実行中のビューワーへプロセス終了要求を送る。one-shot起動への入れ替え専用。
pub fn request_quit() -> Result<bool> {
    request_command(&ViewerCommand::Quit)
}

fn request_command(command: &ViewerCommand) -> Result<bool> {
    platform::request_command(command)
}

/// プロセス終了まで複数のビューワー命令を受け取れるlistenerを開始する。
pub fn start_listener(on_command: impl Fn(ViewerCommand) + Send + 'static) -> Result<()> {
    platform::start_listener(on_command)
}

fn write_command(stream: &mut (impl Read + Write), command: &ViewerCommand) -> Result<()> {
    let payload = serde_json::to_vec(command).context("failed to encode viewer command")?;
    let payload_len = u32::try_from(payload.len()).context("viewer command is too large")?;
    stream.write_all(&payload_len.to_le_bytes())?;
    stream.write_all(&payload)?;
    stream.flush()?;
    let mut ack = [0_u8; 1];
    stream.read_exact(&mut ack).context("viewer listener closed without acknowledging command")?;
    if ack[0] != COMMAND_ACK {
        bail!("viewer listener returned an invalid acknowledgement");
    }
    Ok(())
}

fn read_command(stream: &mut impl Read) -> Result<ViewerCommand> {
    let mut payload_len = [0_u8; 4];
    stream.read_exact(&mut payload_len)?;
    let payload_len = u32::from_le_bytes(payload_len) as usize;
    if payload_len > MAX_COMMAND_BYTES {
        bail!("viewer command exceeds {MAX_COMMAND_BYTES} bytes");
    }
    let mut payload = vec![0_u8; payload_len];
    stream.read_exact(&mut payload)?;
    let command: ViewerCommand =
        serde_json::from_slice(&payload).context("failed to decode viewer command")?;
    if let ViewerCommand::Play { play_overrides, .. } = &command {
        play_overrides.validate()?;
    }
    Ok(command)
}

fn acknowledge_command(stream: &mut impl Write) -> Result<()> {
    stream.write_all(&[COMMAND_ACK])?;
    stream.flush()?;
    Ok(())
}

#[cfg(windows)]
mod platform {
    use std::fs::{File, OpenOptions};
    use std::os::windows::ffi::OsStrExt;
    use std::os::windows::io::{FromRawHandle, IntoRawHandle};
    use std::sync::mpsc;
    use std::time::Duration;

    use windows_sys::Win32::Foundation::{
        CloseHandle, ERROR_PIPE_CONNECTED, GetLastError, HANDLE, INVALID_HANDLE_VALUE,
    };
    use windows_sys::Win32::Storage::FileSystem::{
        FILE_FLAG_FIRST_PIPE_INSTANCE, PIPE_ACCESS_DUPLEX,
    };
    use windows_sys::Win32::System::Pipes::{
        ConnectNamedPipe, CreateNamedPipeW, DisconnectNamedPipe, PIPE_READMODE_BYTE,
        PIPE_TYPE_BYTE, PIPE_WAIT,
    };

    use super::*;

    const ERROR_PIPE_BUSY_CODE: i32 = 231;
    const PIPE_CONNECT_RETRIES: usize = 20;

    struct OwnedPipeHandle(HANDLE);

    impl Drop for OwnedPipeHandle {
        fn drop(&mut self) {
            unsafe {
                CloseHandle(self.0);
            }
        }
    }

    pub(super) fn request_command(command: &ViewerCommand) -> Result<bool> {
        for attempt in 0..PIPE_CONNECT_RETRIES {
            match OpenOptions::new().read(true).write(true).open(WINDOWS_PIPE_NAME) {
                Ok(mut pipe) => {
                    write_command(&mut pipe, command)
                        .context("failed to send command to the BMZ viewer pipe")?;
                    return Ok(true);
                }
                Err(error) if error.raw_os_error() == Some(ERROR_PIPE_BUSY_CODE) => {
                    if attempt + 1 == PIPE_CONNECT_RETRIES {
                        return Err(error).context("BMZ viewer pipe remained busy");
                    }
                    std::thread::sleep(Duration::from_millis(10));
                }
                Err(error)
                    if matches!(
                        error.kind(),
                        io::ErrorKind::NotFound | io::ErrorKind::ConnectionRefused
                    ) =>
                {
                    return Ok(false);
                }
                Err(error) => {
                    return Err(error).context("failed to connect to the BMZ viewer pipe");
                }
            }
        }
        Ok(false)
    }

    pub(super) fn start_listener(
        on_command: impl Fn(ViewerCommand) + Send + 'static,
    ) -> Result<()> {
        let (ready_tx, ready_rx) = mpsc::sync_channel(1);
        std::thread::Builder::new()
            .name("viewer-command-listener".to_string())
            .spawn(move || {
                let name: Vec<u16> = std::ffi::OsStr::new(WINDOWS_PIPE_NAME)
                    .encode_wide()
                    .chain(std::iter::once(0))
                    .collect();
                let handle = unsafe {
                    CreateNamedPipeW(
                        name.as_ptr(),
                        PIPE_ACCESS_DUPLEX | FILE_FLAG_FIRST_PIPE_INSTANCE,
                        PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT,
                        1,
                        MAX_COMMAND_BYTES as u32,
                        MAX_COMMAND_BYTES as u32,
                        0,
                        std::ptr::null(),
                    )
                };
                if handle == INVALID_HANDLE_VALUE {
                    let _ = ready_tx.send(Err(io::Error::last_os_error()));
                    return;
                }
                let _handle_guard = OwnedPipeHandle(handle);
                let _ = ready_tx.send(Ok(()));
                loop {
                    let connected = unsafe { ConnectNamedPipe(handle, std::ptr::null_mut()) } != 0;
                    let connected = connected || unsafe { GetLastError() } == ERROR_PIPE_CONNECTED;
                    if !connected {
                        tracing::warn!(error = %io::Error::last_os_error(), "viewer pipe connection failed");
                        continue;
                    }
                    let mut pipe = unsafe { File::from_raw_handle(handle) };
                    match read_command(&mut pipe) {
                        Ok(command) => {
                            on_command(command);
                            if let Err(error) = acknowledge_command(&mut pipe) {
                                tracing::warn!(%error, "failed to acknowledge viewer command");
                            }
                        }
                        Err(error) => tracing::warn!(%error, "failed to read viewer command"),
                    }
                    let handle = pipe.into_raw_handle();
                    unsafe {
                        DisconnectNamedPipe(handle);
                    }
                }
            })
            .context("failed to spawn the viewer command listener")?;
        ready_rx
            .recv()
            .context("viewer command listener exited before initialization")?
            .context("failed to create the BMZ viewer pipe")
    }
}

#[cfg(not(windows))]
mod platform {
    use std::net::{TcpListener, TcpStream};
    use std::sync::mpsc;

    use super::*;

    pub(super) fn request_command(command: &ViewerCommand) -> Result<bool> {
        match TcpStream::connect(LOOPBACK_ADDRESS) {
            Ok(mut stream) => {
                write_command(&mut stream, command)
                    .context("failed to send command to the BMZ viewer listener")?;
                Ok(true)
            }
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::ConnectionRefused | io::ErrorKind::NotFound
                ) =>
            {
                Ok(false)
            }
            Err(error) => Err(error).context("failed to connect to the BMZ viewer listener"),
        }
    }

    pub(super) fn start_listener(
        on_command: impl Fn(ViewerCommand) + Send + 'static,
    ) -> Result<()> {
        let listener = TcpListener::bind(LOOPBACK_ADDRESS)
            .context("failed to bind the BMZ viewer listener")?;
        let (ready_tx, ready_rx) = mpsc::sync_channel(1);
        std::thread::Builder::new()
            .name("viewer-command-listener".to_string())
            .spawn(move || {
                let _ = ready_tx.send(());
                for connection in listener.incoming() {
                    match connection {
                        Ok(mut stream) => match read_command(&mut stream) {
                            Ok(command) => {
                                on_command(command);
                                if let Err(error) = acknowledge_command(&mut stream) {
                                    tracing::warn!(%error, "failed to acknowledge viewer command");
                                }
                            }
                            Err(error) => tracing::warn!(%error, "failed to read viewer command"),
                        },
                        Err(error) => tracing::warn!(%error, "viewer listener connection failed"),
                    }
                }
            })
            .context("failed to spawn the viewer command listener")?;
        ready_rx.recv().context("viewer command listener exited before initialization")?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc;
    use std::time::Duration;

    use super::*;

    #[test]
    fn legacy_play_command_without_battle_defaults_to_normal_viewer() {
        let command: ViewerCommand =
            serde_json::from_str(r#"{"Play":{"path":"C:\\譜面\\_temp.bms","measure":12}}"#)
                .unwrap();

        assert_eq!(
            command,
            ViewerCommand::Play {
                play_overrides: Default::default(),
                window_overrides: Default::default(),
                path: PathBuf::from(r"C:\譜面\_temp.bms"),
                measure: 12,
                battle: false,
            }
        );
    }

    #[test]
    fn play_stop_and_quit_requests_round_trip_to_persistent_listener() {
        let (tx, rx) = mpsc::sync_channel(4);
        start_listener(move |command| tx.send(command).unwrap()).unwrap();

        let path = PathBuf::from(r"C:\譜面\_temp.bms");
        assert!(request_play(&path, 12, true).unwrap());
        assert_eq!(
            rx.recv_timeout(Duration::from_secs(2)).unwrap(),
            ViewerCommand::Play {
                path,
                measure: 12,
                battle: true,
                play_overrides: Default::default(),
                window_overrides: Default::default()
            }
        );

        assert!(request_stop().unwrap());
        assert_eq!(rx.recv_timeout(Duration::from_secs(2)).unwrap(), ViewerCommand::Stop);

        let replacement = PathBuf::from(r"C:\譜面\replacement.bms");
        assert!(request_play(&replacement, 3, false).unwrap());
        assert_eq!(
            rx.recv_timeout(Duration::from_secs(2)).unwrap(),
            ViewerCommand::Play {
                path: replacement,
                measure: 3,
                battle: false,
                play_overrides: Default::default(),
                window_overrides: Default::default()
            }
        );

        assert!(request_quit().unwrap());
        assert_eq!(rx.recv_timeout(Duration::from_secs(2)).unwrap(), ViewerCommand::Quit);
    }
}

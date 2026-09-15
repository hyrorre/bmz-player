//! Main-thread-only adapter. Non-bundled/development builds keep manual updates.
use super::UpdateCandidate;
#[cfg(any(bmz_sparkle, test))]
use super::{UpdateAsset, UpdateAssetKind};
use crate::config::app_config::UpdateChannelConfig;
use anyhow::Result;
use bmz_updater::process::RestartContext;

pub enum Event {
    Available(Box<UpdateCandidate>),
    Progress { received: u64, total: u64, extracting: bool },
    Ready,
    Error(String),
    UpToDate,
    Shutdown,
    Canceled,
}

#[cfg(bmz_sparkle)]
mod native {
    use std::ffi::{CStr, c_char};
    unsafe extern "C" {
        fn bmz_sparkle_available() -> bool;
        fn bmz_sparkle_check(prerelease: bool, report: bool);
        fn bmz_sparkle_action(action: i32);
        fn bmz_sparkle_poll() -> *const c_char;
        fn bmz_sparkle_finish();
    }
    pub(super) fn available() -> bool {
        unsafe { bmz_sparkle_available() }
    }
    pub(super) fn check(prerelease: bool, report: bool) {
        unsafe {
            bmz_sparkle_check(prerelease, report);
        }
    }
    pub(super) fn action(action: i32) {
        unsafe {
            bmz_sparkle_action(action);
        }
    }
    pub(super) fn poll() -> Option<serde_json::Value> {
        let ptr = unsafe { bmz_sparkle_poll() };
        if ptr.is_null() {
            return None;
        }
        serde_json::from_slice(unsafe { CStr::from_ptr(ptr) }.to_bytes()).ok()
    }
    pub(super) fn finish() {
        unsafe {
            bmz_sparkle_finish();
        }
    }
}

pub fn available() -> bool {
    #[cfg(bmz_sparkle)]
    {
        native::available()
    }
    #[cfg(not(bmz_sparkle))]
    {
        false
    }
}

pub fn check(channel: UpdateChannelConfig, report: bool) -> Result<()> {
    #[cfg(bmz_sparkle)]
    {
        native::check(channel == UpdateChannelConfig::Prerelease, report);
    }
    #[cfg(not(bmz_sparkle))]
    {
        let _ = (channel, report);
    }
    Ok(())
}

pub fn download() {
    #[cfg(bmz_sparkle)]
    {
        native::action(1);
    }
}
pub fn cancel() {
    #[cfg(bmz_sparkle)]
    {
        native::action(0);
    }
}
pub fn pause() {
    #[cfg(bmz_sparkle)]
    {
        native::action(3);
    }
}
pub fn install(context: &RestartContext) -> Result<()> {
    #[cfg(bmz_sparkle)]
    {
        save_restart(context)?;
        native::action(2);
    }
    #[cfg(not(bmz_sparkle))]
    {
        let _ = context;
    }
    Ok(())
}
pub fn finish_shutdown() {
    #[cfg(bmz_sparkle)]
    {
        native::finish();
    }
}
pub fn poll() -> Option<Event> {
    #[cfg(bmz_sparkle)]
    {
        native::poll().and_then(decode_event)
    }
    #[cfg(not(bmz_sparkle))]
    {
        None
    }
}

#[cfg(any(bmz_sparkle, test))]
fn decode_event(value: serde_json::Value) -> Option<Event> {
    Some(match value.get("event")?.as_str()? {
        "available" => {
            let version = value.get("version")?.as_str()?.to_owned();
            // Never interpret an appcast information-only item as an installable update.
            let installable = value.get("installable").and_then(|v| v.as_bool()).unwrap_or(false);
            Event::Available(Box::new(UpdateCandidate {
                tag: format!("v{version}"),
                title: format!("BMZ Player {version}"),
                html_url: format!("{}/tag/v{version}", super::RELEASES_PAGE_URL),
                body: String::new(),
                published_at: None,
                prerelease: version.contains('-'),
                asset: installable.then(|| UpdateAsset {
                    name: "BMZ Player.app".into(),
                    download_url: String::new(),
                    size: 0,
                    sha256: None,
                    kind: UpdateAssetKind::MacosAppZip,
                }),
                version,
            }))
        }
        "progress" => Event::Progress {
            received: value["received"].as_u64().unwrap_or(0),
            total: value["total"].as_u64().unwrap_or(0),
            extracting: value["extracting"].as_bool().unwrap_or(false),
        },
        "ready" => Event::Ready,
        "error" => {
            Event::Error(value["message"].as_str().unwrap_or("Sparkle update failed").into())
        }
        "current" => Event::UpToDate,
        "shutdown" => Event::Shutdown,
        "canceled" => Event::Canceled,
        _ => return None,
    })
}

#[cfg(bmz_sparkle)]
#[derive(serde::Serialize, serde::Deserialize)]
struct SavedRestart {
    executable: std::path::PathBuf,
    created: u64,
    context: RestartContext,
}

#[cfg(bmz_sparkle)]
fn restart_path() -> Result<std::path::PathBuf> {
    Ok(std::path::PathBuf::from(
        std::env::var_os("HOME").ok_or_else(|| anyhow::anyhow!("HOME unavailable"))?,
    )
    .join("Library/Caches/net.hyrorre.bmz-player/update-restart.json"))
}

#[cfg(bmz_sparkle)]
fn save_restart(context: &RestartContext) -> Result<()> {
    let path = restart_path()?;
    std::fs::create_dir_all(path.parent().unwrap())?;
    let saved = SavedRestart {
        executable: std::env::current_exe()?.canonicalize()?,
        created: std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)?.as_secs(),
        context: context.clone(),
    };
    bmz_updater::transaction::atomic_json(&path, &saved)
}

/// Consume only on an argument-free relaunch, bound to this exact installation for ten minutes.
pub fn take_restart() -> Option<RestartContext> {
    #[cfg(bmz_sparkle)]
    {
        let path = restart_path().ok()?;
        let meta = std::fs::symlink_metadata(&path).ok()?;
        if meta.file_type().is_symlink() || meta.len() > 64 * 1024 {
            return None;
        }
        let saved: SavedRestart = serde_json::from_slice(&std::fs::read(&path).ok()?).ok()?;
        if std::env::current_exe().ok()?.canonicalize().ok()? != saved.executable {
            return None;
        }
        std::fs::remove_file(path).ok()?;
        let now =
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).ok()?.as_secs();
        if now < saved.created || now - saved.created > 600 {
            return None;
        }
        Some(saved.context)
    }
    #[cfg(not(bmz_sparkle))]
    {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn informational_update_cannot_install() {
        let Event::Available(candidate) = decode_event(
            serde_json::json!({"event":"available", "version":"0.5.0", "installable":false}),
        )
        .unwrap() else {
            panic!()
        };
        assert!(candidate.asset.is_none());
    }
}

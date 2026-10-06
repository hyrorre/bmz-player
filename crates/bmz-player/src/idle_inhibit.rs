//! Desktop idle inhibition for the focused Linux window, independent of input/rendering.
use std::{collections::HashMap, thread, time::Duration};

use anyhow::{Context, Result, anyhow, bail};
use futures_util::StreamExt;
use tokio::sync::watch;
use zbus::{Connection, MatchRule, MessageStream, zvariant::OwnedObjectPath};

const PORTAL: &str = "org.freedesktop.portal.Desktop";
const SCREENSAVER: &str = "org.freedesktop.ScreenSaver";
const REQUEST: &str = "org.freedesktop.portal.Request";
const REASON: &str = "Playing BMZ Player";
const CALL_TIMEOUT: Duration = Duration::from_secs(2);
const RETRY_DELAY: Duration = Duration::from_secs(30);

pub(crate) struct IdleInhibitor {
    focused: Option<watch::Sender<bool>>,
    worker: Option<thread::JoinHandle<()>>,
}

impl IdleInhibitor {
    pub(crate) fn new(focused: bool) -> Option<Self> {
        let (tx, rx) = watch::channel(focused);
        let worker = thread::Builder::new().name("bmz-idle-inhibit".into()).spawn(move || {
            match tokio::runtime::Builder::new_current_thread().enable_all().build() {
                Ok(runtime) => runtime.block_on(run(rx, None)),
                Err(error) => tracing::warn!(%error, "failed to start desktop idle inhibition"),
            }
        });
        match worker {
            Ok(worker) => Some(Self { focused: Some(tx), worker: Some(worker) }),
            Err(error) => {
                tracing::warn!(%error, "failed to spawn desktop idle inhibition worker");
                None
            }
        }
    }

    pub(crate) fn set_focused(&self, focused: bool) {
        if let Some(tx) = &self.focused {
            tx.send_if_modified(|current| {
                if *current == focused {
                    return false;
                }
                *current = focused;
                true
            });
        }
    }
}

impl Drop for IdleInhibitor {
    fn drop(&mut self) {
        // Closing the channel also cancels an acquisition in flight. Join only
        // during teardown so the bus is disconnected before process exit.
        self.focused.take();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

#[derive(Clone, Copy, Debug)]
enum Backend {
    Portal,
    ScreenSaver(&'static str),
}

impl Backend {
    fn destination(self) -> &'static str {
        match self {
            Self::Portal => PORTAL,
            Self::ScreenSaver(_) => SCREENSAVER,
        }
    }
}

enum Token {
    Portal(OwnedObjectPath),
    ScreenSaver(u32),
}

struct Session {
    bus: Connection,
    backend: Backend,
    token: Option<Token>,
    owner: Option<String>,
}

impl Session {
    async fn release(self) {
        if let Some(token) = self.token {
            // A restarted service may reuse cookie numbers. Never release a
            // previous owner's token against the new owner of its well-known name.
            let destination = self.owner.as_deref();
            let release = async {
                match token {
                    Token::Portal(path) => {
                        self.bus
                            .call_method(destination, path, Some(REQUEST), "Close", &())
                            .await?;
                    }
                    Token::ScreenSaver(cookie) => {
                        let Backend::ScreenSaver(path) = self.backend else { unreachable!() };
                        self.bus
                            .call_method(
                                destination,
                                path,
                                Some(SCREENSAVER),
                                "UnInhibit",
                                &(cookie,),
                            )
                            .await?;
                    }
                }
                Ok::<_, zbus::Error>(())
            };
            if let Err(error) = bounded(release).await {
                tracing::debug!(%error, "idle inhibition release failed; disconnecting owner");
            }
        }
        // Each attempt owns a separate connection. Disconnect even after a
        // timeout/cancellation, when the service may have granted an unknown token.
        if let Err(error) = bounded(self.bus.close()).await {
            tracing::debug!(%error, "idle inhibition owner disconnect failed");
        }
        tracing::debug!(backend = ?self.backend, "desktop idle inhibition owner released");
    }
}

async fn bounded<T>(future: impl Future<Output = Result<T, zbus::Error>>) -> Result<T> {
    Ok(tokio::time::timeout(CALL_TIMEOUT, future).await.context("idle inhibition timed out")??)
}

async fn run(mut focused: watch::Receiver<bool>, address: Option<&str>) {
    loop {
        if focused.has_changed().is_err() {
            break;
        }
        if !*focused.borrow_and_update() {
            if focused.changed().await.is_err() {
                break;
            }
            continue;
        }

        // Keep the connection/token outside the cancellable future so every
        // focus change (including during Inhibit) goes through owner cleanup.
        let mut session = None;
        let failed = tokio::select! {
            biased;
            _ = focused.changed() => false,
            result = acquire_and_hold(&mut session, address) => {
                if let Err(error) = result {
                    tracing::warn!(error = %format!("{error:#}"), "desktop idle inhibition unavailable; retrying while focused");
                }
                true
            }
        };
        if let Some(session) = session {
            session.release().await;
        }
        if failed {
            tokio::select! {
                _ = focused.changed() => {},
                _ = tokio::time::sleep(RETRY_DELAY) => {},
            }
        }
    }
}

async fn acquire_and_hold(session: &mut Option<Session>, address: Option<&str>) -> Result<()> {
    for backend in [
        Backend::Portal,
        Backend::ScreenSaver("/org/freedesktop/ScreenSaver"),
        Backend::ScreenSaver("/ScreenSaver"),
    ] {
        let builder = match address {
            Some(address) => zbus::connection::Builder::address(address)?,
            None => zbus::connection::Builder::session()?,
        };
        let bus = bounded(builder.build()).await?;
        *session = Some(Session { bus, backend, token: None, owner: None });
        let current = session.as_mut().expect("session just initialized");
        let attempt = tokio::time::timeout(CALL_TIMEOUT, acquire(current)).await;
        match attempt {
            Ok(Ok((mut owners, owner))) => {
                tracing::info!(?backend, "desktop idle inhibition active");
                // Reacquire after a service restart or session bus disconnect.
                while let Some(message) = owners.next().await {
                    let message = message?;
                    let (_, _, new_owner): (String, String, String) =
                        message.body().deserialize()?;
                    if new_owner != owner {
                        bail!("idle inhibition service owner changed");
                    }
                }
                bail!("idle inhibition bus disconnected");
            }
            Ok(Err(error)) => {
                tracing::debug!(?backend, %error, "idle inhibition backend unavailable")
            }
            Err(error) => tracing::debug!(?backend, %error, "idle inhibition backend timed out"),
        }
        if let Some(previous) = session.take() {
            previous.release().await;
        }
    }
    bail!("neither the desktop portal nor ScreenSaver API accepted idle inhibition")
}

async fn acquire(session: &mut Session) -> Result<(MessageStream, String)> {
    let owner_rule = MatchRule::builder()
        .msg_type(zbus::message::Type::Signal)
        .sender("org.freedesktop.DBus")?
        .interface("org.freedesktop.DBus")?
        .member("NameOwnerChanged")?
        .add_arg(session.backend.destination())?
        .build();
    let owners = MessageStream::for_match_rule(owner_rule, &session.bus, Some(8)).await?;
    let reply = match session.backend {
        Backend::Portal => {
            let unique = session.bus.unique_name().context("missing session bus unique name")?;
            let path = format!(
                "/org/freedesktop/portal/desktop/request/{}/bmz_idle",
                unique.as_str().trim_start_matches(':').replace('.', "_")
            );
            let rule = MatchRule::builder()
                .msg_type(zbus::message::Type::Signal)
                .sender(PORTAL)?
                .path(path.as_str())?
                .interface(REQUEST)?
                .member("Response")?
                .build();
            // Subscribe before calling Inhibit: a fast Response may precede its reply.
            let mut responses = MessageStream::for_match_rule(rule, &session.bus, Some(1)).await?;
            let options = HashMap::from([
                ("handle_token", zbus::zvariant::Value::from("bmz_idle")),
                ("reason", zbus::zvariant::Value::from(REASON)),
            ]);
            let reply = session
                .bus
                .call_method(
                    Some(PORTAL),
                    "/org/freedesktop/portal/desktop",
                    Some("org.freedesktop.portal.Inhibit"),
                    "Inhibit",
                    &("", 8_u32, options),
                )
                .await?;
            session.owner = Some(reply_owner(&reply)?);
            let handle: OwnedObjectPath = reply.body().deserialize()?;
            let expected_handle = handle.as_str() == path;
            session.token = Some(Token::Portal(handle));
            anyhow::ensure!(expected_handle, "portal ignored the request handle token");
            let response = responses.next().await.context("portal response stream closed")??;
            let (status, _): (u32, HashMap<String, zbus::zvariant::OwnedValue>) =
                response.body().deserialize()?;
            anyhow::ensure!(status == 0, "portal denied idle inhibition (response {status})");
            reply
        }
        Backend::ScreenSaver(path) => {
            let reply = session
                .bus
                .call_method(
                    Some(SCREENSAVER),
                    path,
                    Some(SCREENSAVER),
                    "Inhibit",
                    &("BMZ Player", REASON),
                )
                .await?;
            session.owner = Some(reply_owner(&reply)?);
            session.token = Some(Token::ScreenSaver(reply.body().deserialize()?));
            reply
        }
    };
    Ok((owners, reply_owner(&reply)?))
}

fn reply_owner(reply: &zbus::Message) -> Result<String> {
    Ok(reply
        .header()
        .sender()
        .ok_or_else(|| anyhow!("missing inhibition service owner"))?
        .to_string())
}

#[cfg(test)]
mod tests;

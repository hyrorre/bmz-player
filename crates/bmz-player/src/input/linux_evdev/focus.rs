//! Independent native X11 focus query plus logind session-active check.
//! A short window-thread lease additionally bounds stale UI suppression state.
use std::{
    cell::Cell,
    io,
    os::{fd::AsRawFd, unix::net::UnixStream},
    time::{Duration, Instant},
};
use x11rb::{
    protocol::xproto::ConnectionExt,
    rust_connection::{DefaultStream, PollMode, RustConnection, Stream},
    utils::RawFdContainer,
};

/// Focus changes reach BMZ within this interval; winit focus events still wake at once.
const FOCUS_REFRESH: Duration = Duration::from_millis(20);
/// Consecutive failed queries (e.g. a busy system bus) before input is denied.
const FOCUS_FAILURES: u8 = 3;

struct BoundedStream {
    inner: DefaultStream,
    deadline: Cell<Instant>,
}
impl BoundedStream {
    fn remaining(&self) -> io::Result<Duration> {
        self.deadline
            .get()
            .checked_duration_since(Instant::now())
            .ok_or_else(|| io::Error::from(io::ErrorKind::TimedOut))
    }
}
impl Stream for BoundedStream {
    fn poll(&self, mode: PollMode) -> io::Result<()> {
        loop {
            let timeout = self.remaining()?.as_millis().clamp(1, 1000) as i32;
            let events = (if mode.readable() { libc::POLLIN } else { 0 })
                | (if mode.writable() { libc::POLLOUT } else { 0 });
            let mut fd = libc::pollfd { fd: self.inner.as_raw_fd(), events, revents: 0 };
            let result = unsafe { libc::poll(&mut fd, 1, timeout) };
            if result > 0 {
                return Ok(());
            }
            if result == 0 {
                return Err(io::ErrorKind::TimedOut.into());
            }
            let error = io::Error::last_os_error();
            if error.kind() != io::ErrorKind::Interrupted {
                return Err(error);
            }
        }
    }
    fn read(&self, bytes: &mut [u8], fds: &mut Vec<RawFdContainer>) -> io::Result<usize> {
        self.remaining()?;
        self.inner.read(bytes, fds)
    }
    fn write(&self, bytes: &[u8], fds: &mut Vec<RawFdContainer>) -> io::Result<usize> {
        self.remaining()?;
        self.inner.write(bytes, fds)
    }
}

pub(super) struct Guard {
    x: RustConnection<BoundedStream>,
    window: u32,
    bus: zbus::blocking::Connection,
    session: zbus::zvariant::OwnedObjectPath,
    cached: Option<(Instant, bool)>,
    failures: u8,
}

impl Guard {
    pub(super) fn new(window: u32) -> anyhow::Result<Self> {
        anyhow::ensure!(
            !std::path::Path::new("/.flatpak-info").exists(),
            "Flatpak evdev unavailable"
        );
        let display = std::env::var("DISPLAY")?;
        let (display_number, screen) = display
            .strip_prefix(':')
            .ok_or_else(|| anyhow::anyhow!("local X11 required"))?
            .split_once('.')
            .unwrap_or((display.trim_start_matches(':'), "0"));
        let display_number: u16 = display_number.parse()?;
        let screen: usize = screen.parse()?;
        // Read-only login1 properties; no TakeControl / TakeDevice / activation.
        let bus = zbus::blocking::connection::Builder::system()?
            .method_timeout(Duration::from_millis(5))
            .build()?;
        let session: zbus::zvariant::OwnedObjectPath = bus
            .call_method(
                Some("org.freedesktop.login1"),
                "/org/freedesktop/login1",
                Some("org.freedesktop.login1.Manager"),
                "GetSessionByPID",
                &(std::process::id(),),
            )?
            .body()
            .deserialize()?;
        let properties = session_properties(&bus, &session)?;
        let native_x11 =
            properties.get("Type").and_then(|v| <&str>::try_from(v).ok()) == Some("x11");
        anyhow::ensure!(native_x11, "native X11 session required");
        let unix = UnixStream::connect(format!("/tmp/.X11-unix/X{display_number}"))?;
        let (inner, (family, address)) = DefaultStream::from_unix_stream(unix)?;
        let (name, auth) =
            x11rb_protocol::xauth::get_auth(family, &address, display_number)?.unwrap_or_default();
        let stream =
            BoundedStream { inner, deadline: Cell::new(Instant::now() + Duration::from_secs(1)) };
        let x = RustConnection::connect_to_stream_with_auth_info(stream, screen, name, auth)?;
        anyhow::ensure!(
            !x.query_extension(b"XWAYLAND")?.reply()?.present,
            "XWayland evdev unavailable"
        );
        Ok(Self { x, window, bus, session, cached: None, failures: 0 })
    }
    /// Cached for FOCUS_REFRESH so event delivery does not wait on D-Bus/X11 round trips.
    /// A single timed-out query keeps the previous answer; repeated failures deny input.
    pub(super) fn focused(&mut self) -> bool {
        let now = Instant::now();
        if let Some((at, focused)) = self.cached
            && now.saturating_duration_since(at) < FOCUS_REFRESH
        {
            return focused;
        }
        let focused = match self.query() {
            Ok(focused) => {
                self.failures = 0;
                focused
            }
            Err(_) => {
                self.failures = self.failures.saturating_add(1);
                self.failures < FOCUS_FAILURES && self.cached.is_some_and(|(_, focused)| focused)
            }
        };
        self.cached = Some((now, focused));
        focused
    }
    fn query(&mut self) -> anyhow::Result<bool> {
        let properties = session_properties(&self.bus, &self.session)?;
        if properties.get("Active").and_then(|v| bool::try_from(v).ok()) != Some(true)
            || properties.get("LockedHint").and_then(|v| bool::try_from(v).ok()) != Some(false)
        {
            return Ok(false);
        }
        self.x.stream().deadline.set(Instant::now() + Duration::from_millis(5));
        Ok(self.x.get_input_focus()?.reply()?.focus == self.window)
    }
}

fn session_properties(
    bus: &zbus::blocking::Connection,
    path: &zbus::zvariant::OwnedObjectPath,
) -> anyhow::Result<std::collections::HashMap<String, zbus::zvariant::OwnedValue>> {
    Ok(bus
        .call_method(
            Some("org.freedesktop.login1"),
            path.as_str(),
            Some("org.freedesktop.DBus.Properties"),
            "GetAll",
            &("org.freedesktop.login1.Session",),
        )?
        .body()
        .deserialize()?)
}

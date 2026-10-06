//! Exercise the real wire protocol on a private bus, never the user's desktop bus.
use super::*;
use std::{
    io::{BufRead, BufReader},
    path::PathBuf,
    process::{Child, Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};
use tokio::sync::{Notify, mpsc};
use zbus::zvariant::{OwnedValue, Value};

struct PrivateBus {
    process: Child,
    config: PathBuf,
    address: String,
}

impl PrivateBus {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let config = std::env::temp_dir().join(format!(
            "bmz-idle-bus-{}-{}.conf",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        // No service directories: do not activate real portal/desktop processes.
        std::fs::write(
            &config,
            r#"<busconfig>
          <type>session</type><listen>unix:tmpdir=/tmp</listen><auth>EXTERNAL</auth>
          <policy context="default"><allow send_destination="*"/>
          <allow receive_sender="*"/><allow own="*"/></policy>
        </busconfig>"#,
        )
        .unwrap();
        let mut process = Command::new("dbus-daemon")
            .arg(format!("--config-file={}", config.display()))
            .args(["--nofork", "--nopidfile", "--print-address=1"])
            .stdout(Stdio::piped())
            .spawn()
            .expect("dbus-daemon is required for Linux idle inhibition tests");
        let mut address = String::new();
        BufReader::new(process.stdout.take().unwrap()).read_line(&mut address).unwrap();
        let bus = Self { process, config, address: address.trim().to_owned() };
        assert!(!bus.address.is_empty(), "private D-Bus daemon failed to start");
        bus
    }
}

impl Drop for PrivateBus {
    fn drop(&mut self) {
        let _ = self.process.kill();
        let _ = self.process.wait();
        let _ = std::fs::remove_file(&self.config);
    }
}

#[derive(Debug, PartialEq, Eq)]
enum Event {
    Portal(String),
    Close,
    ScreenSaver(String),
    UnInhibit(u32),
}

struct Portal {
    events: mpsc::UnboundedSender<Event>,
    status: u32,
    gate: Option<Arc<Notify>>,
}

struct Request(mpsc::UnboundedSender<Event>);

#[zbus::interface(name = "org.freedesktop.portal.Request")]
impl Request {
    fn close(&self) {
        let _ = self.0.send(Event::Close);
    }
}

#[zbus::interface(name = "org.freedesktop.portal.Inhibit")]
impl Portal {
    async fn inhibit(
        &self,
        window: &str,
        flags: u32,
        options: HashMap<String, OwnedValue>,
        #[zbus(connection)] bus: &Connection,
        #[zbus(header)] header: zbus::message::Header<'_>,
    ) -> zbus::fdo::Result<OwnedObjectPath> {
        assert_eq!(window, "");
        assert_eq!(flags, 8, "only idle, never logout/user-switch/suspend");
        assert_eq!(<&str>::try_from(&options["reason"]).unwrap(), REASON);
        let token = <&str>::try_from(&options["handle_token"]).unwrap();
        let sender = header.sender().unwrap();
        let path = OwnedObjectPath::try_from(format!(
            "/org/freedesktop/portal/desktop/request/{}/{token}",
            sender.as_str().trim_start_matches(':').replace('.', "_")
        ))
        .unwrap();
        bus.object_server().at(path.clone(), Request(self.events.clone())).await.unwrap();
        let _ = self.events.send(Event::Portal(sender.to_string()));
        if let Some(gate) = &self.gate {
            gate.notified().await;
        }
        // Deliberately emit before returning the handle to reproduce the response race.
        let _ = bus
            .emit_signal(
                Some(sender.as_str()),
                path.clone(),
                REQUEST,
                "Response",
                &(self.status, HashMap::<String, Value<'_>>::new()),
            )
            .await;
        Ok(path)
    }
}

struct ScreenSaver(mpsc::UnboundedSender<Event>);

#[zbus::interface(name = "org.freedesktop.ScreenSaver")]
impl ScreenSaver {
    fn inhibit(
        &self,
        application: &str,
        reason: &str,
        #[zbus(header)] header: zbus::message::Header<'_>,
    ) -> u32 {
        assert_eq!(application, "BMZ Player");
        assert_eq!(reason, REASON);
        let _ = self.0.send(Event::ScreenSaver(header.sender().unwrap().to_string()));
        42
    }

    #[zbus(name = "UnInhibit")]
    fn uninhibit(&self, cookie: u32) {
        let _ = self.0.send(Event::UnInhibit(cookie));
    }
}

struct Fixture {
    service: Connection,
    bus: PrivateBus,
    events: mpsc::UnboundedReceiver<Event>,
}

impl Fixture {
    async fn new(portal_status: Option<u32>, gate: Option<Arc<Notify>>, screen_path: &str) -> Self {
        let bus = PrivateBus::new();
        let (tx, events) = mpsc::unbounded_channel();
        let mut builder = zbus::connection::Builder::address(bus.address.as_str())
            .unwrap()
            .name(SCREENSAVER)
            .unwrap()
            .serve_at(screen_path, ScreenSaver(tx.clone()))
            .unwrap();
        if let Some(status) = portal_status {
            builder = builder
                .name(PORTAL)
                .unwrap()
                .serve_at("/org/freedesktop/portal/desktop", Portal { events: tx, status, gate })
                .unwrap();
        }
        let service = builder.build().await.unwrap();
        Self { service, bus, events }
    }

    fn run(&self, focused: bool) -> (watch::Sender<bool>, tokio::task::JoinHandle<()>) {
        let (tx, rx) = watch::channel(focused);
        let address = self.bus.address.clone();
        (tx, tokio::spawn(async move { run(rx, Some(&address)).await }))
    }

    async fn event(&mut self) -> Event {
        tokio::time::timeout(Duration::from_secs(5), self.events.recv()).await.unwrap().unwrap()
    }

    async fn owner_gone(&self, name: &str) {
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                let reply = self
                    .service
                    .call_method(
                        Some("org.freedesktop.DBus"),
                        "/org/freedesktop/DBus",
                        Some("org.freedesktop.DBus"),
                        "NameHasOwner",
                        &(name,),
                    )
                    .await
                    .unwrap();
                if !reply.body().deserialize::<bool>().unwrap() {
                    break;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
    }
}

#[tokio::test]
async fn portal_focus_loss_regain_and_shutdown_release_each_owner() {
    let mut fixture = Fixture::new(Some(0), None, "/org/freedesktop/ScreenSaver").await;
    let (tx, worker) = fixture.run(false);
    assert!(tokio::time::timeout(Duration::from_millis(30), fixture.events.recv()).await.is_err());
    tx.send(true).unwrap();
    let Event::Portal(first_owner) = fixture.event().await else {
        panic!("portal must be preferred")
    };
    tx.send(false).unwrap();
    fixture.owner_gone(&first_owner).await;
    // Cancellation may precede the reply, in which case disconnecting the
    // owner releases the unknown token without an explicit Close.
    while let Ok(event) = fixture.events.try_recv() {
        assert_eq!(event, Event::Close);
    }
    tx.send(true).unwrap();
    let Event::Portal(second_owner) = fixture.event().await else {
        panic!("expected reacquisition")
    };
    assert_ne!(first_owner, second_owner);
    drop(tx);
    tokio::time::timeout(Duration::from_secs(5), worker).await.unwrap().unwrap();
    fixture.owner_gone(&second_owner).await;
    while let Ok(event) = fixture.events.try_recv() {
        assert_eq!(event, Event::Close);
    }
}

#[tokio::test]
async fn unavailable_portal_falls_back_and_releases_cookie() {
    let mut fixture = Fixture::new(None, None, "/org/freedesktop/ScreenSaver").await;
    let (tx, worker) = fixture.run(true);
    let Event::ScreenSaver(owner) = fixture.event().await else {
        panic!("expected ScreenSaver fallback")
    };
    drop(tx);
    worker.await.unwrap();
    fixture.owner_gone(&owner).await;
    while let Ok(event) = fixture.events.try_recv() {
        assert_eq!(event, Event::UnInhibit(42));
    }
}

#[tokio::test]
async fn portal_denial_closes_request_and_falls_back_to_legacy_path() {
    let mut fixture = Fixture::new(Some(2), None, "/ScreenSaver").await;
    let (tx, worker) = fixture.run(true);
    assert!(matches!(fixture.event().await, Event::Portal(_)));
    assert_eq!(fixture.event().await, Event::Close);
    let Event::ScreenSaver(owner) = fixture.event().await else {
        panic!("expected legacy ScreenSaver fallback")
    };
    drop(tx);
    worker.await.unwrap();
    fixture.owner_gone(&owner).await;
}

#[tokio::test]
async fn focus_loss_during_request_disconnects_owner_before_late_response() {
    let gate = Arc::new(Notify::new());
    let mut fixture = Fixture::new(Some(0), Some(gate.clone()), "/ScreenSaver").await;
    let (tx, worker) = fixture.run(true);
    let Event::Portal(owner) = fixture.event().await else { panic!("expected portal") };
    tx.send(false).unwrap();
    fixture.owner_gone(&owner).await;
    gate.notify_one();
    assert!(tokio::time::timeout(Duration::from_millis(50), fixture.events.recv()).await.is_err());
    drop(tx);
    worker.await.unwrap();
}

#[tokio::test]
async fn shutdown_during_request_disconnects_owner_without_waiting_for_reply() {
    let gate = Arc::new(Notify::new());
    let mut fixture = Fixture::new(Some(0), Some(gate.clone()), "/ScreenSaver").await;
    let (tx, worker) = fixture.run(true);
    let Event::Portal(owner) = fixture.event().await else { panic!("expected portal") };
    drop(tx);
    tokio::time::timeout(Duration::from_millis(500), worker).await.unwrap().unwrap();
    fixture.owner_gone(&owner).await;
    gate.notify_one();
}

#[tokio::test]
async fn portal_timeout_disconnects_attempt_before_fallback() {
    let gate = Arc::new(Notify::new());
    let mut fixture = Fixture::new(Some(0), Some(gate.clone()), "/ScreenSaver").await;
    let (tx, worker) = fixture.run(true);
    let Event::Portal(owner) = fixture.event().await else { panic!("expected portal") };
    assert!(matches!(fixture.event().await, Event::ScreenSaver(_)));
    fixture.owner_gone(&owner).await;
    gate.notify_one();
    drop(tx);
    worker.await.unwrap();
}

#[tokio::test]
async fn granted_portal_handles_early_response_and_explicit_close() {
    let mut fixture = Fixture::new(Some(0), None, "/ScreenSaver").await;
    let bus = zbus::connection::Builder::address(fixture.bus.address.as_str())
        .unwrap()
        .build()
        .await
        .unwrap();
    let mut session = Session { bus, backend: Backend::Portal, token: None, owner: None };
    let (owners, _) =
        tokio::time::timeout(CALL_TIMEOUT, acquire(&mut session)).await.unwrap().unwrap();
    assert!(matches!(fixture.event().await, Event::Portal(_)));
    drop(owners);
    session.release().await;
    assert_eq!(fixture.event().await, Event::Close);
}

#[tokio::test]
async fn granted_cookie_is_released_to_original_service_owner() {
    let mut fixture = Fixture::new(None, None, "/ScreenSaver").await;
    let bus = zbus::connection::Builder::address(fixture.bus.address.as_str())
        .unwrap()
        .build()
        .await
        .unwrap();
    let mut session =
        Session { bus, backend: Backend::ScreenSaver("/ScreenSaver"), token: None, owner: None };
    let (owners, _) =
        tokio::time::timeout(CALL_TIMEOUT, acquire(&mut session)).await.unwrap().unwrap();
    assert!(matches!(fixture.event().await, Event::ScreenSaver(_)));
    // The well-known name no longer reaches this service, but UnInhibit must
    // still target its unique owner rather than a possible replacement service.
    fixture.service.release_name(SCREENSAVER).await.unwrap();
    drop(owners);
    session.release().await;
    assert_eq!(fixture.event().await, Event::UnInhibit(42));
}

#[tokio::test]
async fn service_owner_loss_ends_the_active_inhibition() {
    let mut fixture = Fixture::new(Some(0), None, "/ScreenSaver").await;
    let address = fixture.bus.address.clone();
    let mut session = None;
    let (result, ()) = tokio::time::timeout(Duration::from_secs(5), async {
        tokio::join!(acquire_and_hold(&mut session, Some(&address)), async {
            assert!(matches!(fixture.event().await, Event::Portal(_)));
            fixture.service.release_name(PORTAL).await.unwrap();
        })
    })
    .await
    .unwrap();
    assert!(result.unwrap_err().to_string().contains("service owner changed"));
    session.unwrap().release().await;
    assert_eq!(fixture.event().await, Event::Close);
}

#[test]
fn repeated_focus_events_do_not_restart_inhibition() {
    let (tx, mut rx) = watch::channel(true);
    let inhibitor = IdleInhibitor { focused: Some(tx), worker: None };
    inhibitor.set_focused(true);
    assert!(!rx.has_changed().unwrap());
    inhibitor.set_focused(false);
    assert!(rx.has_changed().unwrap());
    assert!(!*rx.borrow_and_update());
    inhibitor.set_focused(false);
    assert!(!rx.has_changed().unwrap());
    inhibitor.set_focused(true);
    assert!(*rx.borrow_and_update());
    drop(inhibitor);
    assert!(rx.has_changed().is_err());
}

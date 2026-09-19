//! Copy PNG/text using the window's Wayland connection and input serial.
//! Core wl_data_device works in GNOME and Flatpak without data-control globals.

use std::collections::HashMap;
use std::fmt;
use std::fs::File;
use std::io::{self, Write};
use std::os::fd::AsRawFd;
use std::sync::{Arc, mpsc};
use std::thread::{self, JoinHandle};

use anyhow::{Context, Result, anyhow};
use smithay_client_toolkit::reexports::calloop::{
    EventLoop, Interest, LoopHandle, Mode, PostAction, channel, generic::Generic,
};
use smithay_client_toolkit::reexports::calloop_wayland_source::WaylandSource;
use smithay_client_toolkit::reexports::client::{
    Connection, Dispatch, Proxy, QueueHandle, WEnum,
    backend::Backend,
    event_created_child,
    globals::{GlobalListContents, registry_queue_init},
    protocol::{
        wl_data_device, wl_data_device_manager, wl_data_offer, wl_data_source, wl_keyboard,
        wl_pointer, wl_registry, wl_seat,
    },
};
use winit::raw_window_handle::{HasDisplayHandle, RawDisplayHandle};
use winit::window::Window;

pub(crate) struct WaylandClipboard {
    sender: channel::Sender<Command>,
    worker: Option<JoinHandle<()>>,
    // The borrowed wl_display must outlive the worker and all its proxies.
    _window: Arc<Window>,
}

impl fmt::Debug for WaylandClipboard {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("WaylandClipboard").finish_non_exhaustive()
    }
}

impl WaylandClipboard {
    pub(crate) fn new(window: Arc<Window>) -> Result<Option<Arc<Self>>> {
        let RawDisplayHandle::Wayland(display) = window.display_handle()?.as_raw() else {
            return Ok(None);
        };
        // SAFETY: _window retains the display, and Drop joins the worker first.
        let backend = unsafe { Backend::from_foreign_display(display.display.as_ptr().cast()) };
        let connection = Connection::from_backend(backend);
        let (sender, receiver) = channel::channel();
        let (ready_tx, ready_rx) = mpsc::channel();
        let worker =
            thread::Builder::new().name("bmz-wayland-clipboard".into()).spawn(move || {
                if let Err(error) = run(connection, receiver, &ready_tx) {
                    let _ = ready_tx.send(Err(format!("{error:#}")));
                    tracing::warn!(%error, "Wayland clipboard worker stopped");
                }
            })?;
        let clipboard = Arc::new(Self { sender, worker: Some(worker), _window: window });
        ready_rx
            .recv()
            .context("Wayland clipboard initialization stopped")?
            .map_err(|error| anyhow!(error))?;
        tracing::info!("initialized native Wayland image/text clipboard");
        Ok(Some(clipboard))
    }

    fn store(&self, selection: Selection) -> Result<()> {
        let (reply, result) = mpsc::channel();
        self.sender
            .send(Command::Store(selection, reply))
            .map_err(|_| anyhow!("Wayland clipboard worker is unavailable"))?;
        result
            .recv()
            .context("Wayland clipboard worker stopped during copy")?
            .map_err(|error| anyhow!(error))
    }

    pub(crate) fn copy_text(&self, text: String) -> Result<()> {
        self.store(Selection { png: false, bytes: text.into_bytes().into() })
    }
}

impl bmz_render::renderer::ScreenshotClipboard for WaylandClipboard {
    fn copy_image(&self, _width: u32, _height: u32, _rgba: &[u8], png: &[u8]) -> Result<()> {
        self.store(Selection { png: true, bytes: Arc::from(png) })
    }
}

impl Drop for WaylandClipboard {
    fn drop(&mut self) {
        let _ = self.sender.send(Command::Exit);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

struct Selection {
    png: bool,
    bytes: Arc<[u8]>,
}

impl Selection {
    fn mime_types(&self) -> &'static [&'static str] {
        if self.png {
            &["image/png"]
        } else {
            &["text/plain;charset=utf-8", "text/plain", "UTF8_STRING"]
        }
    }
}

enum Command {
    Store(Selection, mpsc::Sender<Result<(), String>>),
    Exit,
}

struct Seat {
    seat: wl_seat::WlSeat,
    device: wl_data_device::WlDataDevice,
    keyboard: Option<wl_keyboard::WlKeyboard>,
    pointer: Option<wl_pointer::WlPointer>,
    serial: Option<u32>,
    focused: bool,
}

impl Drop for Seat {
    fn drop(&mut self) {
        if let Some(keyboard) = &self.keyboard
            && keyboard.version() >= 3
        {
            keyboard.release();
        }
        if let Some(pointer) = &self.pointer
            && pointer.version() >= 3
        {
            pointer.release();
        }
        if self.device.version() >= 2 {
            self.device.release();
        }
        if self.seat.version() >= 5 {
            self.seat.release();
        }
    }
}

struct State {
    manager: wl_data_device_manager::WlDataDeviceManager,
    seats: HashMap<u32, Seat>,
    latest_seat: Option<u32>,
    sources: Vec<wl_data_source::WlDataSource>,
    offers: Vec<wl_data_offer::WlDataOffer>,
    loop_handle: LoopHandle<'static, Self>,
    exit: bool,
}

impl State {
    fn add_seat(
        &mut self,
        registry: &wl_registry::WlRegistry,
        name: u32,
        version: u32,
        qh: &QueueHandle<Self>,
    ) {
        if self.seats.contains_key(&name) {
            return;
        }
        let seat = registry.bind::<wl_seat::WlSeat, _, _>(name, version.min(8), qh, name);
        let device = self.manager.get_data_device(&seat, qh, ());
        self.seats.insert(
            name,
            Seat { seat, device, keyboard: None, pointer: None, serial: None, focused: false },
        );
    }

    fn store(&mut self, selection: Selection, qh: &QueueHandle<Self>) -> Result<()> {
        let seat = self
            .latest_seat
            .and_then(|id| self.seats.get(&id))
            .filter(|seat| seat.focused)
            .context("Wayland clipboard copy requires keyboard focus")?;
        let serial = seat.serial.context("Wayland clipboard has no input serial")?;
        let selection = Arc::new(selection);
        let source = self.manager.create_data_source(qh, selection.clone());
        for mime in selection.mime_types() {
            source.offer((*mime).into());
        }
        seat.device.set_selection(Some(&source), serial);
        self.sources.push(source);
        Ok(())
    }
}

impl Drop for State {
    fn drop(&mut self) {
        for source in self.sources.drain(..) {
            source.destroy();
        }
        for offer in self.offers.drain(..) {
            offer.destroy();
        }
    }
}

fn run(
    connection: Connection,
    receiver: channel::Channel<Command>,
    ready: &mpsc::Sender<Result<(), String>>,
) -> Result<()> {
    let (globals, mut queue) = registry_queue_init::<State>(&connection)?;
    let qh = queue.handle();
    let mut event_loop = EventLoop::try_new()?;
    let manager = globals.bind(&qh, 1..=3, ()).context("wl_data_device_manager is unavailable")?;
    let mut state = State {
        manager,
        seats: HashMap::new(),
        latest_seat: None,
        sources: Vec::new(),
        offers: Vec::new(),
        loop_handle: event_loop.handle(),
        exit: false,
    };
    for global in globals.contents().clone_list() {
        if global.interface == "wl_seat" {
            state.add_seat(globals.registry(), global.name, global.version, &qh);
        }
    }
    queue.roundtrip(&mut state)?;
    let clipboard_connection = connection.clone();
    event_loop
        .handle()
        .insert_source(receiver, move |event, _, state| match event {
            channel::Event::Msg(Command::Store(selection, reply)) => {
                let result = state.store(selection, &qh).and_then(|()| match clipboard_connection
                    .flush()
                {
                    Ok(()) => Ok(()),
                    Err(smithay_client_toolkit::reexports::client::backend::WaylandError::Io(
                        error,
                    )) if error.kind() == io::ErrorKind::WouldBlock => Ok(()),
                    Err(error) => Err(error.into()),
                });
                let _ = reply.send(result.map_err(|error| format!("{error:#}")));
            }
            channel::Event::Msg(Command::Exit) | channel::Event::Closed => state.exit = true,
        })
        .map_err(|error| anyhow!("failed to register clipboard commands: {error}"))?;
    WaylandSource::new(connection, queue)
        .insert(event_loop.handle())
        .map_err(|error| anyhow!("failed to register Wayland events: {error}"))?;
    let _ = ready.send(Ok(()));
    while !state.exit {
        event_loop.dispatch(None, &mut state)?;
    }
    Ok(())
}

impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for State {
    fn event(
        state: &mut Self,
        registry: &wl_registry::WlRegistry,
        event: wl_registry::Event,
        _: &GlobalListContents,
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        match event {
            wl_registry::Event::Global { name, interface, version } if interface == "wl_seat" => {
                state.add_seat(registry, name, version, qh)
            }
            wl_registry::Event::GlobalRemove { name } => {
                state.seats.remove(&name);
            }
            _ => (),
        }
    }
}

impl Dispatch<wl_seat::WlSeat, u32> for State {
    fn event(
        state: &mut Self,
        _: &wl_seat::WlSeat,
        event: wl_seat::Event,
        id: &u32,
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        let Some(seat) = state.seats.get_mut(id) else {
            return;
        };
        if let wl_seat::Event::Capabilities { capabilities: WEnum::Value(caps) } = event {
            if caps.contains(wl_seat::Capability::Keyboard) {
                if seat.keyboard.is_none() {
                    seat.keyboard = Some(seat.seat.get_keyboard(qh, *id));
                }
            } else {
                seat.focused = false;
                if let Some(keyboard) = seat.keyboard.take()
                    && keyboard.version() >= 3
                {
                    keyboard.release();
                }
            }
            if caps.contains(wl_seat::Capability::Pointer) {
                if seat.pointer.is_none() {
                    seat.pointer = Some(seat.seat.get_pointer(qh, *id));
                }
            } else if let Some(pointer) = seat.pointer.take()
                && pointer.version() >= 3
            {
                pointer.release();
            }
        }
    }
}

impl Dispatch<wl_keyboard::WlKeyboard, u32> for State {
    fn event(
        state: &mut Self,
        _: &wl_keyboard::WlKeyboard,
        event: wl_keyboard::Event,
        id: &u32,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let Some(seat) = state.seats.get_mut(id) else {
            return;
        };
        match event {
            wl_keyboard::Event::Enter { serial, .. } => {
                seat.focused = true;
                seat.serial = Some(serial);
                state.latest_seat = Some(*id);
            }
            wl_keyboard::Event::Key { serial, .. } => {
                seat.serial = Some(serial);
                state.latest_seat = Some(*id);
            }
            wl_keyboard::Event::Leave { .. } => seat.focused = false,
            _ => (),
        }
    }
}

impl Dispatch<wl_pointer::WlPointer, u32> for State {
    fn event(
        state: &mut Self,
        _: &wl_pointer::WlPointer,
        event: wl_pointer::Event,
        id: &u32,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let wl_pointer::Event::Button { serial, .. } = event
            && let Some(seat) = state.seats.get_mut(id)
        {
            seat.serial = Some(serial);
            state.latest_seat = Some(*id);
        }
    }
}

impl Dispatch<wl_data_device_manager::WlDataDeviceManager, ()> for State {
    fn event(
        _: &mut Self,
        _: &wl_data_device_manager::WlDataDeviceManager,
        _: wl_data_device_manager::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<wl_data_device::WlDataDevice, ()> for State {
    event_created_child!(State, wl_data_device::WlDataDevice, [0 => (wl_data_offer::WlDataOffer, ())]);
    fn event(
        state: &mut Self,
        _: &wl_data_device::WlDataDevice,
        event: wl_data_device::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            wl_data_device::Event::DataOffer { id } => state.offers.push(id),
            wl_data_device::Event::Selection { .. } | wl_data_device::Event::Leave => {
                // This service only supplies data; egui owns clipboard reading.
                for offer in state.offers.drain(..) {
                    offer.destroy();
                }
            }
            _ => (),
        }
    }
}

impl Dispatch<wl_data_offer::WlDataOffer, ()> for State {
    fn event(
        _: &mut Self,
        _: &wl_data_offer::WlDataOffer,
        _: wl_data_offer::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<wl_data_source::WlDataSource, Arc<Selection>> for State {
    fn event(
        state: &mut Self,
        source: &wl_data_source::WlDataSource,
        event: wl_data_source::Event,
        selection: &Arc<Selection>,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            wl_data_source::Event::Send { mime_type, fd } => {
                if !selection.mime_types().contains(&mime_type.as_str()) {
                    return;
                }
                let file = File::from(fd);
                // SAFETY: fcntl only changes flags on the live, owned descriptor.
                let flags = unsafe { libc::fcntl(file.as_raw_fd(), libc::F_GETFL) };
                if flags < 0
                    || unsafe {
                        libc::fcntl(file.as_raw_fd(), libc::F_SETFL, flags | libc::O_NONBLOCK)
                    } < 0
                {
                    tracing::warn!(error = %io::Error::last_os_error(), "failed to prepare clipboard transfer");
                    return;
                }
                let bytes = selection.bytes.clone();
                let mut offset = 0;
                if let Err(error) = state.loop_handle.insert_source(
                    Generic::new(file, Interest::WRITE, Mode::Level),
                    move |_, file, _| {
                        // SAFETY: the file remains owned by this event source.
                        match write_pending(unsafe { file.get_mut() }, &bytes, &mut offset) {
                            Ok(false) => Ok(PostAction::Continue),
                            Ok(true) => Ok(PostAction::Remove),
                            Err(error) => {
                                tracing::debug!(%error, "clipboard receiver stopped reading");
                                Ok(PostAction::Remove)
                            }
                        }
                    },
                ) {
                    tracing::warn!(%error, "failed to register clipboard transfer");
                }
            }
            wl_data_source::Event::Cancelled => {
                state.sources.retain(|candidate| candidate != source);
                source.destroy();
            }
            _ => (),
        }
    }
}

fn write_pending(writer: &mut impl Write, bytes: &[u8], offset: &mut usize) -> io::Result<bool> {
    while *offset < bytes.len() {
        match writer.write(&bytes[*offset..]) {
            Ok(0) => return Err(io::ErrorKind::WriteZero.into()),
            Ok(count) => *offset += count,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => return Ok(false),
            Err(error) => return Err(error),
        }
    }
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clipboard_pipe_resumes_after_backpressure_without_losing_bytes() {
        struct SlowPipe {
            bytes: Vec<u8>,
            blocked: bool,
        }
        impl Write for SlowPipe {
            fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
                self.blocked = !self.blocked;
                if !self.blocked {
                    return Err(io::ErrorKind::WouldBlock.into());
                }
                let count = bytes.len().min(3);
                self.bytes.extend_from_slice(&bytes[..count]);
                Ok(count)
            }
            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }
        let expected = b"PNG payload larger than the receiving pipe capacity";
        let mut pipe = SlowPipe { bytes: Vec::new(), blocked: false };
        let mut offset = 0;
        while !write_pending(&mut pipe, expected, &mut offset).unwrap() {}
        assert_eq!(pipe.bytes, expected);
    }

    #[test]
    fn clipboard_pipe_reports_disconnected_reader() {
        struct ClosedPipe;
        impl Write for ClosedPipe {
            fn write(&mut self, _: &[u8]) -> io::Result<usize> {
                Err(io::ErrorKind::BrokenPipe.into())
            }
            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }
        assert_eq!(
            write_pending(&mut ClosedPipe, b"image", &mut 0).unwrap_err().kind(),
            io::ErrorKind::BrokenPipe
        );
    }
}

//! A transparent, click-through Wayland surface.
//!
//! The empty input region is set before the surface's first commit, so at no
//! point does the compositor consider this surface a target for pointer input.

use rustix::event::{PollFd, PollFlags, Timespec, poll};
use smithay_client_toolkit::{
    compositor::{CompositorHandler, CompositorState, FrameCallbackData, Region},
    delegate_dispatch2, delegate_registry,
    dispatch2::Dispatch2,
    output::{OutputHandler, OutputState},
    registry::{ProvidesRegistryState, RegistryState},
    registry_handlers,
    shell::{
        WaylandSurface,
        wlr_layer::{
            Anchor, KeyboardInteractivity, Layer, LayerShell, LayerShellHandler, LayerSurface,
            LayerSurfaceConfigure,
        },
        xdg::{
            XdgShell,
            window::{Window, WindowConfigure, WindowDecorations, WindowHandler},
        },
    },
};
use std::{
    ffi::c_void,
    io,
    os::fd::{AsFd, BorrowedFd},
    ptr::NonNull,
    time::Duration,
};
use wayland_client::{
    Connection, EventQueue, Proxy, QueueHandle,
    globals::registry_queue_init,
    protocol::{wl_output, wl_surface},
};
use wayland_protocols::wp::viewporter::client::{wp_viewport, wp_viewporter};

/// Size used until the desktop integration places the window.
const INITIAL_SIZE: (u32, u32) = (640, 360);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    /// A layer-shell overlay, for compositors that offer the protocol.
    LayerShell,
    /// A plain toplevel placed by the GNOME Shell extension that launched us.
    GnomeManaged,
}

enum Role {
    Layer(LayerSurface),
    Toplevel(Window),
}

impl Role {
    fn surface(&self) -> &wl_surface::WlSurface {
        match self {
            Role::Layer(l) => l.wl_surface(),
            Role::Toplevel(w) => w.wl_surface(),
        }
    }
}

struct State {
    registry: RegistryState,
    outputs: OutputState,
    size: Option<(u32, u32)>,
    resized: bool,
    frame_ready: bool,
    closed: bool,
}

pub struct Overlay {
    connection: Connection,
    queue: EventQueue<State>,
    handle: QueueHandle<State>,
    state: State,
    viewport: wp_viewport::WpViewport,
    _viewporter: wp_viewporter::WpViewporter,
    role: Role,
}

fn other(e: impl std::error::Error + Send + Sync + 'static) -> io::Error {
    io::Error::other(e)
}

impl Overlay {
    pub fn connect(mode: Mode) -> io::Result<Self> {
        let connection = Connection::connect_to_env().map_err(other)?;
        let (globals, queue) = registry_queue_init::<State>(&connection).map_err(other)?;
        let handle = queue.handle();
        let compositor = CompositorState::bind(&globals, &handle).map_err(other)?;
        let viewporter: wp_viewporter::WpViewporter =
            globals.bind(&handle, 1..=1, NoEvents).map_err(other)?;
        let surface = compositor.create_surface(&handle);

        // Applied with the first commit below, before the surface ever maps.
        let empty = Region::new(&compositor).map_err(other)?;
        surface.set_input_region(Some(empty.wl_region()));
        let viewport = viewporter.get_viewport(&surface, &handle, NoEvents);

        let role = match mode {
            Mode::LayerShell => {
                let shell = LayerShell::bind(&globals, &handle).map_err(other)?;
                let layer = shell.create_layer_surface(
                    &handle,
                    surface,
                    Layer::Overlay,
                    Some("larkaura"),
                    None,
                );
                layer.set_anchor(Anchor::TOP | Anchor::BOTTOM | Anchor::LEFT | Anchor::RIGHT);
                layer.set_exclusive_zone(-1);
                layer.set_keyboard_interactivity(KeyboardInteractivity::None);
                layer.commit();
                Role::Layer(layer)
            }
            Mode::GnomeManaged => {
                let shell = XdgShell::bind(&globals, &handle).map_err(other)?;
                let window = shell.create_window(surface, WindowDecorations::None, &handle);
                window.set_title("LarkAura Overlay");
                window.set_app_id("dev.larklabs.LarkAura.Overlay");
                window.commit();
                Role::Toplevel(window)
            }
        };
        let state = State {
            registry: RegistryState::new(&globals),
            outputs: OutputState::new(&globals, &handle),
            size: None,
            resized: false,
            frame_ready: false,
            closed: false,
        };
        let mut overlay = Self {
            connection,
            queue,
            handle,
            state,
            viewport,
            _viewporter: viewporter,
            role,
        };
        while overlay.state.size.is_none() && !overlay.state.closed {
            overlay
                .queue
                .blocking_dispatch(&mut overlay.state)
                .map_err(other)?;
        }
        Ok(overlay)
    }

    pub fn raw_handles(&self) -> (NonNull<c_void>, NonNull<c_void>) {
        let display = self.connection.backend().display_ptr() as *mut c_void;
        let surface = self.role.surface().id().as_ptr() as *mut c_void;
        (
            NonNull::new(display).expect("connected display"),
            NonNull::new(surface).expect("live surface"),
        )
    }

    pub fn size(&self) -> (u32, u32) {
        self.state.size.unwrap_or(INITIAL_SIZE)
    }

    pub fn is_closed(&self) -> bool {
        self.state.closed
    }

    pub fn take_resized(&mut self) -> bool {
        std::mem::take(&mut self.state.resized)
    }

    pub fn take_frame_ready(&mut self) -> bool {
        std::mem::take(&mut self.state.frame_ready)
    }

    /// Shows the next buffer at the full logical size, whatever its resolution.
    pub fn set_logical_size(&self, width: u32, height: u32) {
        self.viewport
            .set_destination(width.max(1) as i32, height.max(1) as i32);
    }

    /// Requests a callback for the next commit. Call right before presenting.
    pub fn request_frame(&self) {
        let surface = self.role.surface();
        surface.frame(&self.handle, FrameCallbackData(surface.clone()));
    }

    /// Handles Wayland events, sleeping until one arrives, `wake` is readable
    /// or the timeout passes. With no timeout the process uses no CPU at all.
    pub fn wait(
        &mut self,
        timeout: Option<Duration>,
        wake: Option<BorrowedFd<'_>>,
    ) -> io::Result<()> {
        self.queue
            .dispatch_pending(&mut self.state)
            .map_err(other)?;
        self.queue.flush().map_err(other)?;
        let Some(guard) = self.queue.prepare_read() else {
            self.queue
                .dispatch_pending(&mut self.state)
                .map_err(other)?;
            return Ok(());
        };
        let readable = {
            let display = guard.connection_fd();
            let mut fds = vec![PollFd::from_borrowed_fd(display.as_fd(), PollFlags::IN)];
            if let Some(fd) = wake {
                fds.push(PollFd::from_borrowed_fd(fd, PollFlags::IN));
            }
            let spec = timeout.map(|t| Timespec {
                tv_sec: t.as_secs() as _,
                tv_nsec: t.subsec_nanos() as _,
            });
            match poll(&mut fds, spec.as_ref()) {
                Ok(_) => !fds[0].revents().is_empty(),
                Err(rustix::io::Errno::INTR) => false,
                Err(e) => return Err(e.into()),
            }
        };
        if readable {
            match guard.read() {
                Ok(_) => {}
                Err(wayland_client::backend::WaylandError::Io(e))
                    if e.kind() == io::ErrorKind::WouldBlock => {}
                Err(e) => return Err(other(e)),
            }
        }
        self.queue
            .dispatch_pending(&mut self.state)
            .map_err(other)?;
        Ok(())
    }
}

impl State {
    fn configured(&mut self, width: Option<u32>, height: Option<u32>) {
        let current = self.size.unwrap_or(INITIAL_SIZE);
        let size = (
            width.unwrap_or(current.0).max(1),
            height.unwrap_or(current.1).max(1),
        );
        if self.size != Some(size) {
            self.size = Some(size);
            self.resized = true;
        }
    }
}

struct NoEvents;

impl Dispatch2<wp_viewporter::WpViewporter, State> for NoEvents {
    fn event(
        &self,
        _: &mut State,
        _: &wp_viewporter::WpViewporter,
        _: wp_viewporter::Event,
        _: &Connection,
        _: &QueueHandle<State>,
    ) {
    }
}

impl Dispatch2<wp_viewport::WpViewport, State> for NoEvents {
    fn event(
        &self,
        _: &mut State,
        _: &wp_viewport::WpViewport,
        _: wp_viewport::Event,
        _: &Connection,
        _: &QueueHandle<State>,
    ) {
    }
}

impl CompositorHandler for State {
    fn scale_factor_changed(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: i32,
    ) {
    }
    fn transform_changed(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: wl_output::Transform,
    ) {
    }
    fn frame(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: u32) {
        self.frame_ready = true;
    }
    fn surface_enter(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: &wl_output::WlOutput,
    ) {
    }
    fn surface_leave(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: &wl_output::WlOutput,
    ) {
    }
}

impl OutputHandler for State {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.outputs
    }
    fn new_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
    fn update_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
    fn output_destroyed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
}

impl WindowHandler for State {
    fn request_close(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &Window) {
        self.closed = true;
    }
    fn configure(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &Window,
        c: WindowConfigure,
        _: u32,
    ) {
        self.configured(c.new_size.0.map(|v| v.get()), c.new_size.1.map(|v| v.get()));
    }
}

impl LayerShellHandler for State {
    fn closed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &LayerSurface) {
        self.closed = true;
    }
    fn configure(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &LayerSurface,
        c: LayerSurfaceConfigure,
        _: u32,
    ) {
        let positive = |v: u32| (v > 0).then_some(v);
        self.configured(positive(c.new_size.0), positive(c.new_size.1));
    }
}

impl ProvidesRegistryState for State {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry
    }
    registry_handlers![OutputState];
}

delegate_registry!(State);
delegate_dispatch2!(State);

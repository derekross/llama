//! The Wayland window: one xdg toplevel holding the stacked Winamp windows,
//! drawn on the CPU into shared memory.
//!
//! Skin pixels map to a whole number of *physical* pixels (`View::zoom`),
//! and wp_viewporter scales the buffer to the logical size the compositor
//! wants at fractional scales like 1.5. That keeps every skin pixel sharp.

use std::time::{Duration, SystemTime};

use anyhow::{Context, Result};
use smithay_client_toolkit::reexports::calloop::timer::{TimeoutAction, Timer};
use smithay_client_toolkit::reexports::calloop::{EventLoop, LoopHandle, ping};
use smithay_client_toolkit::reexports::calloop_wayland_source::WaylandSource;
use smithay_client_toolkit::{
    compositor::{CompositorHandler, CompositorState},
    data_device_manager::{
        DataDeviceManagerState, WritePipe,
        data_device::{DataDevice, DataDeviceHandler},
        data_offer::{DataOfferHandler, DragOffer},
        data_source::DataSourceHandler,
    },
    delegate_compositor, delegate_data_device, delegate_keyboard, delegate_output,
    delegate_pointer, delegate_registry, delegate_seat, delegate_shm, delegate_xdg_shell,
    delegate_xdg_window,
    output::{OutputHandler, OutputState},
    registry::{ProvidesRegistryState, RegistryState},
    registry_handlers,
    seat::{
        Capability, SeatHandler, SeatState,
        keyboard::{KeyEvent, KeyboardHandler, Keysym, Modifiers, RawModifiers},
        pointer::{
            CursorIcon, PointerEvent, PointerEventKind, PointerHandler, ThemeSpec, ThemedPointer,
        },
    },
    shell::{
        WaylandSurface,
        xdg::{
            XdgShell, XdgSurface,
            window::{Window, WindowConfigure, WindowDecorations, WindowHandler},
        },
    },
    shm::{Shm, ShmHandler, slot::SlotPool},
};
use wayland_client::{
    Connection, Dispatch, QueueHandle,
    globals::{GlobalList, registry_queue_init},
    protocol::{
        wl_data_device::WlDataDevice, wl_data_device_manager::DndAction,
        wl_data_source::WlDataSource, wl_keyboard, wl_output, wl_pointer, wl_seat, wl_shm,
        wl_surface,
    },
};
use wayland_protocols::wp::fractional_scale::v1::client::{
    wp_fractional_scale_manager_v1, wp_fractional_scale_v1,
};
use wayland_protocols::wp::viewporter::client::{wp_viewport, wp_viewporter};

use crate::cliamp::engine::Engine;
use crate::skin::Skin;
use crate::store::{self, State};
use crate::ui::{Action, Key, Ui, View};

pub const APP_ID: &str = "llama";
const BTN_LEFT: u32 = 0x110;

pub struct Opts {
    pub base: Skin,
    /// Pinged to close the window (`llama --quit`, SIGTERM).
    pub quit: ping::PingSource,
    /// Pinged to bring a minimized window back (a second `llama`).
    pub show: ping::PingSource,
    /// Pinged to minimize (`llama --hide`).
    pub hide: ping::PingSource,
}

pub fn run(opts: Opts) -> Result<()> {
    let started = std::time::Instant::now();
    let conn = Connection::connect_to_env().context("no Wayland display")?;
    let (globals, queue) = registry_queue_init::<App>(&conn)?;
    let qh = queue.handle();
    let mut event_loop: EventLoop<App> = EventLoop::try_new()?;
    let handle = event_loop.handle();
    WaylandSource::new(conn.clone(), queue)
        .insert(handle.clone())
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    let compositor = CompositorState::bind(&globals, &qh).context("wl_compositor")?;
    let xdg = XdgShell::bind(&globals, &qh).context("xdg_shell")?;
    let shm = Shm::bind(&globals, &qh).context("wl_shm")?;
    let dnd = DataDeviceManagerState::bind(&globals, &qh).ok();

    let viewporter = bind_viewporter(&globals, &qh);
    let fractional_mgr = bind_fractional(&globals, &qh);
    let (window, viewport, fractional) = new_window(
        &compositor,
        &xdg,
        viewporter.as_ref(),
        fractional_mgr.as_ref(),
        &qh,
    );

    let state = State::load();
    let view = View {
        zoom: state.zoom.clamp(1, 8),
        shaded: state.shaded,
        eq_open: state.eq_open,
        pl_open: state.pl_open,
        pl_h: state.pl_h.max(crate::skin::sprites::PL_MIN_H),
        time_remaining: state.time_remaining,
        vis_on: state.vis_on,
        ..View::default()
    };
    let skin = load_skin(&state, &opts.base);
    let ui = Ui::new(skin, view);

    let (ping, ping_source) = ping::make_ping()?;
    handle
        .insert_source(ping_source, |_, _, app: &mut App| app.engine_changed())
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    let engine = Engine::start(move || ping.ping());
    handle
        .insert_source(opts.quit, |_, _, app: &mut App| app.exit = true)
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    handle
        .insert_source(opts.show, |_, _, app: &mut App| app.show())
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    handle
        .insert_source(opts.hide, |_, _, app: &mut App| app.hide())
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    // Animation and housekeeping: ~30 fps while something moves, otherwise
    // a slow tick that also notices skin changes made from the bar.
    handle
        .insert_source(Timer::immediate(), |_, _, app: &mut App| {
            app.tick();
            let live = app.engine.live.lock().unwrap();
            let fast = app.ui.animating(&live) || app.ui.view.press.is_some();
            drop(live);
            TimeoutAction::ToDuration(Duration::from_millis(if fast { 33 } else { 250 }))
        })
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    window.commit();

    let mut app = App {
        registry: RegistryState::new(&globals),
        seats: SeatState::new(&globals, &qh),
        outputs: OutputState::new(&globals, &qh),
        compositor,
        xdg,
        viewporter,
        fractional_mgr,
        shm,
        dnd,
        data_device: None,
        pool: SlotPool::new(275 * 116 * 4 * 4, &Shm::bind(&globals, &qh)?)?,
        window,
        viewport,
        _fractional: fractional,
        scale: 1.0,
        configured: false,
        hidden: false,
        place_pending: true,
        frame_pending: false,
        dirty: true,
        exit: false,
        pointer: None,
        keyboard: None,
        seat: None,
        press_serial: 0,
        ptr: (0.0, 0.0),
        ctrl: false,
        set_cursor: false,
        ui,
        engine,
        base: opts.base,
        state,
        state_seen: mtime(&store::state_path()),
        handle: handle.clone(),
        qh: qh.clone(),
        logical: (0, 0),
        started,
        shown: false,
    };

    while !app.exit {
        event_loop.dispatch(None, &mut app)?;
    }
    app.save_state();
    Ok(())
}

/// A fresh toplevel with its scaling objects. It stays unmapped until its
/// first commit.
fn new_window(
    compositor: &CompositorState,
    xdg: &XdgShell,
    viewporter: Option<&wp_viewporter::WpViewporter>,
    fractional_mgr: Option<&wp_fractional_scale_manager_v1::WpFractionalScaleManagerV1>,
    qh: &QueueHandle<App>,
) -> (
    Window,
    Option<wp_viewport::WpViewport>,
    Option<wp_fractional_scale_v1::WpFractionalScaleV1>,
) {
    let surface = compositor.create_surface(qh);
    let viewport = viewporter.map(|vp| vp.get_viewport(&surface, qh, ()));
    let fractional = fractional_mgr.map(|m| m.get_fractional_scale(&surface, qh, ()));
    let window = xdg.create_window(surface, WindowDecorations::None, qh);
    window.set_title("Llama");
    window.set_app_id(APP_ID);
    (window, viewport, fractional)
}

fn bind_viewporter(
    globals: &GlobalList,
    qh: &QueueHandle<App>,
) -> Option<wp_viewporter::WpViewporter> {
    globals.bind(qh, 1..=1, ()).ok()
}

fn bind_fractional(
    globals: &GlobalList,
    qh: &QueueHandle<App>,
) -> Option<wp_fractional_scale_manager_v1::WpFractionalScaleManagerV1> {
    globals.bind(qh, 1..=1, ()).ok()
}

fn mtime(p: &std::path::Path) -> Option<SystemTime> {
    std::fs::metadata(p).and_then(|m| m.modified()).ok()
}

/// The worn skin over the base skin; the base alone when it is unreadable.
pub fn load_skin(state: &State, base: &Skin) -> Skin {
    match &state.skin {
        Some(path) => match Skin::load(path) {
            Ok(s) => s.with_fallback(base),
            Err(e) => {
                log::warn!("{}: {e:#}", path.display());
                base.clone()
            }
        },
        None => base.clone(),
    }
}

struct App {
    registry: RegistryState,
    seats: SeatState,
    outputs: OutputState,
    compositor: CompositorState,
    shm: Shm,
    dnd: Option<DataDeviceManagerState>,
    data_device: Option<DataDevice>,
    pool: SlotPool,
    xdg: XdgShell,
    viewporter: Option<wp_viewporter::WpViewporter>,
    fractional_mgr: Option<wp_fractional_scale_manager_v1::WpFractionalScaleManagerV1>,
    window: Window,
    viewport: Option<wp_viewport::WpViewport>,
    _fractional: Option<wp_fractional_scale_v1::WpFractionalScaleV1>,
    scale: f64,
    configured: bool,
    /// Minimized: unmapped, while the music plays on.
    hidden: bool,
    /// Put the window back at the saved position once it maps.
    place_pending: bool,
    frame_pending: bool,
    dirty: bool,
    exit: bool,
    pointer: Option<ThemedPointer>,
    keyboard: Option<wl_keyboard::WlKeyboard>,
    seat: Option<wl_seat::WlSeat>,
    press_serial: u32,
    ptr: (f64, f64),
    ctrl: bool,
    set_cursor: bool,
    ui: Ui,
    engine: Engine,
    base: Skin,
    state: State,
    state_seen: Option<SystemTime>,
    handle: LoopHandle<'static, App>,
    qh: QueueHandle<App>,
    logical: (i32, i32),
    started: std::time::Instant,
    shown: bool,
}

impl App {
    /// Skin pixels → buffer pixels.
    fn zoom(&self) -> i32 {
        self.ui.view.zoom.max(1)
    }

    /// Surface-local (logical) position → skin pixel.
    fn skin_pos(&self, (x, y): (f64, f64)) -> (i32, i32) {
        let z = self.zoom() as f64;
        (
            (x * self.scale / z).floor() as i32,
            (y * self.scale / z).floor() as i32,
        )
    }

    /// Minimizes. Hyprland has no minimized state for a window to go to, so
    /// the window goes away instead, swapped for a fresh toplevel that stays
    /// unmapped until `show` commits it (Hyprland will not remap a toplevel
    /// once it has been unmapped). The bar icon or a second `llama` brings it
    /// back.
    fn hide(&mut self) {
        if self.hidden {
            return;
        }
        self.save_state();
        let (window, viewport, fractional) = new_window(
            &self.compositor,
            &self.xdg,
            self.viewporter.as_ref(),
            self.fractional_mgr.as_ref(),
            &self.qh,
        );
        if let Some(v) = std::mem::replace(&mut self.viewport, viewport) {
            v.destroy();
        }
        if let Some(f) = std::mem::replace(&mut self._fractional, fractional) {
            f.destroy();
        }
        drop(std::mem::replace(&mut self.window, window));
        self.hidden = true;
        self.configured = false;
        self.frame_pending = false;
        self.ui.view.press = None;
    }

    /// Maps the window again: the fresh toplevel's first commit starts the
    /// configure handshake, and the first configure draws.
    fn show(&mut self) {
        if !self.hidden {
            return;
        }
        self.hidden = false;
        self.place_pending = true;
        self.dirty = true;
        self.window.commit();
    }

    fn redraw(&mut self) {
        self.dirty = true;
        if self.configured && !self.frame_pending {
            self.draw();
        }
    }

    fn draw(&mut self) {
        let qh = self.qh.clone();
        {
            let mut live = self.engine.live.lock().unwrap();
            let replies = std::mem::take(&mut live.replies);
            for (tag, v) in replies {
                let a = self.ui.reply(&tag, &v);
                drop(live);
                self.apply(a);
                live = self.engine.live.lock().unwrap();
            }
            self.ui.draw(&live);
        }
        let z = self.zoom();
        let (sw, sh) = (self.ui.canvas.w, self.ui.canvas.h);
        // Logical size rounds up; the buffer covers it exactly, with any
        // spare physical pixels left transparent on the right and bottom.
        let lw = ((sw * z) as f64 / self.scale).ceil() as i32;
        let lh = ((sh * z) as f64 / self.scale).ceil() as i32;
        let (bw, bh) = if self.viewport.is_some() {
            (
                (lw as f64 * self.scale).round() as i32,
                (lh as f64 * self.scale).round() as i32,
            )
        } else {
            (sw * z, sh * z)
        };
        let stride = bw * 4;
        let Ok((buffer, canvas)) =
            self.pool
                .create_buffer(bw, bh, stride, wl_shm::Format::Argb8888)
        else {
            log::error!("could not allocate a {bw}x{bh} buffer");
            return;
        };
        canvas.fill(0);
        let content = (sw * z * 4) as usize;
        // Scale into a tight scratch image, then copy rows into the (maybe
        // wider) buffer.
        let mut tight = vec![0u8; content * (sh * z) as usize];
        self.ui.canvas.scale_into(&mut tight, z);
        self.ui.font.paint(&mut tight, (sw * z) as usize, z);
        for (row, chunk) in tight.chunks_exact(content).enumerate() {
            let o = row * stride as usize;
            canvas[o..o + content].copy_from_slice(chunk);
        }

        let surface = self.window.wl_surface();
        if let Some(vp) = &self.viewport {
            vp.set_destination(lw, lh);
            surface.set_buffer_scale(1);
        } else {
            surface.set_buffer_scale(1);
        }
        if (lw, lh) != self.logical {
            self.logical = (lw, lh);
            let size = Some((lw as u32, lh as u32));
            self.window.set_min_size(size);
            self.window.set_max_size(size);
            self.window.set_window_geometry(0, 0, lw as u32, lh as u32);
        }
        surface.damage_buffer(0, 0, bw, bh);
        surface.frame(&qh, surface.clone());
        if buffer.attach_to(surface).is_err() {
            return;
        }
        self.window.commit();
        self.frame_pending = true;
        self.dirty = false;
        if self.place_pending {
            self.place_pending = false;
            if let Some((x, y)) = self.state.pos {
                crate::hypr::restore(x, y);
            }
        }
        if !self.shown {
            self.shown = true;
            log::info!(
                "first frame {} ms after launch",
                self.started.elapsed().as_millis()
            );
        }
    }

    fn engine_changed(&mut self) {
        self.redraw();
    }

    fn tick(&mut self) {
        // A skin chosen from the bar (or `llama skin use`) lands in
        // state.json; pick it up live.
        let seen = mtime(&store::state_path());
        if seen != self.state_seen {
            self.state_seen = seen;
            let fresh = State::load();
            if fresh.skin != self.state.skin {
                let skin = load_skin(&fresh, &self.base);
                self.ui.set_skin(skin);
            }
            self.state.skin = fresh.skin;
            self.state.skin_name = fresh.skin_name;
            self.state.skin_md5 = fresh.skin_md5;
            self.dirty = true;
        }
        let live = self.engine.live.lock().unwrap();
        let animating = self.ui.animating(&live) || self.ui.view.press.is_some();
        let want_vis = self.ui.view.vis_on;
        drop(live);
        self.engine.want_spectrum.store(
            want_vis && !self.hidden,
            std::sync::atomic::Ordering::Relaxed,
        );
        if animating || self.dirty {
            self.redraw();
        }
    }

    fn apply(&mut self, actions: Vec<Action>) {
        let mut layout_changed = false;
        for a in actions {
            match a {
                Action::Op(name, params) => self.engine.op(&name, params),
                Action::Query(tag, name, params) => self.engine.query(&tag, &name, params),
                Action::Move => {
                    if let Some(seat) = &self.seat {
                        self.window.move_(seat, self.press_serial);
                    }
                }
                Action::Close => self.exit = true,
                Action::Minimize => self.hide(),
                Action::Resize | Action::Zoom => layout_changed = true,
            }
        }
        if layout_changed {
            self.save_state();
        }
        self.redraw();
    }

    fn save_state(&mut self) {
        let v = &self.ui.view;
        // Keep the user's base zoom: double size is a view of it.
        self.state.zoom = v.zoom;
        self.state.shaded = v.shaded;
        self.state.eq_open = v.eq_open;
        self.state.pl_open = v.pl_open;
        self.state.pl_h = v.pl_h;
        self.state.time_remaining = v.time_remaining;
        self.state.vis_on = v.vis_on;
        if !self.hidden
            && let Some(pos) = crate::hypr::position()
        {
            self.state.pos = Some(pos);
        }
        // Merge with the file so a skin picked meanwhile is not lost.
        let disk = State::load();
        self.state.skin = disk.skin.or(self.state.skin.take());
        if self.state.save().is_ok() {
            self.state_seen = mtime(&store::state_path());
        }
    }
}

impl CompositorHandler for App {
    fn scale_factor_changed(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        factor: i32,
    ) {
        if self._fractional.is_none() {
            self.scale = factor.max(1) as f64;
            self.redraw();
        }
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
        self.frame_pending = false;
        if self.dirty && self.configured {
            self.draw();
        }
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

impl OutputHandler for App {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.outputs
    }
    fn new_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
    fn update_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
    fn output_destroyed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
}

impl WindowHandler for App {
    fn request_close(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &Window) {
        self.exit = true;
    }

    fn configure(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &Window,
        cfg: WindowConfigure,
        _: u32,
    ) {
        let focused = cfg.is_activated();
        if focused != self.ui.view.focused {
            self.ui.view.focused = focused;
            self.dirty = true;
        }
        // We draw at our own size whatever the compositor suggests: Winamp's
        // windows do not stretch.
        self.configured = true;
        self.frame_pending = false;
        self.draw();
    }
}

impl SeatHandler for App {
    fn seat_state(&mut self) -> &mut SeatState {
        &mut self.seats
    }

    fn new_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}

    fn new_capability(
        &mut self,
        _: &Connection,
        qh: &QueueHandle<Self>,
        seat: wl_seat::WlSeat,
        cap: Capability,
    ) {
        self.seat = Some(seat.clone());
        if self.data_device.is_none() {
            self.data_device = self.dnd.as_ref().map(|m| m.get_data_device(qh, &seat));
        }
        if cap == Capability::Keyboard && self.keyboard.is_none() {
            let kb = self.seats.get_keyboard_with_repeat(
                qh,
                &seat,
                None,
                self.handle.clone(),
                Box::new(|app: &mut App, _, event| app.key(event)),
            );
            self.keyboard = kb.ok();
        }
        if cap == Capability::Pointer && self.pointer.is_none() {
            let surface = self.compositor.create_surface(qh);
            self.pointer = self
                .seats
                .get_pointer_with_theme(qh, &seat, self.shm.wl_shm(), surface, ThemeSpec::default())
                .ok();
        }
    }

    fn remove_capability(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: wl_seat::WlSeat,
        cap: Capability,
    ) {
        if cap == Capability::Keyboard
            && let Some(k) = self.keyboard.take()
        {
            k.release();
        }
        if cap == Capability::Pointer
            && let Some(p) = self.pointer.take()
        {
            p.pointer().release();
        }
    }

    fn remove_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}
}

impl App {
    fn key(&mut self, event: KeyEvent) {
        let key = match event.keysym {
            Keysym::Up => Key::Up,
            Keysym::Down => Key::Down,
            Keysym::Left => Key::Left,
            Keysym::Right => Key::Right,
            Keysym::Return | Keysym::KP_Enter => Key::Enter,
            Keysym::Escape => Key::Escape,
            Keysym::BackSpace => Key::Backspace,
            Keysym::Delete => Key::Delete,
            _ => Key::Other,
        };
        let live = self.engine.live.lock().unwrap().clone();
        let actions = self.ui.key(&live, key, event.utf8.as_deref(), self.ctrl);
        self.apply(actions);
    }
}

impl KeyboardHandler for App {
    fn enter(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: &wl_surface::WlSurface,
        _: u32,
        _: &[u32],
        _: &[Keysym],
    ) {
    }

    fn leave(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: &wl_surface::WlSurface,
        _: u32,
    ) {
    }

    fn press_key(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: u32,
        event: KeyEvent,
    ) {
        self.key(event);
    }

    fn repeat_key(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: u32,
        event: KeyEvent,
    ) {
        self.key(event);
    }

    fn release_key(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: u32,
        _: KeyEvent,
    ) {
    }

    fn update_modifiers(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: u32,
        modifiers: Modifiers,
        _: RawModifiers,
        _: u32,
    ) {
        self.ctrl = modifiers.ctrl;
    }
}

impl PointerHandler for App {
    fn pointer_frame(
        &mut self,
        conn: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_pointer::WlPointer,
        events: &[PointerEvent],
    ) {
        for event in events {
            if &event.surface != self.window.wl_surface() {
                continue;
            }
            self.ptr = event.position;
            let (x, y) = self.skin_pos(event.position);
            let live = self.engine.live.lock().unwrap().clone();
            let actions = match event.kind {
                PointerEventKind::Enter { .. } => {
                    self.set_cursor = true;
                    vec![]
                }
                PointerEventKind::Leave { .. } => vec![],
                PointerEventKind::Motion { .. } => {
                    let a = self.ui.pointer_move(&live, x, y);
                    if self.ui.view.pl_menu.is_some() {
                        self.dirty = true;
                    }
                    a
                }
                PointerEventKind::Press {
                    button: BTN_LEFT,
                    serial,
                    ..
                } => {
                    self.press_serial = serial;
                    self.ui.pointer_down(&live, x, y)
                }
                PointerEventKind::Release {
                    button: BTN_LEFT, ..
                } => self.ui.pointer_up(&live, x, y),
                PointerEventKind::Axis { vertical, .. } => {
                    let dy = if vertical.discrete != 0 {
                        vertical.discrete as f64
                    } else {
                        vertical.absolute
                    };
                    if dy.abs() > 0.0 {
                        self.ui.scroll(&live, x, y, dy)
                    } else {
                        vec![]
                    }
                }
                _ => vec![],
            };
            let pressed = matches!(
                event.kind,
                PointerEventKind::Press { .. } | PointerEventKind::Release { .. }
            );
            if pressed || !actions.is_empty() || self.ui.view.press.is_some() {
                self.dirty = true;
            }
            self.apply(actions);
        }
        if self.set_cursor {
            if let Some(p) = &self.pointer {
                let _ = p.set_cursor(conn, CursorIcon::Default);
            }
            self.set_cursor = false;
        }
    }
}

impl ShmHandler for App {
    fn shm_state(&mut self) -> &mut Shm {
        &mut self.shm
    }
}

impl ProvidesRegistryState for App {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry
    }
    registry_handlers![OutputState, SeatState];
}

impl Dispatch<wp_viewporter::WpViewporter, ()> for App {
    fn event(
        _: &mut Self,
        _: &wp_viewporter::WpViewporter,
        _: wp_viewporter::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<wp_viewport::WpViewport, ()> for App {
    fn event(
        _: &mut Self,
        _: &wp_viewport::WpViewport,
        _: wp_viewport::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<wp_fractional_scale_manager_v1::WpFractionalScaleManagerV1, ()> for App {
    fn event(
        _: &mut Self,
        _: &wp_fractional_scale_manager_v1::WpFractionalScaleManagerV1,
        _: wp_fractional_scale_manager_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<wp_fractional_scale_v1::WpFractionalScaleV1, ()> for App {
    fn event(
        app: &mut Self,
        _: &wp_fractional_scale_v1::WpFractionalScaleV1,
        event: wp_fractional_scale_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let wp_fractional_scale_v1::Event::PreferredScale { scale } = event {
            let s = scale as f64 / 120.0;
            if (s - app.scale).abs() > f64::EPSILON {
                app.scale = s;
                app.redraw();
            }
        }
    }
}

delegate_compositor!(App);
delegate_output!(App);
delegate_shm!(App);
delegate_seat!(App);
delegate_keyboard!(App);
delegate_pointer!(App);
delegate_xdg_shell!(App);
delegate_xdg_window!(App);
delegate_registry!(App);
delegate_data_device!(App);

// ---- dropping a .wsz on the window ---------------------------------------------

const URI_LIST: &str = "text/uri-list";

/// The local files in a text/uri-list, decoded.
pub fn uri_files(list: &str) -> Vec<std::path::PathBuf> {
    list.lines()
        .map(str::trim)
        .filter(|l| !l.starts_with('#'))
        .filter_map(|l| l.strip_prefix("file://"))
        .map(|p| {
            // Drop an authority ("file://host/path") and percent-decode.
            let p = if p.starts_with('/') {
                p
            } else {
                p.find('/').map_or(p, |i| &p[i..])
            };
            let bytes = p.as_bytes();
            let mut out = Vec::with_capacity(bytes.len());
            let mut i = 0;
            while i < bytes.len() {
                if bytes[i] == b'%'
                    && i + 2 < bytes.len()
                    && let Ok(b) = u8::from_str_radix(&p[i + 1..i + 3], 16)
                {
                    out.push(b);
                    i += 3;
                    continue;
                }
                out.push(bytes[i]);
                i += 1;
            }
            std::path::PathBuf::from(String::from_utf8_lossy(&out).into_owned())
        })
        .collect()
}

impl DataDeviceHandler for App {
    fn enter(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        dev: &WlDataDevice,
        _: f64,
        _: f64,
        _: &wl_surface::WlSurface,
    ) {
        let Some(device) = self.data_device.as_ref().filter(|d| d.inner() == dev) else {
            return;
        };
        let Some(offer) = device.data().drag_offer() else {
            return;
        };
        let wanted = offer.with_mime_types(|m| m.iter().any(|t| t == URI_LIST));
        offer.accept_mime_type(offer.serial, wanted.then(|| URI_LIST.to_string()));
        offer.set_actions(DndAction::Copy, DndAction::Copy);
    }

    fn leave(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlDataDevice) {}

    fn motion(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlDataDevice, _: f64, _: f64) {}

    fn selection(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlDataDevice) {}

    fn drop_performed(&mut self, conn: &Connection, _: &QueueHandle<Self>, dev: &WlDataDevice) {
        let Some(device) = self.data_device.as_ref().filter(|d| d.inner() == dev) else {
            return;
        };
        let Some(offer) = device.data().drag_offer() else {
            return;
        };
        let pipe = match offer.receive(URI_LIST.to_string()) {
            Ok(p) => p,
            Err(e) => {
                log::warn!("drop: {e}");
                return;
            }
        };
        let _ = conn.flush();
        offer.finish();
        offer.destroy();
        // Read and install off the event loop: the sender writes at its own
        // pace, and a Museum-free local install still touches the disk. The
        // window picks the new skin up from the state file.
        std::thread::spawn(move || {
            use std::io::Read;
            let mut text = String::new();
            let mut pipe = pipe;
            if pipe.read_to_string(&mut text).is_err() {
                return;
            }
            let skin = uri_files(&text).into_iter().find(|p| {
                let ext = p
                    .extension()
                    .map(|e| e.to_string_lossy().to_ascii_lowercase());
                matches!(ext.as_deref(), Some("wsz" | "zip"))
            });
            if let Some(p) = skin
                && let Err(e) = crate::skins::wear(&p.to_string_lossy())
            {
                log::warn!("dropped skin: {e:#}");
            }
        });
    }
}

impl DataOfferHandler for App {
    fn source_actions(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        offer: &mut DragOffer,
        _: DndAction,
    ) {
        offer.set_actions(DndAction::Copy, DndAction::Copy);
    }

    fn selected_action(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &mut DragOffer,
        _: DndAction,
    ) {
    }
}

impl DataSourceHandler for App {
    fn accept_mime(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &WlDataSource,
        _: Option<String>,
    ) {
    }
    fn send_request(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &WlDataSource,
        _: String,
        _: WritePipe,
    ) {
    }
    fn cancelled(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlDataSource) {}
    fn dnd_dropped(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlDataSource) {}
    fn dnd_finished(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlDataSource) {}
    fn action(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlDataSource, _: DndAction) {}
}

#[cfg(test)]
mod tests {
    #[test]
    fn uri_list_decodes_paths() {
        let files = super::uri_files(
            "# comment\r\nfile:///home/a/My%20Skin.wsz\r\nfile://host/x/b.zip\nhttps://e/x.wsz\n",
        );
        assert_eq!(
            files,
            vec![
                std::path::PathBuf::from("/home/a/My Skin.wsz"),
                std::path::PathBuf::from("/x/b.zip")
            ]
        );
    }
}

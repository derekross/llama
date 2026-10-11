//! The Wayland window: one xdg toplevel holding the stacked Winamp windows,
//! drawn on the CPU into shared memory.
//!
//! Skin pixels map to a whole number of *physical* pixels (`View::zoom`),
//! and wp_viewporter scales the buffer to the logical size the compositor
//! wants at fractional scales like 1.5. That keeps every skin pixel sharp.

use std::time::{Duration, SystemTime};

use anyhow::{Context, Result};
use smithay_client_toolkit::reexports::calloop::channel::{self, Channel};
use smithay_client_toolkit::reexports::calloop::timer::{TimeoutAction, Timer};
use smithay_client_toolkit::reexports::calloop::{EventLoop, LoopHandle, ping};
use smithay_client_toolkit::reexports::calloop_wayland_source::WaylandSource;
use smithay_client_toolkit::reexports::protocols::xdg::shell::client::xdg_positioner;
use smithay_client_toolkit::{
    compositor::{CompositorHandler, CompositorState},
    data_device_manager::{
        DataDeviceManagerState, WritePipe,
        data_device::{DataDevice, DataDeviceHandler},
        data_offer::{DataOfferHandler, DragOffer},
        data_source::DataSourceHandler,
    },
    delegate_compositor, delegate_data_device, delegate_keyboard, delegate_output,
    delegate_pointer, delegate_registry, delegate_seat, delegate_shm, delegate_xdg_popup,
    delegate_xdg_shell, delegate_xdg_window,
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
            XdgPositioner, XdgShell, XdgSurface,
            popup::{Popup, PopupConfigure, PopupHandler},
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

use crate::cliamp::engine::{Engine, Live};
use crate::skin::Skin;
use crate::store::{self, State};
use crate::ui::{Action, Key, Mods, Pick, Ui, View};

pub const APP_ID: &str = "llama";
const BTN_LEFT: u32 = 0x110;
const BTN_RIGHT: u32 = 0x111;

/// Results that background threads hand back to the event loop.
pub enum Msg {
    Actions(Vec<Action>),
    Note(String),
    /// A skin loaded off the event loop, for load number `.0`.
    Skin(u64, Box<Skin>),
    /// No file chooser exists: ask for a typed path instead.
    NoChooser,
}

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
        zoom: state.zoom.clamp(1, 8) * if state.double { 2 } else { 1 },
        double: state.double,
        pinned: state.pinned,
        eq_saved: state.eq_saved.clone(),
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

    let (msgs, msg_src): (channel::Sender<Msg>, Channel<Msg>) = channel::channel();
    handle
        .insert_source(msg_src, |event, _, app: &mut App| {
            if let channel::Event::Msg(m) = event {
                app.message(m);
            }
        })
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    // Animation and housekeeping: ~30 fps while something moves, otherwise
    // a slow tick that keeps the clock current and notices skin changes
    // made from the bar.
    handle
        .insert_source(Timer::immediate(), |_, _, app: &mut App| {
            app.tick();
            let live = app.engine.live.clone();
            let live = live.lock().unwrap();
            let fast = !app.hidden && (app.ui.animating(&live) || app.ui.view.press.is_some());
            drop(live);
            TimeoutAction::ToDuration(Duration::from_millis(if fast { 33 } else { 250 }))
        })
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    window.commit();

    let pool = SlotPool::new(275 * 116 * 4 * 4, &shm)?;
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
        pool,
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
        mods: Mods::default(),
        repeat_ok: false,
        scroll_acc: (0, 0.0),
        set_cursor: false,
        menu: None,
        msgs,
        skin_gen: 0,
        last_clock: None,
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
    app.save_state(true);
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

/// The main menu as an xdg popup, so it can hang outside the window.
struct MenuPopup {
    popup: Popup,
    viewport: Option<wp_viewport::WpViewport>,
    hover: Option<usize>,
    configured: bool,
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
    mods: Mods,
    /// Whether the key now held may repeat (decided when it went down).
    repeat_ok: bool,
    /// Wheel travel not yet turned into steps: (value120, pixels).
    scroll_acc: (i32, f64),
    set_cursor: bool,
    /// The open main menu.
    menu: Option<MenuPopup>,
    msgs: channel::Sender<Msg>,
    /// Bumped per skin load, so a slow, stale load is dropped.
    skin_gen: u64,
    /// The second the clock last showed, to redraw once it moves.
    last_clock: Option<u64>,
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

    /// The scale as wp_fractional_scale's 120ths.
    fn scale120(&self) -> i32 {
        (self.scale * 120.0).round().max(1.0) as i32
    }

    /// The logical size that holds `px` physical pixels at this scale, and
    /// that size back in physical pixels. The logical size is rounded up to
    /// a length whose physical size is a whole number, so the viewport maps
    /// buffer pixels 1:1 and nothing is resampled (at 1.5, an even length).
    fn fit(&self, px: i32) -> (i32, i32) {
        let n = self.scale120();
        let step = 120 / gcd(n, 120);
        let logical = (px * 120 + n - 1) / n;
        let logical = (logical + step - 1) / step * step;
        (logical, logical * n / 120)
    }

    /// Surface-local (logical) position → skin pixel.
    fn skin_pos(&self, (x, y): (f64, f64)) -> (i32, i32) {
        let z = self.zoom() as f64;
        let s = self.scale120() as f64 / 120.0;
        ((x * s / z).floor() as i32, (y * s / z).floor() as i32)
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
        log::debug!("minimizing");
        self.close_menu();
        self.save_state(true);
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
        // The new toplevel needs its size hints sent again.
        self.logical = (0, 0);
        self.ui.pointer_lost();
        self.ui.clear_vis();
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
        let live = self.engine.live.clone();
        let replies = {
            let mut live = live.lock().unwrap();
            let replies = std::mem::take(&mut live.replies);
            // The latest failure is the one worth reading.
            if let Some(e) = live.errors.pop() {
                live.errors.clear();
                self.ui.note(e);
            }
            self.ui.draw(&live);
            if live.snap.playing() || live.snap.paused() {
                self.last_clock = Some(live.position() as u64);
            }
            replies
        };
        let z = self.zoom();
        let (sw, sh) = (self.ui.canvas.w, self.ui.canvas.h);
        let (lw, lh, bw, bh) = if self.viewport.is_some() {
            let (lw, bw) = self.fit(sw * z);
            let (lh, bh) = self.fit(sh * z);
            (lw, lh, bw, bh)
        } else {
            (sw * z, sh * z, sw * z, sh * z)
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
        }
        surface.set_buffer_scale(1);
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
            if self.ui.view.pinned {
                crate::hypr::pin(true);
            }
        }
        if !self.shown {
            self.shown = true;
            log::info!(
                "first frame {} ms after launch",
                self.started.elapsed().as_millis()
            );
        }
        // Answers to queries (a playlist list to choose from) act after the
        // frame, so their redraw is not nested inside this one.
        for (tag, v) in replies {
            let a = self.ui.reply(&tag, &v);
            self.apply(a);
        }
    }

    fn engine_changed(&mut self) {
        self.redraw();
    }

    fn message(&mut self, m: Msg) {
        match m {
            Msg::Actions(a) => self.apply(a),
            Msg::Note(text) => {
                self.ui.note(text);
                self.redraw();
            }
            Msg::NoChooser => {
                self.ui.open_prompt(crate::ui::PromptKind::AddLocation);
                self.apply(vec![Action::Resize]);
            }
            Msg::Skin(generation, skin) => {
                if generation == self.skin_gen {
                    self.ui.set_skin(*skin);
                    self.redraw();
                }
            }
        }
    }

    /// Loads the worn skin on a worker: unzipping, decoding and building
    /// region masks can take a moment, and the window keeps running.
    fn load_skin_async(&mut self) {
        self.skin_gen += 1;
        let generation = self.skin_gen;
        let state = self.state.clone();
        let base = self.base.clone();
        let tx = self.msgs.clone();
        std::thread::spawn(move || {
            let skin = load_skin(&state, &base);
            let _ = tx.send(Msg::Skin(generation, Box::new(skin)));
        });
    }

    fn tick(&mut self) {
        // A skin chosen from the bar (or `llama skin use`) lands in
        // state.json; pick it up live.
        let seen = mtime(&store::state_path());
        if seen != self.state_seen {
            self.state_seen = seen;
            let fresh = State::load();
            let changed = fresh.skin != self.state.skin || fresh.skin_md5 != self.state.skin_md5;
            self.state.skin = fresh.skin;
            self.state.skin_name = fresh.skin_name;
            self.state.skin_md5 = fresh.skin_md5;
            if changed {
                self.load_skin_async();
            }
        }
        if let Some((_, at)) = &self.ui.view.note
            && at.elapsed() > crate::ui::NOTE_FOR
        {
            self.ui.view.note = None;
            self.dirty = true;
        }
        let live = self.engine.live.clone();
        let live = live.lock().unwrap();
        let animating = self.ui.animating(&live) || self.ui.view.press.is_some();
        // The clock only needs a frame when its second changes.
        let clock = (live.snap.playing() || live.snap.paused()).then(|| live.position() as u64);
        drop(live);
        if clock != self.last_clock {
            self.dirty = true;
        }
        self.engine.want_spectrum.store(
            self.ui.view.vis_on && !self.hidden,
            std::sync::atomic::Ordering::Relaxed,
        );
        if !self.hidden && (animating || self.dirty) {
            self.redraw();
        }
    }

    fn apply(&mut self, actions: Vec<Action>) {
        if actions.is_empty() {
            if self.dirty {
                self.redraw();
            }
            return;
        }
        let mut layout_changed = false;
        let mut save = false;
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
                Action::Save => save = true,
                Action::OpenMenu(x, y) => self.open_menu(x, y),
                Action::PickFiles(kind) => self.pick_files(kind),
                Action::Pin(on) => crate::hypr::pin(on),
                Action::SkinBrowser => open_skin_browser(),
                Action::BaseSkin => {
                    let mut st = State::load();
                    st.skin = None;
                    st.skin_name = None;
                    st.skin_md5 = None;
                    if let Err(e) = st.save() {
                        self.ui.note(format!("Could not change skin: {e}"));
                    }
                    // The tick notices state.json and loads the base.
                }
            }
        }
        if layout_changed || save {
            self.save_state(false);
        }
        self.redraw();
    }

    /// Writes the view to state.json. The window position is asked of
    /// Hyprland only when `with_pos` (on minimize and quit): it means a
    /// process spawn, too slow for every playlist resize step.
    fn save_state(&mut self, with_pos: bool) {
        let v = &self.ui.view;
        // The base zoom: double size is a view of it.
        self.state.zoom = if v.double {
            (v.zoom / 2).max(1)
        } else {
            v.zoom
        };
        self.state.shaded = v.shaded;
        self.state.eq_open = v.eq_open;
        self.state.pl_open = v.pl_open;
        self.state.pl_h = v.pl_h;
        self.state.time_remaining = v.time_remaining;
        self.state.vis_on = v.vis_on;
        self.state.double = v.double;
        self.state.pinned = v.pinned;
        self.state.eq_saved = v.eq_saved.clone();
        if with_pos
            && !self.hidden
            && let Some(pos) = crate::hypr::position()
        {
            self.state.pos = Some(pos);
        }
        // The skin belongs to whoever wrote state.json last (the bar, a
        // drop): keep theirs, and load it if this window has not yet.
        let disk = State::load();
        let skin_changed = disk.skin != self.state.skin || disk.skin_md5 != self.state.skin_md5;
        self.state.skin = disk.skin;
        self.state.skin_name = disk.skin_name;
        self.state.skin_md5 = disk.skin_md5;
        if skin_changed {
            self.load_skin_async();
        }
        if self.state.save().is_ok() {
            self.state_seen = mtime(&store::state_path());
        }
    }

    // ---- the main menu ---------------------------------------------------------

    fn open_menu(&mut self, x: i32, y: i32) {
        self.close_menu();
        let Some(seat) = self.seat.clone() else {
            return;
        };
        let (mw, mh) = self.ui.menu_size();
        let z = self.zoom();
        let (lw, _) = self.fit(mw * z);
        let (lh, _) = self.fit(mh * z);
        let s = self.scale120() as f64 / 120.0;
        let ax = (x * z) as f64 / s;
        let ay = (y * z) as f64 / s;
        let Ok(positioner) = XdgPositioner::new(&self.xdg) else {
            return;
        };
        positioner.set_size(lw, lh);
        positioner.set_anchor_rect(ax as i32, ay as i32, 1, 1);
        positioner.set_anchor(xdg_positioner::Anchor::TopLeft);
        positioner.set_gravity(xdg_positioner::Gravity::BottomRight);
        positioner.set_constraint_adjustment(
            xdg_positioner::ConstraintAdjustment::SlideX
                | xdg_positioner::ConstraintAdjustment::SlideY
                | xdg_positioner::ConstraintAdjustment::FlipX
                | xdg_positioner::ConstraintAdjustment::FlipY,
        );
        let Ok(popup) = Popup::new(
            self.window.xdg_surface(),
            &positioner,
            &self.qh,
            &self.compositor,
            &self.xdg,
        ) else {
            return;
        };
        let viewport = self
            .viewporter
            .as_ref()
            .map(|vp| vp.get_viewport(popup.wl_surface(), &self.qh, ()));
        popup.xdg_popup().grab(&seat, self.press_serial);
        popup.wl_surface().commit();
        self.menu = Some(MenuPopup {
            popup,
            viewport,
            hover: None,
            configured: false,
        });
    }

    fn close_menu(&mut self) {
        if let Some(m) = self.menu.take() {
            if let Some(v) = m.viewport {
                v.destroy();
            }
            drop(m.popup);
        }
    }

    fn draw_menu(&mut self) {
        let Some(hover) = self.menu.as_ref().filter(|m| m.configured).map(|m| m.hover) else {
            return;
        };
        let (mw, mh) = self.ui.menu_size();
        let z = self.zoom();
        let (lw, bw) = self.fit(mw * z);
        let (lh, bh) = self.fit(mh * z);
        let Ok((buffer, canvas)) =
            self.pool
                .create_buffer(bw, bh, bw * 4, wl_shm::Format::Argb8888)
        else {
            return;
        };
        canvas.fill(0);
        let content = (mw * z * 4) as usize;
        let mut tight = vec![0u8; content * (mh * z) as usize];
        self.ui.draw_menu(hover, &mut tight, z);
        for (row, chunk) in tight.chunks_exact(content).enumerate() {
            let o = row * (bw * 4) as usize;
            canvas[o..o + content].copy_from_slice(chunk);
        }
        let Some(m) = &self.menu else { return };
        let surface = m.popup.wl_surface();
        if let Some(vp) = &m.viewport {
            vp.set_destination(lw, lh);
        }
        surface.damage_buffer(0, 0, bw, bh);
        if buffer.attach_to(surface).is_ok() {
            surface.commit();
        }
    }

    /// Pointer events over the menu.
    fn menu_pointer(&mut self, event: &PointerEvent) {
        let (x, y) = self.skin_pos(event.position);
        let item = self.ui.menu_item_at(x, y);
        match event.kind {
            PointerEventKind::Motion { .. } | PointerEventKind::Enter { .. } => {
                if let Some(m) = self.menu.as_mut()
                    && m.hover != item
                {
                    m.hover = item;
                    self.draw_menu();
                }
            }
            PointerEventKind::Leave { .. } => {
                if let Some(m) = self.menu.as_mut() {
                    m.hover = None;
                }
                self.draw_menu();
            }
            PointerEventKind::Release { .. } => {
                let Some(i) = item else { return };
                let cmd = self.ui.menu_items()[i].cmd;
                self.close_menu();
                let live = self.engine.live.clone();
                let live = live.lock().unwrap();
                let actions = self.ui.menu_command(&live, cmd);
                drop(live);
                self.apply(actions);
            }
            _ => {}
        }
    }

    // ---- files --------------------------------------------------------------------

    /// Runs the file chooser on a thread and queues what comes back. Without
    /// a chooser, falls back to typing a path into the playlist.
    fn pick_files(&mut self, kind: Pick) {
        let tx = self.msgs.clone();
        std::thread::spawn(move || {
            let (title, folder) = match kind {
                Pick::Play => ("Play files", false),
                Pick::AddFiles => ("Add files to the playlist", false),
                Pick::AddFolder => ("Add a folder to the playlist", true),
            };
            let picked = match crate::picker::pick(title, folder) {
                Ok(Some(p)) => p,
                Ok(None) => return,
                Err(e) => {
                    log::warn!("{e:#}");
                    let _ = tx.send(Msg::NoChooser);
                    return;
                }
            };
            let files: Vec<std::path::PathBuf> = picked
                .into_iter()
                .flat_map(|p| {
                    if p.is_dir() {
                        crate::picker::audio_in(&p)
                    } else {
                        vec![p]
                    }
                })
                .collect();
            if files.is_empty() {
                let _ = tx.send(Msg::Note("No audio files there".into()));
                return;
            }
            let _ = tx.send(Msg::Actions(queue_files(kind, &files)));
        });
    }
}

/// The operations that put picked files in the playlist.
fn queue_files(kind: Pick, files: &[std::path::PathBuf]) -> Vec<Action> {
    let mut out = vec![];
    if kind == Pick::Play {
        out.push(crate::ui::op("queue.clear", serde_json::json!({})));
    }
    for f in files {
        out.push(crate::ui::op(
            "queue",
            serde_json::json!({"path": f.to_string_lossy()}),
        ));
    }
    if kind == Pick::Play {
        out.push(crate::ui::op("queue.play", serde_json::json!({"index": 0})));
    }
    out
}

/// Opens the bar panel on its Skins tab (omarchy-shell's IPC).
fn open_skin_browser() {
    let shell = std::env::var("OMARCHY_PATH")
        .map(|p| format!("{p}/shell"))
        .unwrap_or_else(|_| "/usr/share/omarchy/shell".into());
    let _ = std::process::Command::new("qs")
        .args([
            "ipc",
            "-n",
            "-p",
            &shell,
            "call",
            "derekross.llama",
            "skins",
        ])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map(|mut c| std::thread::spawn(move || c.wait()));
}

fn gcd(a: i32, b: i32) -> i32 {
    if b == 0 {
        a.abs().max(1)
    } else {
        gcd(b, a % b)
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
                Box::new(|app: &mut App, _, event| app.key(event, true)),
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
    fn key(&mut self, event: KeyEvent, repeat: bool) {
        let key = match event.keysym {
            Keysym::Up => Key::Up,
            Keysym::Down => Key::Down,
            Keysym::Left => Key::Left,
            Keysym::Right => Key::Right,
            Keysym::Return | Keysym::KP_Enter => Key::Enter,
            Keysym::Escape => Key::Escape,
            Keysym::BackSpace => Key::Backspace,
            Keysym::Delete => Key::Delete,
            Keysym::Page_Up => Key::PageUp,
            Keysym::Page_Down => Key::PageDown,
            Keysym::Home => Key::Home,
            Keysym::End => Key::End,
            _ => Key::Other,
        };
        if repeat && !self.repeat_ok {
            return;
        }
        if !repeat {
            self.repeat_ok = self.ui.repeats(key, self.ui.view.prompt.is_some());
        }
        if key == Key::Escape && self.menu.is_some() {
            self.close_menu();
            return;
        }
        // The key's own character, before Ctrl turns it into a control code.
        let ch = event.keysym.key_char();
        let live = self.engine.live.clone();
        let live = live.lock().unwrap();
        let actions = self
            .ui
            .key(&live, key, ch, event.utf8.as_deref(), self.mods);
        drop(live);
        self.dirty = true;
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
        self.key(event, false);
    }

    fn repeat_key(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: u32,
        event: KeyEvent,
    ) {
        self.key(event, true);
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
        self.mods = Mods {
            ctrl: modifiers.ctrl,
            alt: modifiers.alt,
            shift: modifiers.shift,
        };
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
            if let Some(m) = &self.menu
                && &event.surface == m.popup.wl_surface()
            {
                self.menu_pointer(event);
                continue;
            }
            if &event.surface != self.window.wl_surface() {
                continue;
            }
            self.ptr = event.position;
            let (x, y) = self.skin_pos(event.position);
            let live = self.engine.live.clone();
            let live = live.lock().unwrap();
            let actions = self.pointer_event(&live, event, x, y);
            drop(live);
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

impl App {
    fn pointer_event(&mut self, live: &Live, event: &PointerEvent, x: i32, y: i32) -> Vec<Action> {
        match event.kind {
            PointerEventKind::Enter { .. } => {
                self.set_cursor = true;
                vec![]
            }
            PointerEventKind::Leave { .. } => {
                // A compositor bind can take the pointer mid-drag, with no
                // release to follow.
                if self.ui.view.press.is_some() {
                    self.ui.pointer_lost();
                    self.dirty = true;
                }
                vec![]
            }
            PointerEventKind::Motion { .. } => {
                let a = self.ui.pointer_move(live, x, y);
                if self.ui.view.pl_menu.is_some() || self.ui.view.press.is_some() {
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
                self.dirty = true;
                self.ui.pointer_down(live, x, y, self.mods)
            }
            PointerEventKind::Press {
                button: BTN_RIGHT,
                serial,
                ..
            } => {
                self.press_serial = serial;
                self.dirty = true;
                self.ui.pointer_menu(x, y)
            }
            PointerEventKind::Release {
                button: BTN_LEFT, ..
            } => {
                self.dirty = true;
                self.ui.pointer_up(live, x, y)
            }
            PointerEventKind::Axis { vertical, .. } => {
                // A wheel notch is 120; a touchpad sends pixels, about 15 a
                // step. Either way, only whole steps scroll.
                let steps = if vertical.value120 != 0 {
                    self.scroll_acc.0 += vertical.value120;
                    let n = self.scroll_acc.0 / 120;
                    self.scroll_acc.0 -= n * 120;
                    n
                } else if vertical.discrete != 0 {
                    vertical.discrete
                } else {
                    self.scroll_acc.1 += vertical.absolute;
                    let n = (self.scroll_acc.1 / 15.0).trunc();
                    self.scroll_acc.1 -= n * 15.0;
                    n as i32
                };
                if vertical.stop {
                    self.scroll_acc = (0, 0.0);
                }
                if steps != 0 {
                    self.dirty = true;
                    self.ui.scroll(live, x, y, steps)
                } else {
                    vec![]
                }
            }
            _ => vec![],
        }
    }
}

impl PopupHandler for App {
    fn configure(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        popup: &Popup,
        _: PopupConfigure,
    ) {
        if let Some(m) = self.menu.as_mut()
            && m.popup.wl_surface() == popup.wl_surface()
        {
            m.configured = true;
            self.draw_menu();
        }
    }

    fn done(&mut self, _: &Connection, _: &QueueHandle<Self>, popup: &Popup) {
        if self
            .menu
            .as_ref()
            .is_some_and(|m| m.popup.wl_surface() == popup.wl_surface())
        {
            self.close_menu();
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
delegate_xdg_popup!(App);
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
            // Decode on bytes: a stray '%' next to a multi-byte character
            // must not split it.
            let bytes = p.as_bytes();
            let hex = |b: u8| (b as char).to_digit(16).map(|d| d as u8);
            let mut out = Vec::with_capacity(bytes.len());
            let mut i = 0;
            while i < bytes.len() {
                if bytes[i] == b'%'
                    && i + 2 < bytes.len()
                    && let (Some(h), Some(l)) = (hex(bytes[i + 1]), hex(bytes[i + 2]))
                {
                    out.push(h << 4 | l);
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
        // Only a drop we accepted (one offering file paths) is ours to read.
        if !offer.with_mime_types(|m| m.iter().any(|t| t == URI_LIST)) {
            offer.destroy();
            return;
        }
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
        let tx = self.msgs.clone();
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
            let note = match skin {
                Some(p) => match crate::skins::wear(&p.to_string_lossy()) {
                    Ok(_) => return,
                    Err(e) => format!("Could not wear that skin: {e:#}"),
                },
                None => "Drop a .wsz skin here to wear it".into(),
            };
            let _ = tx.send(Msg::Note(note));
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

    #[test]
    fn stray_percent_does_not_split_characters() {
        let files = super::uri_files("file:///x/%a\u{e9}.wsz\nfile:///y/%+f.wsz\n");
        assert_eq!(
            files,
            vec![
                std::path::PathBuf::from("/x/%a\u{e9}.wsz"),
                std::path::PathBuf::from("/y/%+f.wsz")
            ]
        );
    }

    #[test]
    fn sizes_are_whole_pixels_at_every_scale() {
        for n in [120, 150, 180, 210, 240] {
            let step = 120 / super::gcd(n, 120);
            for px in [550, 551, 1100, 908] {
                let logical = (px * 120 + n - 1) / n;
                let logical = (logical + step - 1) / step * step;
                assert_eq!(logical * n % 120, 0, "scale {n}, {px}px");
                assert!(logical * n / 120 >= px);
            }
        }
    }
}

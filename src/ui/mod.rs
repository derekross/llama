//! The three Winamp windows, stacked in one surface: main, equalizer,
//! playlist. Everything here is in skin pixels; the Wayland layer scales.

mod eq;
pub mod font;
mod main_win;
mod playlist;

use std::collections::BTreeSet;
use std::time::Instant;

use serde_json::{Value, json};

use crate::cliamp::engine::Live;
use crate::render::Canvas;
use crate::skin::sprites::{self as sp, R};
use crate::skin::{Sheet, Skin, region};

pub use playlist::Menu as PlMenuKind;

/// What the window should do in response to input.
#[derive(Debug, PartialEq)]
pub enum Action {
    /// A cliamp operation.
    Op(String, Value),
    /// Start an interactive move (title bar drag).
    Move,
    Close,
    Minimize,
    /// The surface size changed (shade, EQ/PL toggles, playlist resize).
    Resize,
    /// Zoom changed (clutterbar D, Ctrl+D).
    Zoom,
    /// Ask the engine for data the UI will read back from `Live::replies`.
    Query(String, String, Value),
}

pub fn op(name: &str, params: Value) -> Action {
    Action::Op(name.to_string(), params)
}

/// What the pointer went down on.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Target {
    Options,
    Minimize,
    Shade,
    Close,
    Transport(usize),
    Shuffle,
    Repeat,
    EqToggle,
    PlToggle,
    Time,
    Vis,
    Clutter(usize),
    Volume,
    Balance,
    Seek,
    MainTitle,
    EqTitle,
    EqClose,
    EqOn,
    EqAuto,
    EqPresets,
    /// 0..=9 are bands, 10 is the preamp.
    EqBand(usize),
    PlTitle,
    PlClose,
    PlRow(usize),
    PlScroll,
    PlResize,
    PlMenu(PlMenuKind),
    ShadeSeek,
}

/// A free-text box drawn over the playlist (Add URL, Save list).
#[derive(Clone, Debug)]
pub struct Prompt {
    pub title: String,
    pub text: String,
    pub kind: PromptKind,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PromptKind {
    AddLocation,
    SavePlaylist,
}

/// A pick-one list drawn over the playlist (Load list, EQ presets).
#[derive(Clone, Debug)]
pub struct Chooser {
    pub title: String,
    pub items: Vec<String>,
    pub sel: usize,
    pub kind: ChooserKind,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ChooserKind {
    LoadPlaylist,
    EqPreset,
}

pub struct View {
    pub zoom: i32,
    pub shaded: bool,
    pub eq_open: bool,
    pub pl_open: bool,
    /// Playlist height in skin pixels: 116 + n·29.
    pub pl_h: i32,
    pub time_remaining: bool,
    pub vis_on: bool,
    pub focused: bool,
    pub press: Option<Target>,
    /// Where a slider drag currently sits, 0..=1 (seek, volume, balance, EQ).
    pub drag: Option<f64>,
    pub hover: (i32, i32),
    pub pl_scroll: usize,
    pub pl_sel: BTreeSet<usize>,
    pub pl_last_click: Option<(usize, Instant)>,
    pub pl_menu: Option<PlMenuKind>,
    pub pl_resize_from: Option<(i32, i32)>,
    pub prompt: Option<Prompt>,
    pub chooser: Option<Chooser>,
    pub eq_saved: Option<Vec<f64>>,
    pub last_title_click: Option<Instant>,
}

impl Default for View {
    fn default() -> Self {
        View {
            zoom: 2,
            shaded: false,
            eq_open: true,
            pl_open: true,
            pl_h: sp::PL_MIN_H + sp::PL_STEP_H * 2,
            time_remaining: false,
            vis_on: true,
            focused: true,
            press: None,
            drag: None,
            hover: (0, 0),
            pl_scroll: 0,
            pl_sel: BTreeSet::new(),
            pl_last_click: None,
            pl_menu: None,
            pl_resize_from: None,
            prompt: None,
            chooser: None,
            eq_saved: None,
            last_title_click: None,
        }
    }
}

pub struct Ui {
    pub skin: Skin,
    pub font: font::PlFont,
    pub canvas: Canvas,
    pub view: View,
    marquee_start: Instant,
    marquee_text: String,
    peaks: [f32; 19],
    bars: [f32; 19],
    masks: Masks,
}

#[derive(Default)]
struct Masks {
    main: Option<Vec<bool>>,
    shade: Option<Vec<bool>>,
    eq: Option<Vec<bool>>,
}

/// Where each window sits in the stack.
pub struct Layout {
    pub main: R,
    pub eq: Option<R>,
    pub pl: Option<R>,
}

impl Ui {
    pub fn new(skin: Skin, view: View) -> Ui {
        let font = font::PlFont::new(&skin.pl_colors().font);
        let mut ui = Ui {
            skin,
            font,
            canvas: Canvas::new(sp::MAIN_W, sp::MAIN_H),
            view,
            marquee_start: Instant::now(),
            marquee_text: String::new(),
            peaks: [0.0; 19],
            bars: [0.0; 19],
            masks: Masks::default(),
        };
        ui.build_masks();
        ui
    }

    pub fn set_skin(&mut self, skin: Skin) {
        self.font.set_family(&skin.pl_colors().font);
        self.skin = skin;
        self.build_masks();
    }

    fn build_masks(&mut self) {
        let r = &self.skin.region;
        self.masks = Masks {
            main: r
                .normal
                .as_ref()
                .map(|p| region::mask(p, sp::MAIN_W, sp::MAIN_H)),
            shade: r
                .shade
                .as_ref()
                .map(|p| region::mask(p, sp::MAIN_W, sp::SHADE_H)),
            eq: r.eq.as_ref().map(|p| region::mask(p, sp::MAIN_W, sp::EQ_H)),
        };
    }

    pub fn layout(&self) -> Layout {
        let main_h = if self.view.shaded {
            sp::SHADE_H
        } else {
            sp::MAIN_H
        };
        let main = sp::r(0, 0, sp::MAIN_W, main_h);
        let mut y = main_h;
        let eq = self.view.eq_open.then(|| {
            let r = sp::r(0, y, sp::MAIN_W, sp::EQ_H);
            y += sp::EQ_H;
            r
        });
        let pl = self
            .view
            .pl_open
            .then(|| sp::r(0, y, sp::PL_MIN_W, self.view.pl_h));
        Layout { main, eq, pl }
    }

    /// Size in skin pixels.
    pub fn size(&self) -> (i32, i32) {
        let l = self.layout();
        let bottom = l.pl.or(l.eq).unwrap_or(l.main);
        (sp::MAIN_W, bottom.y + bottom.h)
    }

    /// Composes the whole surface onto `self.canvas`.
    pub fn draw(&mut self, live: &Live) {
        let (w, h) = self.size();
        self.canvas.resize(w, h);
        self.font.begin();
        let l = self.layout();
        self.draw_main(live, l.main);
        if let Some(r) = l.eq {
            self.draw_eq(live, r);
        }
        if let Some(r) = l.pl {
            self.draw_playlist(live, r);
        }
        self.compose_mask(&l);
    }

    /// The last drawn frame at `zoom`, text included, as a PNG.
    pub fn to_png(&mut self, path: &std::path::Path, zoom: i32) -> anyhow::Result<()> {
        let (w, h) = (self.canvas.w, self.canvas.h);
        let mut buf = vec![0u8; (w * zoom * h * zoom * 4) as usize];
        self.canvas.scale_into(&mut buf, zoom);
        self.font.paint(&mut buf, (w * zoom) as usize, zoom);
        self.canvas.save_scaled(path, buf, zoom)
    }

    fn compose_mask(&mut self, l: &Layout) {
        let main = if self.view.shaded {
            &self.masks.shade
        } else {
            &self.masks.main
        };
        let eq = if l.eq.is_some() {
            &self.masks.eq
        } else {
            &None
        };
        if main.is_none() && eq.is_none() {
            self.canvas.mask = None;
            return;
        }
        let (w, h) = (self.canvas.w, self.canvas.h);
        let mut m = vec![true; (w * h) as usize];
        let mut put = |src: &Option<Vec<bool>>, r: R| {
            if let Some(src) = src {
                for y in 0..r.h {
                    for x in 0..r.w {
                        m[((r.y + y) * w + x) as usize] = src[(y * r.w + x) as usize];
                    }
                }
            }
        };
        put(main, l.main);
        if let Some(r) = l.eq {
            put(eq, r);
        }
        self.canvas.mask = Some(m);
    }

    /// Whether anything animates, so the window knows to keep drawing.
    pub fn animating(&self, live: &Live) -> bool {
        let scrolling = self.marquee_text.chars().count() as i32 * sp::CHAR_W > sp::MARQUEE.w;
        live.snap.playing() || live.busy() || scrolling || self.peaks.iter().any(|&p| p > 0.0)
    }

    // ---- input -------------------------------------------------------------

    fn target_at(&self, live: &Live, x: i32, y: i32) -> Option<Target> {
        let l = self.layout();
        if l.main.contains(x, y) {
            return self.main_target(x, y);
        }
        if let Some(r) = l.eq.filter(|r| r.contains(x, y)) {
            return self.eq_target(x - r.x, y - r.y);
        }
        if let Some(r) = l.pl.filter(|r| r.contains(x, y)) {
            return self.pl_target(live, x - r.x, y - r.y, r);
        }
        None
    }

    pub fn pointer_down(&mut self, live: &Live, x: i32, y: i32) -> Vec<Action> {
        self.view.hover = (x, y);
        // An open chooser or prompt takes every click.
        if self.view.chooser.is_some() || self.view.prompt.is_some() {
            return self.overlay_click(x, y);
        }
        if let Some(kind) = self.view.pl_menu {
            // A click elsewhere closes an open playlist menu.
            if let Some(item) = self.pl_menu_item(kind, x, y) {
                self.view.pl_menu = None;
                return self.pl_menu_action(live, kind, item);
            }
            self.view.pl_menu = None;
        }
        let Some(t) = self.target_at(live, x, y) else {
            return vec![];
        };
        self.view.press = Some(t);
        self.view.drag = None;
        let mut out = vec![];
        match t {
            Target::MainTitle | Target::EqTitle | Target::PlTitle => {
                let now = Instant::now();
                let double = self
                    .view
                    .last_title_click
                    .is_some_and(|t| now.duration_since(t).as_millis() < 400);
                self.view.last_title_click = Some(now);
                self.view.press = None;
                if double && t == Target::MainTitle {
                    self.view.shaded = !self.view.shaded;
                    out.push(Action::Resize);
                } else {
                    out.push(Action::Move);
                }
            }
            Target::Volume
            | Target::Balance
            | Target::Seek
            | Target::ShadeSeek
            | Target::EqBand(_) => {
                out.extend(self.slider_drag(live, t, x, y));
            }
            Target::PlRow(i) => out.extend(self.pl_row_click(i)),
            Target::PlScroll => out.extend(self.pl_scroll_drag(live, y)),
            Target::PlResize => self.view.pl_resize_from = Some((y, self.view.pl_h)),
            Target::PlMenu(kind) => self.view.pl_menu = Some(kind),
            _ => {}
        }
        out
    }

    pub fn pointer_move(&mut self, live: &Live, x: i32, y: i32) -> Vec<Action> {
        self.view.hover = (x, y);
        match self.view.press {
            Some(t @ (Target::Volume | Target::Balance | Target::EqBand(_))) => {
                self.slider_drag(live, t, x, y)
            }
            Some(t @ (Target::Seek | Target::ShadeSeek)) => {
                self.slider_drag(live, t, x, y);
                vec![]
            }
            Some(Target::PlScroll) => self.pl_scroll_drag(live, y),
            Some(Target::PlResize) => {
                let Some((y0, h0)) = self.view.pl_resize_from else {
                    return vec![];
                };
                let steps = ((y - y0) as f32 / sp::PL_STEP_H as f32).round() as i32;
                let h = (h0 + steps * sp::PL_STEP_H)
                    .clamp(sp::PL_MIN_H, sp::PL_MIN_H + sp::PL_STEP_H * 20);
                if h != self.view.pl_h {
                    self.view.pl_h = h;
                    return vec![Action::Resize];
                }
                vec![]
            }
            _ => vec![],
        }
    }

    pub fn pointer_up(&mut self, live: &Live, x: i32, y: i32) -> Vec<Action> {
        let Some(t) = self.view.press.take() else {
            return vec![];
        };
        let drag = self.view.drag.take();
        self.view.pl_resize_from = None;
        // Sliders commit where they were let go; buttons only fire when the
        // pointer is still on them, as in Winamp.
        match t {
            Target::Seek | Target::ShadeSeek => {
                let d = live.snap.duration;
                if let (Some(r), true) = (drag, d > 0.0 && live.snap.seekable) {
                    return vec![op("seek.absolute", json!({"value": (r * d).round()}))];
                }
                return vec![];
            }
            Target::Volume
            | Target::Balance
            | Target::EqBand(_)
            | Target::PlScroll
            | Target::PlResize => return vec![],
            Target::PlMenu(kind) => {
                // Pressing a menu button opens it; dragging to an item and
                // letting go picks it. Letting go on the bottom item (which
                // covers the button itself) leaves the menu open for a click.
                if let Some(item) = self.pl_menu_item(kind, x, y)
                    && item + 1 != kind.spec().items.len()
                {
                    self.view.pl_menu = None;
                    return self.pl_menu_action(live, kind, item);
                }
                return vec![];
            }
            _ => {}
        }
        if self.target_at(live, x, y) != Some(t) {
            return vec![];
        }
        self.click(live, t)
    }

    fn click(&mut self, live: &Live, t: Target) -> Vec<Action> {
        let s = &live.snap;
        match t {
            Target::Transport(0) => vec![op("prev", json!({}))],
            Target::Transport(1) => {
                if s.paused() {
                    vec![op("play", json!({}))]
                } else if s.playing() {
                    // Winamp's play restarts the current track.
                    vec![op("seek.absolute", json!({"value": 0}))]
                } else {
                    vec![op("play", json!({}))]
                }
            }
            Target::Transport(2) => vec![op("toggle", json!({}))],
            Target::Transport(3) => vec![op("stop", json!({}))],
            Target::Transport(4) => vec![op("next", json!({}))],
            Target::Transport(5) => {
                self.open_prompt(PromptKind::AddLocation);
                vec![Action::Resize]
            }
            Target::Shuffle => vec![op("shuffle", json!({"name": "toggle"}))],
            Target::Repeat => {
                vec![op(
                    "repeat",
                    json!({"name": if s.repeat_on() { "off" } else { "all" }}),
                )]
            }
            Target::EqToggle => {
                self.view.eq_open = !self.view.eq_open;
                vec![Action::Resize]
            }
            Target::PlToggle => {
                self.view.pl_open = !self.view.pl_open;
                vec![Action::Resize]
            }
            Target::Time => {
                self.view.time_remaining = !self.view.time_remaining;
                vec![]
            }
            Target::Vis => {
                self.view.vis_on = !self.view.vis_on;
                vec![]
            }
            Target::Clutter(3) => {
                self.view.zoom = if self.view.zoom >= 4 {
                    2
                } else {
                    self.view.zoom * 2
                };
                vec![Action::Zoom]
            }
            Target::Shade => {
                self.view.shaded = !self.view.shaded;
                vec![Action::Resize]
            }
            Target::Close => vec![Action::Close],
            Target::Minimize => vec![Action::Minimize],
            Target::EqClose => {
                self.view.eq_open = false;
                vec![Action::Resize]
            }
            Target::PlClose => {
                self.view.pl_open = false;
                vec![Action::Resize]
            }
            Target::EqOn => self.eq_on_toggle(live),
            Target::EqPresets => {
                self.open_eq_presets(live);
                vec![Action::Resize]
            }
            _ => vec![],
        }
    }

    pub fn scroll(&mut self, live: &Live, x: i32, y: i32, dy: f64) -> Vec<Action> {
        let l = self.layout();
        if let Some(r) = l.pl.filter(|r| r.contains(x, y)) {
            let rows = live.queue.len();
            let visible = self.pl_visible_rows(r) as usize;
            let max = rows.saturating_sub(visible);
            let step = if dy > 0.0 { 3 } else { -3 };
            self.view.pl_scroll = (self.view.pl_scroll as i64 + step).clamp(0, max as i64) as usize;
            return vec![];
        }
        let delta = if dy > 0.0 { -1.0 } else { 1.0 };
        vec![op("volume.adjust", json!({"value": delta}))]
    }

    // ---- keyboard ------------------------------------------------------------

    /// Winamp's classic keys. `text` is the typed character, if any.
    pub fn key(&mut self, live: &Live, key: Key, text: Option<&str>, ctrl: bool) -> Vec<Action> {
        if self.view.prompt.is_some() {
            return self.prompt_key(live, key, text);
        }
        if self.view.chooser.is_some() {
            return self.chooser_key(key);
        }
        if ctrl {
            return match text.map(|t| t.to_ascii_lowercase()).as_deref() {
                Some("d") => self.click(live, Target::Clutter(3)),
                Some("w") => self.click(live, Target::Shade),
                _ => vec![],
            };
        }
        match key {
            Key::Up => vec![op("volume.adjust", json!({"value": 1}))],
            Key::Down => vec![op("volume.adjust", json!({"value": -1}))],
            Key::Left => vec![op("seek", json!({"value": -5}))],
            Key::Right => vec![op("seek", json!({"value": 5}))],
            Key::Delete => self.pl_remove_selected(live),
            Key::Escape => {
                self.view.pl_menu = None;
                vec![]
            }
            Key::Enter => match self.view.pl_sel.first() {
                Some(&i) => vec![op("queue.play", json!({"index": i}))],
                None => vec![],
            },
            _ => match text.map(|t| t.to_ascii_lowercase()).as_deref() {
                Some("z") => self.click(live, Target::Transport(0)),
                Some("x") => self.click(live, Target::Transport(1)),
                Some("c") => self.click(live, Target::Transport(2)),
                Some("v") => self.click(live, Target::Transport(3)),
                Some("b") => self.click(live, Target::Transport(4)),
                Some("l") => self.click(live, Target::Transport(5)),
                Some("s") => self.click(live, Target::Shuffle),
                Some("r") => self.click(live, Target::Repeat),
                Some("g") => self.click(live, Target::EqToggle),
                Some("e") => self.click(live, Target::PlToggle),
                _ => vec![],
            },
        }
    }

    // ---- overlays (prompt, chooser) ---------------------------------------

    pub fn open_prompt(&mut self, kind: PromptKind) {
        let title = match kind {
            PromptKind::AddLocation => "Add file, folder or URL:",
            PromptKind::SavePlaylist => "Save playlist as:",
        };
        self.view.chooser = None;
        self.view.prompt = Some(Prompt {
            title: title.into(),
            text: String::new(),
            kind,
        });
        self.view.pl_open = true;
    }

    pub fn open_chooser(&mut self, kind: ChooserKind, title: &str, items: Vec<String>, sel: usize) {
        self.view.prompt = None;
        self.view.chooser = Some(Chooser {
            title: title.into(),
            items,
            sel,
            kind,
        });
        self.view.pl_open = true;
    }

    fn prompt_key(&mut self, live: &Live, key: Key, text: Option<&str>) -> Vec<Action> {
        let Some(p) = self.view.prompt.as_mut() else {
            return vec![];
        };
        match key {
            Key::Escape => self.view.prompt = None,
            Key::Backspace => {
                p.text.pop();
            }
            Key::Enter => {
                let p = self.view.prompt.take().unwrap();
                let text = expand_home(p.text.trim());
                if text.is_empty() {
                    return vec![];
                }
                return match p.kind {
                    PromptKind::AddLocation => vec![op("queue", json!({"path": text}))],
                    PromptKind::SavePlaylist => vec![op(
                        "playlist.replace",
                        json!({"provider": "local", "playlist": text, "tracks": live.queue_raw}),
                    )],
                };
            }
            _ => {
                if let Some(t) = text.filter(|t| !t.chars().any(char::is_control)) {
                    p.text.push_str(t);
                }
            }
        }
        vec![]
    }

    fn chooser_key(&mut self, key: Key) -> Vec<Action> {
        let Some(c) = self.view.chooser.as_mut() else {
            return vec![];
        };
        match key {
            Key::Escape => self.view.chooser = None,
            Key::Up => c.sel = c.sel.saturating_sub(1),
            Key::Down => c.sel = (c.sel + 1).min(c.items.len().saturating_sub(1)),
            Key::Enter => return self.choose(),
            _ => {}
        }
        vec![]
    }

    fn choose(&mut self) -> Vec<Action> {
        let Some(c) = self.view.chooser.take() else {
            return vec![];
        };
        let Some(item) = c.items.get(c.sel).cloned() else {
            return vec![];
        };
        match c.kind {
            ChooserKind::LoadPlaylist => vec![op(
                "provider.load",
                json!({"provider": "local", "playlist": item}),
            )],
            ChooserKind::EqPreset => vec![op("eq", json!({"name": item}))],
        }
    }

    fn overlay_click(&mut self, x: i32, y: i32) -> Vec<Action> {
        let Some(r) = self.layout().pl else {
            self.view.prompt = None;
            self.view.chooser = None;
            return vec![];
        };
        let body = playlist::body(r);
        if !body.contains(x, y) {
            self.view.prompt = None;
            self.view.chooser = None;
            return vec![];
        }
        if let Some(c) = self.view.chooser.as_mut() {
            let row = (y - body.y - font::LINE_H - 2) / font::LINE_H;
            if row >= 0 && (row as usize) < c.items.len() {
                let first = playlist::chooser_first(c, body);
                let i = first + row as usize;
                if i < c.items.len() {
                    if c.sel == i {
                        return self.choose();
                    }
                    c.sel = i;
                }
            }
        }
        vec![]
    }

    /// Feeds an answer to an earlier `Action::Query` back in.
    pub fn reply(&mut self, tag: &str, result: &Value) -> Vec<Action> {
        if tag == "playlists" {
            let items: Vec<String> = result["playlists"]
                .as_array()
                .map(|a| {
                    a.iter()
                        .filter_map(|v| {
                            v["name"]
                                .as_str()
                                .or_else(|| v.as_str())
                                .map(str::to_string)
                        })
                        .collect()
                })
                .unwrap_or_default();
            if !items.is_empty() {
                self.open_chooser(ChooserKind::LoadPlaylist, "Load playlist:", items, 0);
                return vec![Action::Resize];
            }
        }
        vec![]
    }

    pub fn has_sheet(&self, s: Sheet) -> bool {
        self.skin.sheet(s).is_some()
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Key {
    Up,
    Down,
    Left,
    Right,
    Enter,
    Escape,
    Backspace,
    Delete,
    Other,
}

fn expand_home(p: &str) -> String {
    if let Some(rest) = p.strip_prefix("~/")
        && let Some(home) = std::env::var_os("HOME")
    {
        return format!("{}/{rest}", home.to_string_lossy());
    }
    p.to_string()
}

/// m:ss, or h:mm:ss past an hour.
pub fn clock(secs: f64) -> String {
    let s = secs.max(0.0) as u64;
    if s >= 3600 {
        format!("{}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60)
    } else {
        format!("{}:{:02}", s / 60, s % 60)
    }
}

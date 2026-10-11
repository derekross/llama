//! The playlist editor: pledit.bmp chrome around cliamp's live queue, the
//! ADD / REM / SEL / LIST pop-up menus, and the prompt and chooser overlays.

use std::time::Instant;

use serde_json::json;

use super::{Action, ChooserKind, Key, Mods, Pick, PromptKind, Target, Ui, clock, font, op};
use crate::cliamp::engine::Live;
use crate::skin::Sheet;
use crate::skin::sprites::{self as sp, PlMenu, R};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Menu {
    Add,
    Rem,
    Sel,
    Misc,
    List,
}

impl Menu {
    pub fn spec(self) -> &'static PlMenu {
        match self {
            Menu::Add => &sp::PL_ADD,
            Menu::Rem => &sp::PL_REM,
            Menu::Sel => &sp::PL_SEL,
            Menu::Misc => &sp::PL_MISC,
            Menu::List => &sp::PL_LIST,
        }
    }
}

/// The text area inside the chrome, in canvas coordinates.
pub fn body(r: R) -> R {
    sp::r(
        r.x + sp::PL_LEFT_W,
        r.y + sp::PL_TOP_H,
        r.w - sp::PL_LEFT_W - sp::PL_RIGHT_W,
        r.h - sp::PL_TOP_H - sp::PL_BOTTOM_H,
    )
}

/// The first chooser row shown, keeping the selection in view.
pub fn chooser_first(c: &super::Chooser, body: R) -> usize {
    let rows = ((body.h - font::LINE_H - 2) / font::LINE_H).max(1) as usize;
    c.sel.saturating_sub(rows - 1)
}

pub const EQ_PRESETS: [&str; 16] = [
    "Flat",
    "Rock",
    "Pop",
    "Jazz",
    "Classical",
    "Bass Boost",
    "Treble Boost",
    "Vocal",
    "Electronic",
    "Acoustic",
    "Hip-Hop",
    "R&B",
    "Loudness",
    "Late Night",
    "Podcast",
    "Small Speakers",
];

impl Ui {
    pub(super) fn pl_visible_rows(&self, r: R) -> i32 {
        (body(r).h / font::LINE_H).max(1)
    }

    /// The local rectangle of a menu's button in a w×h playlist.
    pub(super) fn pl_menu_button(&self, kind: Menu, w: i32, h: i32) -> R {
        let x = if kind == Menu::List {
            w - kind.spec().x
        } else {
            kind.spec().x
        };
        sp::r(x, h - 30, 22, 18)
    }

    /// The item of an open menu under canvas point (x, y).
    pub(super) fn pl_menu_item(&self, kind: Menu, x: i32, y: i32) -> Option<usize> {
        let r = self.layout().pl?;
        let (lx, ly) = (x - r.x, y - r.y);
        let b = self.pl_menu_button(kind, r.w, r.h);
        let n = kind.spec().items.len() as i32;
        (0..n)
            .find(|&i| sp::r(b.x, b.y - (n - 1 - i) * 18, 22, 18).contains(lx, ly))
            .map(|i| i as usize)
    }

    pub(super) fn draw_playlist(&mut self, live: &Live, r: R) {
        let img = self.skin.sheet(Sheet::Pledit);
        let colors = self.skin.pl_colors();
        let focused = self.view.focused;
        let c = &mut self.canvas;
        let (w, h) = (r.w, r.h);
        let at = |x: i32, y: i32| (r.x + x, r.y + y);

        // Top: corners, the centered title, tiles between.
        let pick = |a: R, b: R| if focused { b } else { a };
        let mut x = 25;
        while x < w - 25 {
            c.blit(img, pick(sp::PL_TOP_TILE, sp::PL_TOP_TILE_ACTIVE), at(x, 0));
            x += 25;
        }
        c.blit(img, pick(sp::PL_TOP_LEFT, sp::PL_TOP_LEFT_ACTIVE), at(0, 0));
        c.blit(
            img,
            pick(sp::PL_TITLE, sp::PL_TITLE_ACTIVE),
            at((w - 100) / 2, 0),
        );
        c.blit(
            img,
            pick(sp::PL_TOP_RIGHT, sp::PL_TOP_RIGHT_ACTIVE),
            at(w - 25, 0),
        );
        if self.view.press == Some(Target::PlClose) {
            c.blit(img, sp::PL_CLOSE_DOWN, at(w - 11, 3));
        }

        // Sides.
        let mut y = sp::PL_TOP_H;
        while y < h - sp::PL_BOTTOM_H {
            c.blit(img, sp::PL_LEFT_TILE, at(0, y));
            c.blit(img, sp::PL_RIGHT_TILE, at(w - sp::PL_RIGHT_W, y));
            y += 29;
        }

        // Bottom.
        let mut x = 125;
        while x < w - 150 {
            c.blit(img, sp::PL_BOTTOM_TILE, at(x, h - sp::PL_BOTTOM_H));
            x += 25;
        }
        c.blit(img, sp::PL_BOTTOM_LEFT, at(0, h - sp::PL_BOTTOM_H));
        c.blit(img, sp::PL_BOTTOM_RIGHT, at(w - 150, h - sp::PL_BOTTOM_H));

        // Text area.
        let b = body(r);
        c.fill(b, colors.normal_bg);
        if self.view.prompt.is_some() || self.view.chooser.is_some() {
            self.draw_overlay(b);
        } else {
            self.draw_rows(live, b);
        }

        // Scroll handle.
        let menu = self.view.pl_menu.map(|kind| {
            let hover = self.pl_menu_item(kind, self.view.hover.0, self.view.hover.1);
            (kind, self.pl_menu_button(kind, w, h), hover)
        });
        let img = self.skin.sheet(Sheet::Pledit);
        let c = &mut self.canvas;
        let total = live.queue.len();
        let visible = (b.h / font::LINE_H) as usize;
        let track = b.h - 18;
        let max = total.saturating_sub(visible);
        let hy = if max == 0 {
            0
        } else {
            (self.view.pl_scroll.min(max) as i32 * track) / max as i32
        };
        let handle = if self.view.press == Some(Target::PlScroll) {
            sp::PL_SCROLL_ACTIVE
        } else {
            sp::PL_SCROLL
        };
        c.blit(img, handle, (r.x + w - 15, b.y + hy));

        // Running time: "selected/total" in text.bmp glyphs.
        let total_secs: f64 = live.queue.iter().filter_map(|t| t.duration).sum();
        let sel_secs: f64 = self
            .view
            .pl_sel
            .iter()
            .filter_map(|&i| live.queue.get(i)?.duration)
            .sum();
        let unknown = live.queue.iter().any(|t| t.duration.is_none());
        let running = format!(
            "{}/{}{}",
            clock(sel_secs),
            clock(total_secs),
            if unknown { "+" } else { "" }
        );
        let font = self.skin.sheet(Sheet::Text);
        c.text(font, &running, sp::r(r.x + w - 143, r.y + h - 28, 90, 6), 0);
        // Mini time: the current position, like the main LCD.
        if live.snap.playing() || live.snap.paused() {
            let t = clock(live.position());
            c.text(
                font,
                &format!("{t:>5}"),
                sp::r(r.x + w - 84, r.y + h - 15, 25, 6),
                0,
            );
        }

        // An open menu sits above its button.
        if let Some((kind, btn, hover)) = menu {
            let spec = kind.spec();
            let n = spec.items.len() as i32;
            let top = btn.y - (n - 1) * 18;
            c.blit(img, spec.bar, at(btn.x - 3, top));
            self.font
                .cover(sp::r(btn.x - 3, top, 3 + spec.items[0].0.w, n * 18));
            for (i, (item, down)) in spec.items.iter().enumerate() {
                let s = if hover == Some(i) { *down } else { *item };
                c.blit(img, s, at(btn.x, top + i as i32 * 18));
            }
        }
    }

    fn draw_rows(&mut self, live: &Live, b: R) {
        let colors = self.skin.pl_colors();
        let visible = (b.h / font::LINE_H) as usize;
        let max = live.queue.len().saturating_sub(visible);
        self.view.pl_scroll = self.view.pl_scroll.min(max);
        let current = live.snap.current();
        for row in 0..visible {
            let i = self.view.pl_scroll + row;
            let Some(t) = live.queue.get(i) else { break };
            let y = b.y + row as i32 * font::LINE_H;
            let line = sp::r(b.x, y, b.w, font::LINE_H);
            if self.view.pl_sel.contains(&i) {
                self.canvas.fill(line, colors.selected_bg);
            }
            let color = if current == Some(i) {
                colors.current
            } else {
                colors.normal
            };
            let dur = t.duration.map(clock).unwrap_or_default();
            let dur_w = self.font.width(&dur);
            let text_clip = sp::r(b.x, y, b.w - dur_w - 6, font::LINE_H);
            self.font.draw(
                &format!("{}. {}", i + 1, t.display()),
                b.x + 2,
                y,
                color,
                text_clip,
            );
            if !dur.is_empty() {
                self.font.draw(&dur, b.x + b.w - dur_w - 3, y, color, line);
            }
        }
    }

    fn draw_overlay(&mut self, b: R) {
        let colors = self.skin.pl_colors();
        if let Some(p) = self.view.prompt.clone() {
            self.font
                .draw(&p.title, b.x + 2, b.y + 1, colors.current, b);
            let field = sp::r(b.x + 2, b.y + font::LINE_H + 3, b.w - 4, font::LINE_H + 1);
            self.canvas.fill(field, colors.selected_bg);
            // Keep the end of long text (and the caret) in view.
            let mut shown = format!("{}_", p.text);
            while self.font.width(&shown) > field.w - 4 && shown.chars().count() > 1 {
                shown.remove(0);
            }
            self.font
                .draw(&shown, field.x + 2, field.y, colors.current, field);
            let hint = "Enter to add, Esc to cancel";
            self.font
                .draw(hint, b.x + 2, field.y + font::LINE_H + 4, colors.normal, b);
        } else if let Some(ch) = self.view.chooser.clone() {
            self.font
                .draw(&ch.title, b.x + 2, b.y + 1, colors.current, b);
            let first = chooser_first(&ch, b);
            let rows = ((b.h - font::LINE_H - 2) / font::LINE_H).max(1) as usize;
            for (row, item) in ch.items.iter().enumerate().skip(first).take(rows) {
                let y = b.y + font::LINE_H + 2 + (row - first) as i32 * font::LINE_H;
                let line = sp::r(b.x, y, b.w, font::LINE_H);
                if row == ch.sel {
                    self.canvas.fill(line, colors.selected_bg);
                }
                self.font.draw(item, b.x + 4, y, colors.normal, line);
            }
        }
    }

    pub(super) fn pl_target(&self, live: &Live, x: i32, y: i32, r: R) -> Option<Target> {
        let (w, h) = (r.w, r.h);
        if sp::r(w - 11, 3, 9, 9).contains(x, y) {
            return Some(Target::PlClose);
        }
        if y < sp::PL_TOP_H {
            return Some(Target::PlTitle);
        }
        if sp::r(w - 20, h - 20, 20, 20).contains(x, y) {
            return Some(Target::PlResize);
        }
        for kind in [Menu::Add, Menu::Rem, Menu::Sel, Menu::Misc, Menu::List] {
            if self.pl_menu_button(kind, w, h).contains(x, y) {
                return Some(Target::PlMenu(kind));
            }
        }
        let (mx, my) = sp::PL_MINI_TRANSPORT_AT;
        let (mx, my) = (w - 150 + mx, h - sp::PL_BOTTOM_H + my);
        if (my..my + 10).contains(&y) && (mx..mx + 60).contains(&x) {
            return Some(Target::PlTransport(((x - mx) / 10) as usize));
        }
        let b = body(sp::r(0, 0, w, h));
        if sp::r(w - 15, b.y, 8, b.h).contains(x, y) {
            return Some(Target::PlScroll);
        }
        if b.contains(x, y) {
            let i = self.view.pl_scroll + ((y - b.y) / font::LINE_H) as usize;
            if i < live.queue.len() {
                return Some(Target::PlRow(i));
            }
        }
        None
    }

    /// Click selects; Ctrl-click toggles a row, Shift-click selects a range;
    /// a second click on the same row within 400 ms plays it.
    pub(super) fn pl_row_click(&mut self, i: usize, mods: Mods) -> Vec<Action> {
        let now = Instant::now();
        let double = matches!(self.view.pl_last_click, Some((j, t)) if j == i && now.duration_since(t).as_millis() < 400);
        self.view.pl_last_click = Some((i, now));
        self.view.press = None;
        if mods.shift {
            let a = self.view.pl_anchor.unwrap_or(i);
            if !mods.ctrl {
                self.view.pl_sel.clear();
            }
            self.view.pl_sel.extend(a.min(i)..=a.max(i));
            return vec![];
        }
        self.view.pl_anchor = Some(i);
        if mods.ctrl {
            if !self.view.pl_sel.remove(&i) {
                self.view.pl_sel.insert(i);
            }
            return vec![];
        }
        self.view.pl_sel.clear();
        self.view.pl_sel.insert(i);
        if double {
            self.view.pl_last_click = None;
            return vec![op("queue.play", json!({"index": i}))];
        }
        vec![]
    }

    pub(super) fn pl_scroll_drag(&mut self, live: &Live, y: i32) -> Vec<Action> {
        let Some(r) = self.layout().pl else {
            return vec![];
        };
        let b = body(r);
        let visible = (b.h / font::LINE_H) as usize;
        let max = live.queue.len().saturating_sub(visible);
        let t = ((y - b.y - 9) as f64 / (b.h - 18).max(1) as f64).clamp(0.0, 1.0);
        self.view.pl_scroll = (t * max as f64).round() as usize;
        vec![]
    }

    /// Removes rows from the bottom up, so earlier indexes stay valid.
    fn remove_rows(&mut self, rows: impl DoubleEndedIterator<Item = usize>) -> Vec<Action> {
        let out = rows
            .rev()
            .map(|i| op("queue.remove", json!({"index": i})))
            .collect();
        self.view.pl_sel.clear();
        out
    }

    pub(super) fn pl_remove_selected(&mut self, _live: &Live) -> Vec<Action> {
        let sel: Vec<usize> = self.view.pl_sel.iter().copied().collect();
        self.remove_rows(sel.into_iter())
    }

    /// Keys while the playlist has focus; `None` passes the key on.
    pub(super) fn pl_key(
        &mut self,
        live: &Live,
        key: Key,
        ch: Option<char>,
        mods: Mods,
    ) -> Option<Vec<Action>> {
        let n = live.queue.len();
        if n == 0 {
            return None;
        }
        if mods.ctrl && ch == Some('a') {
            self.view.pl_sel = (0..n).collect();
            return Some(vec![]);
        }
        let page = self
            .layout()
            .pl
            .map_or(1, |r| self.pl_visible_rows(r).max(1) as usize);
        let cur = self.view.pl_anchor.or(self.view.pl_sel.first().copied());
        let to = match key {
            Key::Up => cur.map_or(0, |c| c.saturating_sub(1)),
            Key::Down => cur.map_or(0, |c| (c + 1).min(n - 1)),
            Key::PageUp => cur.map_or(0, |c| c.saturating_sub(page)),
            Key::PageDown => cur.map_or(0, |c| (c + page).min(n - 1)),
            Key::Home => 0,
            Key::End => n - 1,
            _ => return None,
        };
        if mods.shift {
            let a = self.view.pl_sel.first().copied().unwrap_or(to);
            self.view.pl_sel = (a.min(to)..=a.max(to)).collect();
        } else {
            self.view.pl_sel.clear();
            self.view.pl_sel.insert(to);
        }
        self.view.pl_anchor = Some(to);
        self.pl_scroll_to(to, page);
        Some(vec![])
    }

    /// Scrolls just enough to show row `i`.
    fn pl_scroll_to(&mut self, i: usize, page: usize) {
        if i < self.view.pl_scroll {
            self.view.pl_scroll = i;
        } else if i >= self.view.pl_scroll + page {
            self.view.pl_scroll = i + 1 - page;
        }
    }

    /// cliamp `queue.move`s that sort the live playlist by title, as a
    /// selection sort: each move puts the next title in place.
    fn sort_by_title(live: &Live) -> Vec<Action> {
        let key = |t: &crate::cliamp::Track| t.display().to_lowercase();
        let mut order: Vec<usize> = (0..live.queue.len()).collect();
        let mut out = vec![];
        for k in 0..order.len() {
            let best = (k..order.len())
                .min_by_key(|&j| key(&live.queue[order[j]]))
                .unwrap_or(k);
            if best != k {
                out.push(op("queue.move", json!({"index": best, "to": k})));
                let moved = order.remove(best);
                order.insert(k, moved);
            }
        }
        out
    }

    pub(super) fn pl_menu_action(&mut self, live: &Live, kind: Menu, item: usize) -> Vec<Action> {
        let n = live.queue.len();
        match (kind, item) {
            (Menu::Add, 0) => {
                self.open_prompt(PromptKind::AddLocation);
                vec![]
            }
            (Menu::Add, 1) => vec![Action::PickFiles(Pick::AddFolder)],
            (Menu::Add, _) => vec![Action::PickFiles(Pick::AddFiles)],
            (Menu::Rem, 0) => {
                self.note("Remove dead entries: cliamp skips them itself");
                vec![]
            }
            (Menu::Misc, 0) => {
                self.note("Sorting the playlist by title");
                Self::sort_by_title(live)
            }
            (Menu::Misc, 1) => {
                let t = self
                    .view
                    .pl_sel
                    .first()
                    .and_then(|&i| live.queue.get(i))
                    .or(live.snap.now());
                let text = t.map_or("No track selected".into(), |t| {
                    if t.path.is_empty() {
                        t.display()
                    } else {
                        format!("{}  *  {}", t.display(), t.path)
                    }
                });
                self.note(text);
                vec![]
            }
            (Menu::Misc, _) => {
                self.note("Playlist options: use the bar panel's Library tab");
                vec![]
            }
            (Menu::Rem, 1) => {
                self.view.pl_sel.clear();
                vec![op("queue.clear", json!({}))]
            }
            (Menu::Rem, 2) => {
                let keep = self.view.pl_sel.clone();
                self.remove_rows(
                    (0..n)
                        .filter(|i| !keep.contains(i))
                        .collect::<Vec<_>>()
                        .into_iter(),
                )
            }
            (Menu::Rem, 3) => self.pl_remove_selected(live),
            (Menu::Sel, 0) => {
                let sel = self.view.pl_sel.clone();
                self.view.pl_sel = (0..n).filter(|i| !sel.contains(i)).collect();
                vec![]
            }
            (Menu::Sel, 1) => {
                self.view.pl_sel.clear();
                vec![]
            }
            (Menu::Sel, 2) => {
                self.view.pl_sel = (0..n).collect();
                vec![]
            }
            (Menu::List, 0) => {
                self.view.pl_sel.clear();
                vec![op("queue.clear", json!({}))]
            }
            (Menu::List, 1) => {
                self.open_prompt(PromptKind::SavePlaylist);
                vec![]
            }
            (Menu::List, 2) => {
                vec![Action::Query(
                    "playlists".into(),
                    "provider.playlists".into(),
                    json!({"provider": "local"}),
                )]
            }
            _ => vec![],
        }
    }

    pub fn open_eq_presets(&mut self, live: &Live) {
        let items: Vec<String> = EQ_PRESETS.iter().map(|s| s.to_string()).collect();
        let sel = items
            .iter()
            .position(|p| *p == live.snap.eq_preset)
            .unwrap_or(0);
        self.open_chooser(
            ChooserKind::EqPreset,
            "Equalizer preset (Enter to apply):",
            items,
            sel,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cliamp::Track;
    use crate::ui::View;

    fn setup() -> (Ui, Live) {
        let skin = crate::skin::test_skin("base-2.91").unwrap_or_default();
        let ui = Ui::new(skin, View::default());
        let live = Live {
            queue: (0..5)
                .map(|i| Track {
                    title: format!("t{i}"),
                    ..Default::default()
                })
                .collect(),
            ..Default::default()
        };
        (ui, live)
    }

    #[test]
    fn crop_removes_unselected_bottom_up() {
        let (mut ui, live) = setup();
        ui.view.pl_sel = [1, 3].into_iter().collect();
        let a = ui.pl_menu_action(&live, Menu::Rem, 2);
        let idx: Vec<i64> = a
            .iter()
            .map(|a| match a {
                Action::Op(_, v) => v["index"].as_i64().unwrap(),
                _ => -1,
            })
            .collect();
        assert_eq!(idx, vec![4, 2, 0]);
    }

    #[test]
    fn double_click_plays() {
        let (mut ui, _) = setup();
        assert!(ui.pl_row_click(2, Mods::default()).is_empty());
        assert_eq!(
            ui.pl_row_click(2, Mods::default()),
            vec![op("queue.play", json!({"index": 2}))]
        );
    }

    #[test]
    fn ctrl_and_shift_select() {
        let (mut ui, _) = setup();
        let ctrl = Mods {
            ctrl: true,
            ..Mods::default()
        };
        let shift = Mods {
            shift: true,
            ..Mods::default()
        };
        ui.pl_row_click(1, Mods::default());
        ui.pl_row_click(3, ctrl);
        assert_eq!(ui.view.pl_sel.iter().copied().collect::<Vec<_>>(), [1, 3]);
        ui.pl_row_click(3, ctrl);
        assert_eq!(ui.view.pl_sel.iter().copied().collect::<Vec<_>>(), [1]);
        ui.pl_row_click(4, shift);
        assert_eq!(ui.view.pl_sel.iter().copied().collect::<Vec<_>>(), [3, 4]);
    }

    #[test]
    fn keyboard_moves_selection() {
        let (mut ui, live) = setup();
        let m = Mods::default();
        ui.pl_key(&live, Key::Down, None, m);
        ui.pl_key(&live, Key::Down, None, m);
        assert_eq!(ui.view.pl_sel.first(), Some(&1));
        ui.pl_key(&live, Key::End, None, m);
        assert_eq!(ui.view.pl_sel.first(), Some(&4));
        assert!(ui.pl_key(&live, Key::Left, None, m).is_none());
    }

    #[test]
    fn sort_moves_into_title_order() {
        let (_, mut live) = setup();
        for (t, name) in live.queue.iter_mut().zip(["d", "b", "e", "a", "c"]) {
            t.title = name.into();
        }
        let mut titles: Vec<String> = live.queue.iter().map(|t| t.title.clone()).collect();
        for a in Ui::sort_by_title(&live) {
            let Action::Op(_, v) = a else { panic!() };
            let (i, to) = (
                v["index"].as_u64().unwrap() as usize,
                v["to"].as_u64().unwrap() as usize,
            );
            let t = titles.remove(i);
            titles.insert(to, t);
        }
        assert_eq!(titles, ["a", "b", "c", "d", "e"]);
    }

    #[test]
    fn rows_hit_under_scroll() {
        let (ui, live) = setup();
        let r = ui.layout().pl.unwrap();
        let b = body(sp::r(0, 0, r.w, r.h));
        assert_eq!(
            ui.pl_target(&live, b.x + 5, b.y + font::LINE_H + 2, r),
            Some(Target::PlRow(1))
        );
    }
}

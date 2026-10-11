//! The main menu: Winamp's options button, the clutterbar O and a right
//! click all open it. It is drawn in the skin's playlist colors and font, as
//! a popup that may hang outside the window.

use super::font::{LINE_H, PlFont};
use super::{Action, Pick, PromptKind, Target, Ui};
use crate::cliamp::engine::Live;
use crate::render::Canvas;
use crate::skin::sprites::{self as sp, R};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cmd {
    PlayFiles,
    PlayLocation,
    AddFolder,
    Equalizer,
    Playlist,
    SkinBrowser,
    BaseSkin,
    DoubleSize,
    Shade,
    Pin,
    Visualizer,
    TimeRemaining,
    Minimize,
    Exit,
}

pub struct Item {
    pub label: &'static str,
    pub keys: &'static str,
    /// `Some` for a toggle, drawn with a check mark when on.
    pub check: Option<bool>,
    pub cmd: Cmd,
    /// Draw a separator line above this item.
    pub group: bool,
}

const PAD: i32 = 4;
const SEP_H: i32 = 5;
const CHECK_W: i32 = 12;
/// A check mark, 8×6 pixels.
const CHECK: [(i32, i32); 8] = [
    (0, 3),
    (1, 4),
    (2, 5),
    (3, 4),
    (4, 3),
    (5, 2),
    (6, 1),
    (7, 0),
];

/// The open menu.
pub struct OptMenu {
    pub hover: Option<usize>,
}

impl Ui {
    pub fn menu_items(&self) -> Vec<Item> {
        let v = &self.view;
        let item = |label, keys, check, cmd, group| Item {
            label,
            keys,
            check,
            cmd,
            group,
        };
        vec![
            item("Play file...", "L", None, Cmd::PlayFiles, false),
            item("Play location...", "Ctrl+L", None, Cmd::PlayLocation, false),
            item("Add folder...", "Shift+L", None, Cmd::AddFolder, false),
            item("Equalizer", "Alt+G", Some(v.eq_open), Cmd::Equalizer, true),
            item(
                "Playlist editor",
                "Alt+E",
                Some(v.pl_open),
                Cmd::Playlist,
                false,
            ),
            item("Skin browser...", "", None, Cmd::SkinBrowser, true),
            item("Default skin", "", None, Cmd::BaseSkin, false),
            item(
                "Double size",
                "Ctrl+D",
                Some(v.double),
                Cmd::DoubleSize,
                true,
            ),
            item("Window shade", "Ctrl+W", Some(v.shaded), Cmd::Shade, false),
            item("All workspaces", "Ctrl+A", Some(v.pinned), Cmd::Pin, false),
            item("Visualizer", "", Some(v.vis_on), Cmd::Visualizer, false),
            item(
                "Time remaining",
                "",
                Some(v.time_remaining),
                Cmd::TimeRemaining,
                false,
            ),
            item("Minimize", "", None, Cmd::Minimize, true),
            item("Exit", "", None, Cmd::Exit, false),
        ]
    }

    fn menu_font(&mut self) -> &mut PlFont {
        let family = self.skin.pl_colors().font;
        let f = self.menu_font.get_or_insert_with(|| PlFont::new(&family));
        f.set_family(&family);
        f
    }

    /// Row rectangles, in menu-local skin pixels, and the menu's size.
    fn menu_rows(&mut self, items: &[Item]) -> (Vec<R>, i32, i32) {
        let font = self.menu_font();
        let widest = items
            .iter()
            .map(|i| font.width(i.label) + font.width(i.keys) + 16)
            .max()
            .unwrap_or(0);
        let w = PAD + CHECK_W + widest + PAD;
        let mut y = PAD;
        let rows = items
            .iter()
            .map(|i| {
                if i.group && y > PAD {
                    y += SEP_H;
                }
                let r = sp::r(1, y, w - 2, LINE_H);
                y += LINE_H;
                r
            })
            .collect();
        (rows, w, y + PAD)
    }

    /// The menu's size in skin pixels.
    pub fn menu_size(&mut self) -> (i32, i32) {
        let items = self.menu_items();
        let (_, w, h) = self.menu_rows(&items);
        (w, h)
    }

    /// The item under menu-local point (x, y).
    pub fn menu_item_at(&mut self, x: i32, y: i32) -> Option<usize> {
        let items = self.menu_items();
        let (rows, _, _) = self.menu_rows(&items);
        rows.iter().position(|r| r.contains(x, y))
    }

    /// Draws the menu into `out` (a little-endian ARGB image `zoom` times
    /// the menu's skin size).
    pub fn draw_menu(&mut self, hover: Option<usize>, out: &mut [u8], zoom: i32) {
        let items = self.menu_items();
        let (rows, w, h) = self.menu_rows(&items);
        let colors = self.skin.pl_colors();
        let mut c = Canvas::new(w, h);
        c.fill(sp::r(0, 0, w, h), colors.normal_bg);
        // A one-pixel frame in the text color, and group separators.
        for x in 0..w {
            c.set(x, 0, colors.normal);
            c.set(x, h - 1, colors.normal);
        }
        for y in 0..h {
            c.set(0, y, colors.normal);
            c.set(w - 1, y, colors.normal);
        }
        let font = self
            .menu_font
            .get_or_insert_with(|| PlFont::new(&colors.font));
        font.begin();
        for (i, (item, r)) in items.iter().zip(&rows).enumerate() {
            if item.group && i > 0 {
                let y = r.y - SEP_H / 2 - 1;
                for x in PAD..w - PAD {
                    c.set(x, y, colors.normal);
                }
            }
            let fg = if hover == Some(i) {
                c.fill(*r, colors.selected_bg);
                colors.current
            } else {
                colors.normal
            };
            if item.check == Some(true) {
                // Drawn in pixels: skin fonts often lack a check mark glyph.
                for (dx, dy) in CHECK {
                    for t in 0..2 {
                        c.set(r.x + PAD + dx, r.y + 3 + dy + t, colors.current);
                    }
                }
            }
            font.draw(item.label, r.x + PAD + CHECK_W, r.y, fg, *r);
            if !item.keys.is_empty() {
                let kw = font.width(item.keys);
                font.draw(item.keys, r.x + r.w - PAD - kw, r.y, fg, *r);
            }
        }
        c.scale_into(out, zoom);
        font.paint(out, (w * zoom) as usize, zoom);
    }

    /// Runs a menu command.
    pub fn menu_command(&mut self, live: &Live, cmd: Cmd) -> Vec<Action> {
        match cmd {
            Cmd::PlayFiles => vec![Action::PickFiles(Pick::Play)],
            Cmd::PlayLocation => {
                self.open_prompt(PromptKind::PlayLocation);
                vec![Action::Resize]
            }
            Cmd::AddFolder => vec![Action::PickFiles(Pick::AddFolder)],
            Cmd::Equalizer => self.click(live, Target::EqToggle),
            Cmd::Playlist => self.click(live, Target::PlToggle),
            Cmd::SkinBrowser => vec![Action::SkinBrowser],
            Cmd::BaseSkin => vec![Action::BaseSkin],
            Cmd::DoubleSize => self.click(live, Target::Clutter(3)),
            Cmd::Shade => self.click(live, Target::Shade),
            Cmd::Pin => self.click(live, Target::Clutter(1)),
            Cmd::Visualizer => self.click(live, Target::Clutter(4)),
            Cmd::TimeRemaining => self.click(live, Target::Time),
            Cmd::Minimize => vec![Action::Minimize],
            Cmd::Exit => vec![Action::Close],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::skin::Skin;
    use crate::ui::View;

    #[test]
    fn rows_do_not_overlap_and_fit() {
        let mut ui = Ui::new(Skin::default(), View::default());
        let items = ui.menu_items();
        let (rows, _, h) = ui.menu_rows(&items);
        for pair in rows.windows(2) {
            assert!(pair[0].y + pair[0].h <= pair[1].y);
        }
        assert!(rows.last().unwrap().y + LINE_H <= h);
        assert_eq!(ui.menu_item_at(rows[3].x + 2, rows[3].y + 2), Some(3));
    }
}

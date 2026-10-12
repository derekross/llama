//! The main window: title bar, LCD, marquee, visualizer, sliders, transport.

use std::time::Instant;

use serde_json::json;

use super::{Action, Target, Ui, clock, op};
use crate::cliamp::engine::Live;
use crate::cliamp::volume_ratio;
use crate::skin::Sheet;
use crate::skin::sprites::{self as sp, R};

/// Winamp stepped its marquee one glyph every 220 ms... in pixel steps.
const MARQUEE_STEP_MS: u128 = 220;
const MARQUEE_STEP_PX: i32 = 5;

impl Ui {
    pub(super) fn draw_main(&mut self, live: &Live, r: R) {
        if self.view.shaded {
            return self.draw_main_shaded(live, r);
        }
        let s = &live.snap;
        let pressed = self.view.press;
        let is = |t: Target| pressed == Some(t);
        let c = &mut self.canvas;
        let skin = &self.skin;

        c.blit(skin.sheet(Sheet::Main), sp::MAIN_BG, (0, 0));
        let title = if self.view.focused {
            sp::TITLE_ACTIVE
        } else {
            sp::TITLE
        };
        c.blit(skin.sheet(Sheet::Titlebar), title, (0, 0));

        // Title bar buttons: their released look is part of the title bar
        // art, so only a press draws anything.
        let tb = skin.sheet(Sheet::Titlebar);
        for (b, t) in [
            (sp::BTN_OPTIONS, Target::Options),
            (sp::BTN_MINIMIZE, Target::Minimize),
            (sp::BTN_SHADE, Target::Shade),
            (sp::BTN_CLOSE, Target::Close),
        ] {
            if is(t) {
                c.blit(tb, b.down, b.at);
            }
        }

        // Clutterbar: the background shows the released state; a pressed
        // letter swaps in its sprite. D is lit while at double size.
        c.blit(tb, sp::CLUTTER_BG, sp::CLUTTER_AT);
        for (i, (sprite, dy)) in sp::CLUTTER_BUTTONS.iter().enumerate() {
            if is(Target::Clutter(i)) || (i == 3 && self.view.zoom > 2) {
                c.blit(tb, *sprite, (sp::CLUTTER_AT.0, sp::CLUTTER_AT.1 + dy));
            }
        }

        // Play state.
        let pp = skin.sheet(Sheet::Playpaus);
        let status = if s.playing() {
            sp::PLAYING
        } else if s.paused() {
            sp::PAUSED
        } else {
            sp::STOPPED
        };
        c.blit(pp, status, sp::STATUS_AT);
        if s.playing() {
            let work = if live.connected && s.stream_error.is_none() {
                sp::WORKING
            } else {
                sp::NOT_WORKING
            };
            c.blit(pp, work, sp::WORKING_AT);
        }

        // Time: hidden when stopped, blinking when paused.
        let blink_off = s.paused()
            && std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .is_ok_and(|d| d.as_millis() / 500 % 2 == 1);
        if (s.playing() || s.paused()) && !blink_off {
            let pos = live.position();
            let (secs, minus) = if self.view.time_remaining && s.duration > 0.0 {
                (s.duration - pos, true)
            } else {
                (pos, false)
            };
            let secs = secs.max(0.0) as u32;
            let (m, sec) = ((secs / 60).min(99), secs % 60);
            if let Some((img, ex)) = skin.digits() {
                for (i, d) in [m / 10, m % 10, sec / 10, sec % 10].into_iter().enumerate() {
                    c.blit(Some(img), sp::digit(d), sp::DIGITS_AT[i]);
                }
                if minus {
                    if ex {
                        c.blit(Some(img), sp::MINUS_EX, sp::MINUS_EX_AT);
                    } else {
                        c.blit(Some(img), sp::MINUS, sp::MINUS_AT);
                    }
                }
            }
        }

        // Mono/stereo: cliamp reports mono output; anything playing in
        // stereo lights the stereo lamp.
        let ms = skin.sheet(Sheet::Monoster);
        let active = s.playing() || s.paused();
        c.blit(
            ms,
            if active && s.mono {
                sp::MONO_ON
            } else {
                sp::MONO_OFF
            },
            sp::MONO_AT,
        );
        c.blit(
            ms,
            if active && !s.mono {
                sp::STEREO_ON
            } else {
                sp::STEREO_OFF
            },
            sp::STEREO_AT,
        );

        // Volume.
        let vol = match (pressed, self.view.drag) {
            (Some(Target::Volume), Some(d)) => d,
            _ => s.volume.map(volume_ratio).unwrap_or(1.0),
        };
        let vs = skin.sheet(Sheet::Volume);
        let frame = (vol * 27.0).round() as i32;
        c.blit(vs, sp::volume_frame(frame), (sp::VOLUME.x, sp::VOLUME.y));
        if vs.is_some_and(|v| v.h >= 433) {
            let thumb = if is(Target::Volume) {
                sp::SLIDER_THUMB_DOWN
            } else {
                sp::SLIDER_THUMB
            };
            c.blit(
                vs,
                thumb,
                (
                    sp::VOLUME.x + (vol * sp::VOLUME_TRAVEL as f64).round() as i32,
                    sp::VOLUME.y + 1,
                ),
            );
        }

        // Balance: cliamp has none, so it rests at center.
        let bs = skin.sheet(Sheet::Balance);
        c.blit(bs, sp::balance_frame(0), (sp::BALANCE.x, sp::BALANCE.y));
        if bs.is_some_and(|b| b.h >= 433) {
            let thumb = if is(Target::Balance) {
                sp::SLIDER_THUMB_DOWN
            } else {
                sp::SLIDER_THUMB
            };
            c.blit(
                bs,
                thumb,
                (sp::BALANCE.x + sp::BALANCE_TRAVEL / 2, sp::BALANCE.y + 1),
            );
        }

        // EQ / PL toggles.
        let sr = skin.sheet(Sheet::Shufrep);
        c.blit(
            sr,
            sp::EQ_TOGGLE.sprite(self.view.eq_open, is(Target::EqToggle)),
            sp::EQ_TOGGLE.at,
        );
        c.blit(
            sr,
            sp::PL_TOGGLE.sprite(self.view.pl_open, is(Target::PlToggle)),
            sp::PL_TOGGLE.at,
        );

        // Position bar: the thumb only shows when the track is seekable.
        let pb = skin.sheet(Sheet::Posbar);
        c.blit(pb, sp::POS_BG, (sp::POSBAR.x, sp::POSBAR.y));
        if s.duration > 0.0 && (s.playing() || s.paused()) {
            let ratio = match (pressed, self.view.drag) {
                (Some(Target::Seek), Some(d)) => d,
                _ => live.position() / s.duration,
            };
            let thumb = if is(Target::Seek) {
                sp::POS_THUMB_DOWN
            } else {
                sp::POS_THUMB
            };
            c.blit(
                pb,
                thumb,
                (
                    sp::POSBAR.x + (ratio.clamp(0.0, 1.0) * sp::POS_TRAVEL as f64) as i32,
                    sp::POSBAR.y,
                ),
            );
        }

        // Transport.
        let cb = skin.sheet(Sheet::Cbuttons);
        for (i, b) in sp::TRANSPORT.iter().enumerate() {
            c.blit(
                cb,
                if is(Target::Transport(i)) {
                    b.down
                } else {
                    b.up
                },
                b.at,
            );
        }
        c.blit(
            sr,
            sp::SHUFFLE.sprite(s.shuffle, is(Target::Shuffle)),
            sp::SHUFFLE.at,
        );
        c.blit(
            sr,
            sp::REPEAT.sprite(s.repeat_on(), is(Target::Repeat)),
            sp::REPEAT.at,
        );

        // Marquee: a slider drag takes it over, like Winamp.
        let text = self.marquee(live);
        let font = self.skin.sheet(Sheet::Text);
        let width = text.chars().count() as i32 * sp::CHAR_W;
        let offset = if width > sp::MARQUEE.w && pressed.is_none() {
            let steps = (self.marquee_start.elapsed().as_millis() / MARQUEE_STEP_MS) as i32;
            (steps * MARQUEE_STEP_PX) % width
        } else {
            0
        };
        let c = &mut self.canvas;
        if offset > 0 {
            // Loop seamlessly: draw the text twice, end to end.
            let doubled = format!("{text}{text}");
            c.text(font, &doubled, sp::MARQUEE, offset);
        } else {
            c.text(font, &text, sp::MARQUEE, 0);
        }

        if self.view.vis_on {
            self.draw_vis(live, sp::VIS, 16);
        }
    }

    fn draw_main_shaded(&mut self, live: &Live, _r: R) {
        let s = &live.snap;
        let c = &mut self.canvas;
        let tb = self.skin.sheet(Sheet::Titlebar);
        let bg = if self.view.focused {
            sp::SHADE_BG_ACTIVE
        } else {
            sp::SHADE_BG
        };
        c.blit(tb, bg, (0, 0));
        let is = |t| self.view.press == Some(t);
        if is(Target::Options) {
            c.blit(tb, sp::BTN_OPTIONS.down, sp::BTN_OPTIONS.at);
        }
        if is(Target::Minimize) {
            c.blit(tb, sp::BTN_MINIMIZE.down, sp::BTN_MINIMIZE.at);
        }
        c.blit(
            tb,
            if is(Target::Shade) {
                sp::BTN_UNSHADE.down
            } else {
                sp::BTN_UNSHADE.up
            },
            sp::BTN_UNSHADE.at,
        );
        if is(Target::Close) {
            c.blit(tb, sp::BTN_CLOSE.down, sp::BTN_CLOSE.at);
        }
        if s.duration > 0.0 && (s.playing() || s.paused()) {
            c.blit(tb, sp::SHADE_POS_BG, sp::SHADE_POS_AT);
            let ratio = match (self.view.press, self.view.drag) {
                (Some(Target::ShadeSeek), Some(d)) => d,
                _ => live.position() / s.duration,
            };
            let x = (ratio.clamp(0.0, 1.0) * (sp::SHADE_POS_BG.w - sp::SHADE_POS_THUMB.w) as f64)
                as i32;
            c.blit(
                tb,
                sp::SHADE_POS_THUMB,
                (sp::SHADE_POS_AT.0 + x, sp::SHADE_POS_AT.1),
            );
        }
        if s.playing() || s.paused() {
            let pos = live.position();
            let t = if self.view.time_remaining && s.duration > 0.0 {
                format!("-{:>4}", clock(s.duration - pos))
            } else {
                format!(" {:>4}", clock(pos))
            };
            let font = self.skin.sheet(Sheet::Text);
            self.canvas.text(
                font,
                &t,
                sp::r(sp::SHADE_TIME_AT.0, sp::SHADE_TIME_AT.1, 25, 6),
                0,
            );
        }
        if self.view.vis_on {
            self.draw_vis(live, sp::VIS_SHADE, 5);
        }
    }

    /// The text the marquee shows now.
    fn marquee(&mut self, live: &Live) -> String {
        let s = &live.snap;
        let drag = self.view.drag.unwrap_or(0.0);
        let text = match self.view.press {
            Some(Target::Volume) => format!("Volume: {}%", (drag * 100.0).round()),
            Some(Target::Balance) => "Balance: Center".into(),
            Some(Target::Seek) | Some(Target::ShadeSeek) if s.duration > 0.0 => format!(
                "Seek to: {}/{} ({}%)",
                clock(drag * s.duration),
                clock(s.duration),
                (drag * 100.0).round()
            ),
            Some(Target::EqBand(10)) => "Preamp: 0.0 dB (cliamp has no preamp)".into(),
            Some(Target::EqBand(b)) => {
                const HZ: [&str; 10] = [
                    "70Hz", "180Hz", "320Hz", "600Hz", "1KHz", "3KHz", "6KHz", "12KHz", "14KHz",
                    "16KHz",
                ];
                let db = 12.0 - drag * 24.0;
                format!("EQ: {}: {:+.1} dB", HZ[b], db)
            }
            _ if self.view.note.is_some() => {
                self.view.note.as_ref().map(|n| n.0.clone()).unwrap_or_default()
            }
            _ if self.view.status.is_some() => self
                .view
                .status
                .as_ref()
                .map(|s| format!("{}  ***  ", s.0))
                .unwrap_or_default(),
            _ if !live.connected => "Llama: waiting for cliamp...".into(),
            _ if live.busy() => {
                "cliamp is busy loading from a provider; your clicks will go through when it is done  ***  ".into()
            }
            _ => match s.now() {
                Some(t) => {
                    let mut text = match s.current() {
                        Some(i) => format!("{}. {}", i + 1, t.display()),
                        None => t.display(),
                    };
                    if s.duration > 0.0 {
                        text.push_str(&format!(" ({})", clock(s.duration)));
                    }
                    if let Some(e) = s.stream_error.as_ref() {
                        text.push_str(&format!("  ***  {e}"));
                    }
                    if text.chars().count() as i32 * sp::CHAR_W > sp::MARQUEE.w {
                        text.push_str("  ***  ");
                    }
                    text
                }
                None => concat!("Llama ", env!("CARGO_PKG_VERSION"), " - it really whips the llama's ass").into(),
            },
        };
        if text != self.marquee_text {
            self.marquee_text = text.clone();
            self.marquee_start = Instant::now();
        }
        text
    }

    /// The spectrum: 19 bars of 3 px with 1 px gaps, colored row by row from
    /// viscolor.txt, with falling peak dots.
    fn draw_vis(&mut self, live: &Live, area: R, height: i32) {
        let colors = self.skin.viscolors();
        let c = &mut self.canvas;
        // The background is the skin's own art in main.bmp; Winamp painted
        // color 0 with dots of color 1 on every other pixel.
        if height == 16 {
            for y in 0..area.h {
                for x in 0..area.w {
                    let dot = x % 2 == 1 && y % 2 == 1;
                    c.set(
                        area.x + x,
                        area.y + y,
                        if dot { colors[1] } else { colors[0] },
                    );
                }
            }
        } else {
            c.fill(area, colors[0]);
        }
        let bands = &live.spectrum;
        let n = 19;
        for i in 0..n {
            // Spread cliamp's 10 bands over 19 bars.
            let v = if bands.is_empty() {
                0.0
            } else {
                let pos = i as f32 * (bands.len() - 1) as f32 / (n - 1) as f32;
                let (lo, t) = (pos.floor() as usize, pos.fract());
                let hi = (lo + 1).min(bands.len() - 1);
                bands[lo] * (1.0 - t) + bands[hi] * t
            };
            let v = v.clamp(0.0, 1.0);
            // Winamp's bars fall smoothly instead of snapping down.
            self.bars[i] = if v > self.bars[i] {
                v
            } else {
                (self.bars[i] - 0.06).max(v)
            };
            self.peaks[i] = if self.bars[i] >= self.peaks[i] {
                self.bars[i]
            } else {
                (self.peaks[i] - 0.02).max(0.0)
            };
            let bx = area.x + i as i32 * 4;
            let bar_h = (self.bars[i] * height as f32).round() as i32;
            for dy in 0..bar_h {
                let y = area.y + height - 1 - dy;
                // Rows take colors 2 (top) .. 17 (bottom) relative to a full bar.
                let row = height - 1 - dy;
                let color = colors[(2 + row * 16 / height).clamp(2, 17) as usize];
                for dx in 0..3 {
                    c.set(bx + dx, y, color);
                }
            }
            let peak_y = (self.peaks[i] * height as f32).round() as i32;
            if peak_y > 0 && height == 16 {
                for dx in 0..3 {
                    c.set(bx + dx, area.y + height - peak_y, colors[23]);
                }
            }
        }
    }

    pub(super) fn main_target(&self, x: i32, y: i32) -> Option<Target> {
        if self.view.shaded {
            return self.shaded_target(x, y);
        }
        let hit = |r: R| r.contains(x, y);
        let buttons = [
            (sp::BTN_OPTIONS.hit(), Target::Options),
            (sp::BTN_MINIMIZE.hit(), Target::Minimize),
            (sp::BTN_SHADE.hit(), Target::Shade),
            (sp::BTN_CLOSE.hit(), Target::Close),
        ];
        for (r, t) in buttons {
            if hit(r) {
                return Some(t);
            }
        }
        if y < 14 {
            return Some(Target::MainTitle);
        }
        for (i, (sprite, dy)) in sp::CLUTTER_BUTTONS.iter().enumerate() {
            if hit(sp::r(sp::CLUTTER_AT.0, sp::CLUTTER_AT.1 + dy, 8, sprite.h)) {
                return Some(Target::Clutter(i));
            }
        }
        for (i, b) in sp::TRANSPORT.iter().enumerate() {
            if hit(b.hit()) {
                return Some(Target::Transport(i));
            }
        }
        let rest = [
            (sp::SHUFFLE.hit(), Target::Shuffle),
            (sp::REPEAT.hit(), Target::Repeat),
            (sp::EQ_TOGGLE.hit(), Target::EqToggle),
            (sp::PL_TOGGLE.hit(), Target::PlToggle),
            (sp::TIME_HIT, Target::Time),
            (sp::VIS, Target::Vis),
            (sp::VOLUME, Target::Volume),
            (sp::BALANCE, Target::Balance),
            (sp::POSBAR, Target::Seek),
        ];
        rest.into_iter().find(|(r, _)| hit(*r)).map(|(_, t)| t)
    }

    fn shaded_target(&self, x: i32, y: i32) -> Option<Target> {
        let hit = |r: R| r.contains(x, y);
        for (r, t) in [
            (sp::BTN_OPTIONS.hit(), Target::Options),
            (sp::BTN_MINIMIZE.hit(), Target::Minimize),
            (sp::BTN_UNSHADE.hit(), Target::Shade),
            (sp::BTN_CLOSE.hit(), Target::Close),
            (
                sp::r(sp::SHADE_POS_AT.0, sp::SHADE_POS_AT.1, 17, 7),
                Target::ShadeSeek,
            ),
            (sp::VIS_SHADE, Target::Vis),
        ] {
            if hit(r) {
                return Some(t);
            }
        }
        // The shaded bar's tiny transport, drawn into the background art.
        const XS: [(i32, i32); 6] = [
            (169, 177),
            (177, 187),
            (187, 197),
            (197, 206),
            (206, 216),
            (216, 225),
        ];
        if (2..12).contains(&y)
            && let Some(i) = XS.iter().position(|&(a, b)| (a..b).contains(&x))
        {
            return Some(Target::Transport(i));
        }
        Some(Target::MainTitle)
    }

    /// Moves a slider under the pointer; returns live operations for the
    /// ones that act while dragging (volume, EQ).
    pub(super) fn slider_drag(&mut self, live: &Live, t: Target, x: i32, y: i32) -> Vec<Action> {
        let l = self.layout();
        match t {
            Target::Volume => {
                let r = ((x - sp::VOLUME.x - 7) as f64 / sp::VOLUME_TRAVEL as f64).clamp(0.0, 1.0);
                let before = self.view.drag;
                self.view.drag = Some(r);
                let db = crate::cliamp::volume_db(r);
                if before.map(crate::cliamp::volume_db) != Some(db) {
                    return vec![op("volume", json!({"value": db}))];
                }
            }
            Target::Balance => self.view.drag = Some(0.5),
            Target::Seek => {
                let r = ((x - sp::POSBAR.x - 14) as f64 / sp::POS_TRAVEL as f64).clamp(0.0, 1.0);
                self.view.drag = Some(r);
            }
            Target::ShadeSeek => {
                let r = ((x - sp::SHADE_POS_AT.0 - 1) as f64 / 14.0).clamp(0.0, 1.0);
                self.view.drag = Some(r);
            }
            Target::EqBand(b) => {
                let top = l.eq.map_or(0, |r| r.y) + sp::EQ_SLIDER_Y;
                let r = ((y - top - 5) as f64 / sp::EQ_THUMB_TRAVEL as f64).clamp(0.0, 1.0);
                let before = self.view.drag;
                self.view.drag = Some(r);
                if b == 10 {
                    return vec![];
                }
                let db = ((12.0 - r * 24.0) * 2.0).round() / 2.0;
                let old = before.map(|r| ((12.0 - r * 24.0) * 2.0).round() / 2.0);
                // With EQ off, a drag shapes the curve ON will bring back,
                // silently, like Winamp's bypass.
                if let Some(saved) = self.view.eq_saved.as_mut() {
                    saved.resize(10, 0.0);
                    if saved[b] != db {
                        saved[b] = db;
                        return vec![Action::Save];
                    }
                    return vec![];
                }
                let current = live.snap.eq_bands.get(b).copied();
                if old != Some(db) && current != Some(db) {
                    return vec![op("eq", json!({"band": b, "value": db}))];
                }
            }
            _ => {}
        }
        vec![]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::View;

    fn ui() -> Ui {
        let skin = crate::skin::test_skin("base-2.91").unwrap_or_default();
        Ui::new(skin, View::default())
    }

    #[test]
    fn hits() {
        let u = ui();
        assert_eq!(u.main_target(40, 90), Some(Target::Transport(1)));
        assert_eq!(u.main_target(140, 92), Some(Target::Transport(5)));
        assert_eq!(u.main_target(266, 5), Some(Target::Close));
        assert_eq!(u.main_target(100, 5), Some(Target::MainTitle));
        assert_eq!(u.main_target(120, 60), Some(Target::Volume));
        assert_eq!(u.main_target(100, 76), Some(Target::Seek));
        assert_eq!(u.main_target(170, 95), Some(Target::Shuffle));
        assert_eq!(u.main_target(11, 48), Some(Target::Clutter(3)));
    }

    #[test]
    fn status_holds_the_marquee_until_cleared() {
        let mut u = ui();
        let live = Live::default();
        u.view.status = Some(("Loading Mix...".into(), std::time::Instant::now()));
        assert!(u.marquee(&live).starts_with("Loading Mix..."));
        assert!(u.animating(&live));
        u.view.status = None;
        assert!(!u.marquee(&live).starts_with("Loading"));
    }

    #[test]
    fn volume_drag_sends_db_once_per_step() {
        let mut u = ui();
        let live = Live::default();
        let a = u.slider_drag(
            &live,
            Target::Volume,
            sp::VOLUME.x + 7 + sp::VOLUME_TRAVEL,
            60,
        );
        assert_eq!(a, vec![op("volume", json!({"value": 6.0}))]);
        let b = u.slider_drag(
            &live,
            Target::Volume,
            sp::VOLUME.x + 7 + sp::VOLUME_TRAVEL,
            60,
        );
        assert!(b.is_empty());
    }
}

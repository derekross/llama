//! The equalizer window: ten bands on cliamp's ten, a preamp drawn at 0 dB
//! (cliamp has none), ON, AUTO, presets and the response curve.

use serde_json::json;

use super::{Action, Target, Ui, op};
use crate::cliamp::engine::Live;
use crate::skin::Sheet;
use crate::skin::sprites::{self as sp, R};

/// Band gain (dB) as a slider position: 0 at the top (+12), 1 at the bottom.
fn ratio(db: f64) -> f64 {
    ((12.0 - db) / 24.0).clamp(0.0, 1.0)
}

impl Ui {
    fn eq_on(&self) -> bool {
        self.view.eq_saved.is_none()
    }

    pub(super) fn draw_eq(&mut self, live: &Live, r: R) {
        let pressed = self.view.press;
        let is = |t: Target| pressed == Some(t);
        let eq_on = self.eq_on();
        let c = &mut self.canvas;
        let img = self.skin.sheet(Sheet::Eqmain);
        let at = |x: i32, y: i32| (r.x + x, r.y + y);

        c.blit(img, sp::EQ_BG, at(0, 0));
        c.blit(
            img,
            if self.view.focused {
                sp::EQ_TITLE_ACTIVE
            } else {
                sp::EQ_TITLE
            },
            at(0, 0),
        );
        if is(Target::EqClose) {
            c.blit(
                img,
                sp::EQ_CLOSE.down,
                at(sp::EQ_CLOSE.at.0, sp::EQ_CLOSE.at.1),
            );
        }
        c.blit(
            img,
            sp::EQ_ON.sprite(eq_on, is(Target::EqOn)),
            at(sp::EQ_ON.at.0, sp::EQ_ON.at.1),
        );
        c.blit(
            img,
            sp::EQ_AUTO.sprite(false, is(Target::EqAuto)),
            at(sp::EQ_AUTO.at.0, sp::EQ_AUTO.at.1),
        );
        let presets = if is(Target::EqPresets) {
            sp::EQ_PRESETS.down
        } else {
            sp::EQ_PRESETS.up
        };
        c.blit(img, presets, at(sp::EQ_PRESETS.at.0, sp::EQ_PRESETS.at.1));

        let bands: Vec<f64> = (0..10)
            .map(|b| {
                if let (Some(Target::EqBand(d)), Some(v)) = (pressed, self.view.drag)
                    && d == b
                {
                    return 12.0 - v * 24.0;
                }
                // While off, the sliders show the curve that ON brings back.
                match &self.view.eq_saved {
                    Some(saved) => saved.get(b).copied().unwrap_or(0.0),
                    None => live.snap.eq_bands.get(b).copied().unwrap_or(0.0),
                }
            })
            .collect();

        // Sliders: the background strip fills with color as the gain rises.
        let slider = |c: &mut crate::render::Canvas, x: i32, rr: f64, t: Target| {
            let frame = ((1.0 - rr) * 27.0).round() as i32;
            c.blit(img, sp::eq_slider_frame(frame), at(x, sp::EQ_SLIDER_Y));
            let thumb = if is(t) {
                sp::EQ_THUMB_DOWN
            } else {
                sp::EQ_THUMB
            };
            let ty = sp::EQ_SLIDER_Y + (rr * sp::EQ_THUMB_TRAVEL as f64).round() as i32;
            c.blit(img, thumb, at(x + 1, ty));
        };
        let pre = match (pressed, self.view.drag) {
            (Some(Target::EqBand(10)), Some(v)) => v,
            _ => 0.5,
        };
        slider(c, sp::EQ_PREAMP_AT.0, pre, Target::EqBand(10));
        for (b, db) in bands.iter().enumerate() {
            slider(c, sp::eq_band_at(b).0, ratio(*db), Target::EqBand(b));
        }

        // Response curve: the graph background, the preamp line at 0 dB,
        // then the bands joined by a smooth line colored by height.
        let (gx, gy) = at(sp::EQ_GRAPH_AT.0, sp::EQ_GRAPH_AT.1);
        c.blit(img, sp::EQ_GRAPH_BG, (gx, gy));
        c.blit(img, sp::EQ_PREAMP_LINE, (gx, gy + 9));
        let colors: Vec<u32> = (0..19)
            .map(|y| {
                img.and_then(|i| i.get(sp::EQ_GRAPH_COLORS.x, sp::EQ_GRAPH_COLORS.y + y))
                    .unwrap_or(0xFF00FF00)
            })
            .collect();
        let shown = &bands;
        let mut prev: Option<i32> = None;
        for x in 0..113 {
            let pos = x as f64 * 9.0 / 112.0;
            let v = catmull(shown, pos);
            let y = (ratio(v) * 18.0).round() as i32;
            let (a, b) = match prev {
                Some(p) if (p - y).abs() > 1 => (p.min(y), p.max(y)),
                _ => (y, y),
            };
            for yy in a..=b {
                c.set(
                    gx + 1 + x.min(110),
                    gy + yy,
                    colors[yy.clamp(0, 18) as usize],
                );
            }
            prev = Some(y);
        }
    }

    pub(super) fn eq_target(&self, x: i32, y: i32) -> Option<Target> {
        let hit = |r: R| r.contains(x, y);
        if hit(sp::EQ_CLOSE.hit()) {
            return Some(Target::EqClose);
        }
        if y < 14 {
            return Some(Target::EqTitle);
        }
        for (r, t) in [
            (sp::EQ_ON.hit(), Target::EqOn),
            (sp::EQ_AUTO.hit(), Target::EqAuto),
            (sp::EQ_PRESETS.hit(), Target::EqPresets),
        ] {
            if hit(r) {
                return Some(t);
            }
        }
        let slider = |x0: i32| sp::r(x0, sp::EQ_SLIDER_Y, sp::EQ_SLIDER_W, sp::EQ_SLIDER_H);
        if hit(slider(sp::EQ_PREAMP_AT.0)) {
            return Some(Target::EqBand(10));
        }
        (0..10)
            .find(|&b| hit(slider(sp::eq_band_at(b).0)))
            .map(Target::EqBand)
    }

    /// ON off flattens cliamp's EQ and remembers the curve; ON again puts
    /// it back. cliamp itself has no bypass switch.
    pub(super) fn eq_on_toggle(&mut self, live: &Live) -> Vec<Action> {
        let bands: Vec<f64> = match self.view.eq_saved.take() {
            Some(saved) => saved,
            None => {
                let current = live.snap.eq_bands.clone();
                self.view.eq_saved = Some(current);
                vec![0.0; 10]
            }
        };
        let mut out: Vec<Action> = bands
            .iter()
            .enumerate()
            .filter(|(b, v)| live.snap.eq_bands.get(*b) != Some(v))
            .map(|(b, v)| op("eq", json!({"band": b, "value": v})))
            .collect();
        out.push(Action::Save);
        out
    }
}

/// A Catmull-Rom curve through the ten band values, at `pos` in 0..=9.
fn catmull(v: &[f64], pos: f64) -> f64 {
    if v.is_empty() {
        return 0.0;
    }
    let n = v.len() as i64;
    let i = (pos.floor() as i64).clamp(0, n - 1);
    let t = pos - i as f64;
    let p = |k: i64| v[k.clamp(0, n - 1) as usize];
    let (p0, p1, p2, p3) = (p(i - 1), p(i), p(i + 1), p(i + 2));
    0.5 * (2.0 * p1
        + (-p0 + p2) * t
        + (2.0 * p0 - 5.0 * p1 + 4.0 * p2 - p3) * t * t
        + (-p0 + 3.0 * p1 - 3.0 * p2 + p3) * t * t * t)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn curve_passes_through_bands() {
        let v = [0.0, 3.0, -6.0, 12.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0];
        for (i, &b) in v.iter().enumerate() {
            assert!((catmull(&v, i as f64) - b).abs() < 1e-9);
        }
    }

    #[test]
    fn eq_on_off_round_trip() {
        let skin = crate::skin::test_skin("base-2.91").unwrap_or_default();
        let mut ui = Ui::new(skin, super::super::View::default());
        let mut live = Live::default();
        live.snap.eq_bands = vec![0.0, 3.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, -2.0];
        let off = ui.eq_on_toggle(&live);
        // Two bands flatten, then the saved curve is persisted.
        assert_eq!(off.len(), 3);
        assert_eq!(off[2], Action::Save);
        live.snap.eq_bands = vec![0.0; 10];
        let on = ui.eq_on_toggle(&live);
        assert_eq!(
            on,
            vec![
                op("eq", json!({"band": 1, "value": 3.0})),
                op("eq", json!({"band": 9, "value": -2.0})),
                Action::Save
            ]
        );
    }

    #[test]
    fn drags_while_off_shape_the_saved_curve() {
        let skin = crate::skin::test_skin("base-2.91").unwrap_or_default();
        let mut ui = Ui::new(skin, super::super::View::default());
        let live = Live::default();
        ui.view.eq_saved = Some(vec![0.0; 10]);
        let top = ui.layout().eq.unwrap().y + sp::EQ_SLIDER_Y + 5;
        let a = ui.slider_drag(&live, Target::EqBand(2), 0, top);
        assert_eq!(a, vec![Action::Save]);
        assert_eq!(ui.view.eq_saved.as_ref().unwrap()[2], 12.0);
    }
}

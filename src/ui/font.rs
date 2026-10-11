//! The playlist's text: the skin's pledit.txt font (via fontconfig).
//!
//! Layout happens in skin pixels like everything else, but the glyphs are
//! not drawn into the skin canvas. Thresholding an unhinted 11 px outline to
//! hard pixels and then magnifying it 3× looks grainy, so `draw` only records
//! each run, and `paint` rasterizes the runs antialiased at the buffer's own
//! resolution after the skin has been scaled up.

use std::collections::HashMap;
use std::process::Command;

use ab_glyph::{Font, FontVec, PxScale, ScaleFont};

use crate::skin::sprites::R;

pub const LINE_H: i32 = 13;
const PX: f32 = 11.0;

pub struct PlFont {
    font: Option<FontVec>,
    family: String,
    /// (char, physical pixels per skin pixel) → rasterized glyph.
    cache: HashMap<(char, i32), Glyph>,
    runs: Vec<Run>,
    /// Areas drawn over the text afterwards (an open menu); text skips them.
    covers: Vec<R>,
}

struct Run {
    text: String,
    x: i32,
    y: i32,
    color: u32,
    clip: R,
}

struct Glyph {
    x0: i32,
    y0: i32,
    w: i32,
    h: i32,
    coverage: Vec<u8>,
}

fn find_font(family: &str) -> Option<FontVec> {
    for pattern in [
        family,
        "Arial",
        "Liberation Sans",
        "DejaVu Sans",
        "sans-serif",
    ] {
        let out = Command::new("fc-match")
            .args(["-f", "%{file}", pattern])
            .output()
            .ok()?;
        let path = String::from_utf8_lossy(&out.stdout).to_string();
        if let Ok(bytes) = std::fs::read(&path)
            && let Ok(f) = FontVec::try_from_vec(bytes)
        {
            return Some(f);
        }
    }
    None
}

impl PlFont {
    pub fn new(family: &str) -> PlFont {
        PlFont {
            font: find_font(family),
            family: family.to_string(),
            cache: HashMap::new(),
            runs: Vec::new(),
            covers: Vec::new(),
        }
    }

    /// Switches to another skin's font when the family differs.
    pub fn set_family(&mut self, family: &str) {
        if family != self.family {
            *self = PlFont::new(family);
        }
    }

    /// Forgets the previous frame's text.
    pub fn begin(&mut self) {
        self.runs.clear();
        self.covers.clear();
    }

    /// Advance width of `c` in skin pixels.
    fn advance(&self, c: char) -> f32 {
        let Some(font) = self.font.as_ref() else {
            return 0.0;
        };
        font.as_scaled(PxScale::from(PX))
            .h_advance(font.glyph_id(c))
    }

    pub fn width(&mut self, text: &str) -> i32 {
        text.chars().map(|c| self.advance(c)).sum::<f32>().round() as i32
    }

    /// Queues `text` with its top-left at (x, y), clipped to `clip`, all in
    /// skin pixels.
    pub fn draw(&mut self, text: &str, x: i32, y: i32, color: u32, clip: R) {
        self.runs.push(Run {
            text: text.to_string(),
            x,
            y,
            color,
            clip,
        });
    }

    /// Keeps text from showing through `r`, which is drawn after it.
    pub fn cover(&mut self, r: R) {
        self.covers.push(r);
    }

    fn glyph(&mut self, c: char, zoom: i32) -> Option<&Glyph> {
        let font = self.font.as_ref()?;
        Some(self.cache.entry((c, zoom)).or_insert_with(|| {
            let scale = PxScale::from(PX * zoom as f32);
            let ascent = font.as_scaled(scale).ascent();
            let glyph = font
                .glyph_id(c)
                .with_scale_and_position(scale, ab_glyph::point(0.0, ascent));
            match font.outline_glyph(glyph) {
                Some(o) => {
                    let b = o.px_bounds();
                    let (w, h) = (b.width() as i32, b.height() as i32);
                    let mut coverage = vec![0u8; (w * h).max(0) as usize];
                    o.draw(|x, y, cov| {
                        if let Some(p) = coverage.get_mut((y as i32 * w + x as i32) as usize) {
                            *p = (cov.clamp(0.0, 1.0) * 255.0).round() as u8;
                        }
                    });
                    Glyph {
                        x0: b.min.x as i32,
                        y0: b.min.y as i32,
                        w,
                        h,
                        coverage,
                    }
                }
                None => Glyph {
                    x0: 0,
                    y0: 0,
                    w: 0,
                    h: 0,
                    coverage: Vec::new(),
                },
            }
        }))
    }

    /// Rasterizes the queued text into `out`, a little-endian ARGB image
    /// `stride` pixels wide holding the skin scaled up `zoom` times.
    pub fn paint(&mut self, out: &mut [u8], stride: usize, zoom: i32) {
        let runs = std::mem::take(&mut self.runs);
        let covers = std::mem::take(&mut self.covers);
        let rows = out.len() / 4 / stride.max(1);
        for run in &runs {
            let clip = (
                run.clip.x * zoom,
                run.clip.y * zoom,
                (run.clip.x + run.clip.w) * zoom,
                (run.clip.y + run.clip.h) * zoom,
            );
            let [cb, cg, cr, _] = run.color.to_le_bytes();
            let mut pen = (run.x * zoom) as f32;
            for ch in run.text.chars() {
                let advance = self.advance(ch) * zoom as f32;
                let gx = pen.round() as i32;
                if gx >= clip.2 {
                    break;
                }
                let top = run.y * zoom;
                let Some(g) = self.glyph(ch, zoom) else { break };
                for yy in 0..g.h {
                    let py = top + g.y0 + yy;
                    if py < clip.1 || py >= clip.3 || py < 0 || py as usize >= rows {
                        continue;
                    }
                    for xx in 0..g.w {
                        let a = g.coverage[(yy * g.w + xx) as usize] as u32;
                        let px = gx + g.x0 + xx;
                        if a == 0 || px < clip.0 || px >= clip.2 || px < 0 || px as usize >= stride
                        {
                            continue;
                        }
                        let (sx, sy) = (px / zoom, py / zoom);
                        if covers.iter().any(|r| r.contains(sx, sy)) {
                            continue;
                        }
                        let o = (py as usize * stride + px as usize) * 4;
                        let mix = |dst: u8, src: u8| {
                            ((src as u32 * a + dst as u32 * (255 - a) + 127) / 255) as u8
                        };
                        out[o] = mix(out[o], cb);
                        out[o + 1] = mix(out[o + 1], cg);
                        out[o + 2] = mix(out[o + 2], cr);
                    }
                }
                pen += advance;
            }
        }
        self.runs = runs;
        self.covers = covers;
    }
}

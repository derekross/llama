//! Drawing at skin resolution, then scaling up by whole pixels.
//!
//! Everything is composed on a canvas where one pixel is one skin pixel,
//! exactly as Winamp drew it. Only the final copy into the Wayland buffer
//! multiplies by the zoom, nearest-neighbour, so skins stay pixel-sharp.

use crate::skin::Image;
use crate::skin::sprites::{self, At, R};

pub struct Canvas {
    pub w: i32,
    pub h: i32,
    pub px: Vec<u32>,
    /// Per-pixel opacity from region.txt; `None` means fully opaque.
    pub mask: Option<Vec<bool>>,
}

impl Canvas {
    pub fn new(w: i32, h: i32) -> Canvas {
        Canvas {
            w,
            h,
            px: vec![0xFF00_0000; (w * h) as usize],
            mask: None,
        }
    }

    pub fn resize(&mut self, w: i32, h: i32) {
        if (w, h) != (self.w, self.h) {
            *self = Canvas::new(w, h);
        }
    }

    /// Copies `src` rectangle of `img` to `at`, clipped to `clip` (in canvas
    /// coordinates). Sheets that are smaller than the spec (it happens)
    /// leave the uncovered part as it was.
    pub fn blit_clip(&mut self, img: &Image, src: R, at: At, clip: R) {
        for dy in 0..src.h {
            let (sy, ty) = (src.y + dy, at.1 + dy);
            if sy < 0
                || sy >= img.h
                || ty < clip.y
                || ty >= clip.y + clip.h
                || ty < 0
                || ty >= self.h
            {
                continue;
            }
            for dx in 0..src.w {
                let (sx, tx) = (src.x + dx, at.0 + dx);
                if sx < 0
                    || sx >= img.w
                    || tx < clip.x
                    || tx >= clip.x + clip.w
                    || tx < 0
                    || tx >= self.w
                {
                    continue;
                }
                self.px[(ty * self.w + tx) as usize] = img.px[(sy * img.w + sx) as usize];
            }
        }
    }

    pub fn blit(&mut self, img: Option<&Image>, src: R, at: At) {
        if let Some(img) = img {
            self.blit_clip(img, src, at, sprites::r(0, 0, self.w, self.h));
        }
    }

    pub fn fill(&mut self, r: R, color: u32) {
        let x0 = r.x.max(0);
        let x1 = (r.x + r.w).min(self.w);
        for y in r.y.max(0)..(r.y + r.h).min(self.h) {
            for x in x0..x1 {
                self.px[(y * self.w + x) as usize] = color;
            }
        }
    }

    pub fn set(&mut self, x: i32, y: i32, color: u32) {
        if x >= 0 && y >= 0 && x < self.w && y < self.h {
            self.px[(y * self.w + x) as usize] = color;
        }
    }

    /// Writes `text` in text.bmp's 5×6 glyphs, starting `offset` pixels into
    /// the string (for the scrolling marquee), clipped to `area`.
    pub fn text(&mut self, font: Option<&Image>, text: &str, area: R, offset: i32) {
        let Some(font) = font else { return };
        let mut x = area.x - offset;
        for c in text.chars() {
            if x >= area.x + area.w {
                break;
            }
            if x + sprites::CHAR_W > area.x {
                self.blit_clip(font, sprites::glyph_rect(c), (x, area.y), area);
            }
            x += sprites::CHAR_W;
        }
    }

    /// Copies the canvas into an ARGB8888 buffer at `zoom` physical pixels
    /// per skin pixel. Masked-out pixels become fully transparent.
    pub fn scale_into(&self, out: &mut [u8], zoom: i32) {
        let ow = (self.w * zoom) as usize;
        for y in 0..self.h {
            let src_row = &self.px[(y * self.w) as usize..((y + 1) * self.w) as usize];
            let mask_row = self
                .mask
                .as_ref()
                .map(|m| &m[(y * self.w) as usize..((y + 1) * self.w) as usize]);
            let first = (y * zoom) as usize * ow * 4;
            {
                let row = &mut out[first..first + ow * 4];
                for (x, &p) in src_row.iter().enumerate() {
                    let p = match mask_row {
                        Some(m) if !m[x] => 0,
                        _ => p,
                    };
                    let bytes = p.to_le_bytes();
                    for z in 0..zoom as usize {
                        let o = (x * zoom as usize + z) * 4;
                        row[o..o + 4].copy_from_slice(&bytes);
                    }
                }
            }
            for z in 1..zoom as usize {
                let (head, tail) = out.split_at_mut(first + z * ow * 4);
                tail[..ow * 4].copy_from_slice(&head[first..first + ow * 4]);
            }
        }
    }

    /// The canvas as an RGBA image, for snapshots and tests.
    pub fn to_png(&self, path: &std::path::Path, zoom: i32) -> anyhow::Result<()> {
        let mut buf = vec![0u8; (self.w * zoom * self.h * zoom * 4) as usize];
        self.scale_into(&mut buf, zoom);
        self.save_scaled(path, buf, zoom)
    }

    /// Saves `buf`, this canvas already scaled `zoom` times, as a PNG.
    pub fn save_scaled(
        &self,
        path: &std::path::Path,
        mut buf: Vec<u8>,
        zoom: i32,
    ) -> anyhow::Result<()> {
        for px in buf.as_chunks_mut::<4>().0 {
            px.swap(0, 2); // BGRA (little-endian ARGB) → RGBA
        }
        image::save_buffer(
            path,
            &buf,
            (self.w * zoom) as u32,
            (self.h * zoom) as u32,
            image::ExtendedColorType::Rgba8,
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blit_clips_to_sheet_and_canvas() {
        let mut img = Image::new(2, 2);
        img.px = vec![1, 2, 3, 4];
        let mut c = Canvas::new(3, 3);
        c.blit(Some(&img), sprites::r(0, 0, 5, 5), (2, 2));
        assert_eq!(c.px[8], 1);
        assert_eq!(c.px[0], 0xFF00_0000);
    }

    #[test]
    fn scale_doubles_pixels_and_masks() {
        let mut c = Canvas::new(2, 1);
        c.px = vec![0xFF112233, 0xFF445566];
        c.mask = Some(vec![true, false]);
        let mut out = vec![0u8; (2 * 2) * 2 * 4];
        c.scale_into(&mut out, 2);
        let px: Vec<u32> = out
            .as_chunks::<4>()
            .0
            .iter()
            .map(|b| u32::from_le_bytes(*b))
            .collect();
        assert_eq!(
            px,
            vec![0xFF112233, 0xFF112233, 0, 0, 0xFF112233, 0xFF112233, 0, 0]
        );
    }
}

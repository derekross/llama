//! region.txt: polygons that cut a skin's windows out of their rectangles.
//!
//! Each section lists `NumPoints` (points per polygon) and `PointList`
//! (x,y pairs, separated by commas or spaces). Winamp handed these to
//! `CreatePolyPolygonRgn`; the window is the union of the polygons, filled
//! even-odd, with right and bottom edges exclusive. Sampling pixel centers
//! reproduces exactly that.

#[derive(Clone, Debug, Default)]
pub struct Regions {
    pub normal: Option<Vec<Vec<(f32, f32)>>>,
    pub shade: Option<Vec<Vec<(f32, f32)>>>,
    pub eq: Option<Vec<Vec<(f32, f32)>>>,
    pub eq_shade: Option<Vec<Vec<(f32, f32)>>>,
}

pub fn parse(text: &str) -> Regions {
    let mut regions = Regions::default();
    let mut section = String::new();
    let mut counts: Option<String> = None;
    let mut points: Option<String> = None;

    let mut flush = |section: &str, counts: &mut Option<String>, points: &mut Option<String>| {
        if let (Some(c), Some(p)) = (counts.take(), points.take()) {
            let polys = polygons(&c, &p);
            if polys.is_empty() {
                return;
            }
            match section {
                "normal" => regions.normal = Some(polys),
                "windowshade" => regions.shade = Some(polys),
                "equalizer" => regions.eq = Some(polys),
                "equalizerws" => regions.eq_shade = Some(polys),
                _ => {}
            }
        }
        *counts = None;
        *points = None;
    };

    for line in text.lines() {
        let line = line.trim();
        if line.starts_with(';') || line.starts_with("//") || line.is_empty() {
            continue;
        }
        if let Some(name) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            flush(&section, &mut counts, &mut points);
            section = name.trim().to_ascii_lowercase();
            continue;
        }
        let Some((k, v)) = line.split_once('=') else {
            continue;
        };
        match k.trim().to_ascii_lowercase().as_str() {
            "numpoints" => counts = Some(v.to_string()),
            "pointlist" => points = Some(v.to_string()),
            _ => {}
        }
    }
    flush(&section, &mut counts, &mut points);
    regions
}

/// Real skins use a few hundred points; anything far beyond that is broken
/// or hostile, and the section is dropped rather than rasterised.
const MAX_POLY_POINTS: usize = 1_000;
const MAX_POINTS: usize = 10_000;

fn polygons(counts: &str, points: &str) -> Vec<Vec<(f32, f32)>> {
    let mut sizes = Vec::new();
    let mut total = 0usize;
    for n in counts
        .split(',')
        .filter_map(|s| s.trim().parse::<usize>().ok())
    {
        total = match total.checked_add(n) {
            Some(t) if n <= MAX_POLY_POINTS && t <= MAX_POINTS => t,
            _ => return Vec::new(),
        };
        sizes.push(n);
    }
    let nums: Vec<f32> = points
        .split([',', ' ', '\t'])
        .filter(|s| !s.is_empty())
        .filter_map(|s| s.trim().parse().ok())
        .take(2 * total)
        .collect();
    // "inf" and "NaN" parse as floats; no real skin has them.
    if nums.iter().any(|v: &f32| !v.is_finite()) {
        return Vec::new();
    }
    let pts: Vec<(f32, f32)> = nums
        .as_chunks::<2>()
        .0
        .iter()
        .map(|c| (c[0], c[1]))
        .collect();
    let mut out = Vec::new();
    let mut i = 0;
    for n in sizes {
        // Cannot overflow: the sizes sum to at most MAX_POINTS.
        if i + n > pts.len() {
            break; // more polygons declared than points given
        }
        if n >= 3 {
            out.push(pts[i..i + n].to_vec());
        }
        i += n;
    }
    out
}

/// A w×h mask, `true` where the window is opaque.
///
/// Scanline even-odd fill: for each row, the x where every edge crosses the
/// pixel-centre line, sorted; a pixel centre is inside when an odd number of
/// crossings lie to its right. That is the classic point-in-polygon test
/// evaluated once per row instead of once per pixel, with the same float
/// expression, so the result is bit-for-bit the same.
pub fn mask(polys: &[Vec<(f32, f32)>], w: i32, h: i32) -> Vec<bool> {
    let mut m = vec![false; (w.max(0) * h.max(0)) as usize];
    let mut xs_cross = Vec::new();
    for poly in polys {
        let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
        for &(x, y) in poly {
            x0 = x0.min(x);
            y0 = y0.min(y);
            x1 = x1.max(x);
            y1 = y1.max(y);
        }
        let xs = (x0.floor().max(0.0) as i32)..(x1.ceil().min(w as f32) as i32);
        let ys = (y0.floor().max(0.0) as i32)..(y1.ceil().min(h as f32) as i32);
        for y in ys {
            let yc = y as f32 + 0.5;
            xs_cross.clear();
            let mut j = poly.len() - 1;
            for i in 0..poly.len() {
                let (xi, yi) = poly[i];
                let (xj, yj) = poly[j];
                if (yi > yc) != (yj > yc) {
                    let c = (xj - xi) * (yc - yi) / (yj - yi) + xi;
                    // `x < NaN` never toggles.
                    if !c.is_nan() {
                        xs_cross.push(c);
                    }
                }
                j = i;
            }
            xs_cross.sort_unstable_by(f32::total_cmp);
            // Crossings at or left of the current pixel centre.
            let mut k = 0;
            for x in xs.clone() {
                let xc = x as f32 + 0.5;
                while k < xs_cross.len() && xs_cross[k] <= xc {
                    k += 1;
                }
                if (xs_cross.len() - k) % 2 == 1 {
                    m[(y * w + x) as usize] = true;
                }
            }
        }
    }
    m
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rectangle_edges_are_exclusive() {
        let r = parse("[Normal]\nNumPoints=4\nPointList=2,1 5,1 5,3 2,3\n");
        let m = mask(r.normal.as_ref().unwrap(), 8, 5);
        let on: Vec<(i32, i32)> = (0..5)
            .flat_map(|y| (0..8).map(move |x| (x, y)))
            .filter(|&(x, y)| m[(y * 8 + x) as usize])
            .collect();
        assert_eq!(on, vec![(2, 1), (3, 1), (4, 1), (2, 2), (3, 2), (4, 2)]);
    }

    #[test]
    fn short_and_overdeclared_polygons() {
        let r = parse("[normal]\nnumpoints=2, 4, 4\npointlist=0 0 1 1, 0,0 4,0 4,4 0,4\n");
        // The 2-point polygon is skipped; the third has no points left.
        assert_eq!(r.normal.unwrap().len(), 1);
    }

    #[test]
    fn real_skin_sections() {
        let Some(skin) = crate::skin::test_skin("Green-Dimension-V2") else {
            return;
        };
        let r = skin.region;
        assert!(r.normal.is_some() && r.shade.is_some() && r.eq.is_some() && r.eq_shade.is_some());
        let m = mask(r.normal.as_ref().unwrap(), 275, 116);
        // The corner at (0,0) is cut away; the middle of the window is not.
        assert!(!m[0]);
        assert!(m[(50 * 275 + 150) as usize]);
    }

    #[test]
    fn overflowing_numpoints_is_dropped() {
        let r = parse("[Normal]\nNumPoints=3,18446744073709551615\nPointList=0,0 4,0 4,4\n");
        assert!(r.normal.is_none());
        let r = parse("[Normal]\nNumPoints=18446744073709551615,3\nPointList=0,0 4,0 4,4\n");
        assert!(r.normal.is_none());
    }

    #[test]
    fn huge_point_counts_are_dropped_quickly() {
        let n = 100_000;
        let pts: Vec<String> = (0..n)
            .map(|i| {
                let a = i as f32 / n as f32 * std::f32::consts::TAU;
                format!("{},{}", 137.0 + 130.0 * a.cos(), 58.0 + 55.0 * a.sin())
            })
            .collect();
        let text = format!("[Normal]\nNumPoints={n}\nPointList={}\n", pts.join(" "));
        let t = std::time::Instant::now();
        let r = parse(&text);
        assert!(r.normal.is_none());
        // Many polygons each under the cap still hit the total cap.
        let counts = ["900"; 20].join(",");
        let text = format!(
            "[Normal]\nNumPoints={counts}\nPointList={}\n",
            pts.join(" ")
        );
        assert!(parse(&text).normal.is_none());
        // The largest accepted region still rasterises fast.
        let counts = ["1000"; 10].join(",");
        let text = format!(
            "[Normal]\nNumPoints={counts}\nPointList={}\n",
            pts.join(" ")
        );
        let r = parse(&text);
        assert_eq!(r.normal.as_ref().unwrap().len(), 10);
        mask(r.normal.as_ref().unwrap(), 275, 116);
        assert!(
            t.elapsed() < std::time::Duration::from_secs(2),
            "{:?}",
            t.elapsed()
        );
    }

    #[test]
    fn non_finite_points_are_dropped() {
        assert!(
            parse("[Normal]\nNumPoints=3\nPointList=0,0 inf,0 4,NaN\n")
                .normal
                .is_none()
        );
    }

    /// The per-pixel point-in-polygon test the scanline fill replaced.
    fn inside(x: f32, y: f32, poly: &[(f32, f32)]) -> bool {
        let mut c = false;
        let mut j = poly.len() - 1;
        for i in 0..poly.len() {
            let (xi, yi) = poly[i];
            let (xj, yj) = poly[j];
            if (yi > y) != (yj > y) && x < (xj - xi) * (y - yi) / (yj - yi) + xi {
                c = !c;
            }
            j = i;
        }
        c
    }

    fn reference_mask(polys: &[Vec<(f32, f32)>], w: i32, h: i32) -> Vec<bool> {
        let mut m = vec![false; (w * h) as usize];
        for poly in polys {
            for y in 0..h {
                for x in 0..w {
                    if inside(x as f32 + 0.5, y as f32 + 0.5, poly) {
                        m[(y * w + x) as usize] = true;
                    }
                }
            }
        }
        m
    }

    #[test]
    fn scanline_matches_point_in_polygon() {
        // xorshift, so the polygons are the same every run.
        let mut s = 0x2545_F491_4F6C_DD1Du64;
        let mut rnd = move |range: f32| {
            s ^= s << 13;
            s ^= s >> 7;
            s ^= s << 17;
            (s >> 40) as f32 / (1u64 << 24) as f32 * range
        };
        for round in 0..200 {
            let (w, h) = (40, 30);
            let polys: Vec<Vec<(f32, f32)>> = (0..1 + round % 4)
                .map(|_| {
                    let n = 3 + (rnd(10.0) as usize);
                    (0..n)
                        .map(|_| {
                            let (x, y) = (rnd(50.0) - 5.0, rnd(40.0) - 5.0);
                            // Half the rounds on whole and half pixels, where
                            // edges pass exactly through pixel centres.
                            if round % 2 == 0 {
                                ((x * 2.0).round() / 2.0, (y * 2.0).round() / 2.0)
                            } else {
                                (x, y)
                            }
                        })
                        .collect()
                })
                .collect();
            assert_eq!(
                mask(&polys, w, h),
                reference_mask(&polys, w, h),
                "{polys:?}"
            );
        }
        if let Some(skin) = crate::skin::test_skin("Green-Dimension-V2") {
            let r = skin.region;
            for (polys, h) in [
                (r.normal, 116),
                (r.shade, 14),
                (r.eq, 116),
                (r.eq_shade, 14),
            ] {
                let polys = polys.unwrap();
                assert_eq!(mask(&polys, 275, h), reference_mask(&polys, 275, h));
            }
        }
    }
}

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

fn polygons(counts: &str, points: &str) -> Vec<Vec<(f32, f32)>> {
    let nums: Vec<f32> = points
        .split([',', ' ', '\t'])
        .filter(|s| !s.is_empty())
        .filter_map(|s| s.trim().parse().ok())
        .collect();
    let pts: Vec<(f32, f32)> = nums
        .as_chunks::<2>()
        .0
        .iter()
        .map(|c| (c[0], c[1]))
        .collect();
    let mut out = Vec::new();
    let mut i = 0;
    for n in counts
        .split(',')
        .filter_map(|s| s.trim().parse::<usize>().ok())
    {
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

/// A w×h mask, `true` where the window is opaque.
pub fn mask(polys: &[Vec<(f32, f32)>], w: i32, h: i32) -> Vec<bool> {
    let mut m = vec![false; (w * h) as usize];
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
            for x in xs.clone() {
                if inside(x as f32 + 0.5, y as f32 + 0.5, poly) {
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
        let text = include_str!("../../tests/region-green-dimension.txt");
        let r = parse(text);
        assert!(r.normal.is_some() && r.shade.is_some() && r.eq.is_some() && r.eq_shade.is_some());
        let m = mask(r.normal.as_ref().unwrap(), 275, 116);
        // The corner at (0,0) is cut away; the middle of the window is not.
        assert!(!m[0]);
        assert!(m[(50 * 275 + 150) as usize]);
    }
}

//! Classic Winamp skins: a `.wsz` (a zip) or a folder of BMP sprite sheets
//! plus a few text files.
//!
//! Skins in the wild are messy: names in any case, files in a subfolder,
//! sheets missing or with corrupt headers. Everything here degrades to "use
//! the base skin's sheet" rather than failing.

pub mod region;
pub mod sprites;

use std::collections::HashMap;
use std::io::{Cursor, Read};
use std::path::Path;

use anyhow::{Context, Result, bail};

const MAX_SHEET_DIM: u32 = 2048;

/// An image in 0xAARRGGBB pixels.
#[derive(Clone, Debug, Default)]
pub struct Image {
    pub w: i32,
    pub h: i32,
    pub px: Vec<u32>,
}

impl Image {
    pub fn new(w: i32, h: i32) -> Self {
        Image {
            w,
            h,
            px: vec![0; (w.max(0) * h.max(0)) as usize],
        }
    }

    pub fn get(&self, x: i32, y: i32) -> Option<u32> {
        (x >= 0 && y >= 0 && x < self.w && y < self.h).then(|| self.px[(y * self.w + x) as usize])
    }

    /// Decodes a BMP or PNG, refusing anything far larger than a sprite
    /// sheet before allocating for it (real sheets are ≤ ~350×300).
    fn decode(bytes: &[u8]) -> Option<Image> {
        let mut limits = image::Limits::default();
        limits.max_image_width = Some(MAX_SHEET_DIM);
        limits.max_image_height = Some(MAX_SHEET_DIM);
        limits.max_alloc = Some(64 << 20);
        let mut reader = image::ImageReader::new(Cursor::new(bytes))
            .with_guessed_format()
            .ok()?;
        reader.limits(limits);
        let img = reader.decode().ok()?.to_rgba8();
        let (w, h) = img.dimensions();
        let px = img
            .pixels()
            .map(|p| 0xFF00_0000 | (p[0] as u32) << 16 | (p[1] as u32) << 8 | p[2] as u32)
            .collect();
        Some(Image {
            w: w as i32,
            h: h as i32,
            px,
        })
    }
}

/// The sprite sheets Llama draws from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Sheet {
    Main,
    Titlebar,
    Cbuttons,
    Numbers,
    NumsEx,
    Text,
    Playpaus,
    Monoster,
    Posbar,
    Shufrep,
    Volume,
    Balance,
    Eqmain,
    EqEx,
    Pledit,
}

impl Sheet {
    pub const ALL: [Sheet; 15] = [
        Sheet::Main,
        Sheet::Titlebar,
        Sheet::Cbuttons,
        Sheet::Numbers,
        Sheet::NumsEx,
        Sheet::Text,
        Sheet::Playpaus,
        Sheet::Monoster,
        Sheet::Posbar,
        Sheet::Shufrep,
        Sheet::Volume,
        Sheet::Balance,
        Sheet::Eqmain,
        Sheet::EqEx,
        Sheet::Pledit,
    ];

    pub fn file(self) -> &'static str {
        match self {
            Sheet::Main => "main.bmp",
            Sheet::Titlebar => "titlebar.bmp",
            Sheet::Cbuttons => "cbuttons.bmp",
            Sheet::Numbers => "numbers.bmp",
            Sheet::NumsEx => "nums_ex.bmp",
            Sheet::Text => "text.bmp",
            Sheet::Playpaus => "playpaus.bmp",
            Sheet::Monoster => "monoster.bmp",
            Sheet::Posbar => "posbar.bmp",
            Sheet::Shufrep => "shufrep.bmp",
            Sheet::Volume => "volume.bmp",
            Sheet::Balance => "balance.bmp",
            Sheet::Eqmain => "eqmain.bmp",
            Sheet::EqEx => "eq_ex.bmp",
            Sheet::Pledit => "pledit.bmp",
        }
    }
}

/// Playlist colors and font from pledit.txt.
#[derive(Clone, Debug)]
pub struct PlColors {
    pub normal: u32,
    pub current: u32,
    pub normal_bg: u32,
    pub selected_bg: u32,
    pub font: String,
}

impl Default for PlColors {
    fn default() -> Self {
        PlColors {
            normal: 0xFF00FF00,
            current: 0xFFFFFFFF,
            normal_bg: 0xFF000000,
            selected_bg: 0xFF0000C6,
            font: "Arial".into(),
        }
    }
}

/// Base-skin visualizer colors: 0 background, 1 dots, 2..=17 spectrum top to
/// bottom, 18..=22 oscilloscope, 23 peak dots.
pub const DEFAULT_VISCOLOR: [u32; 24] = [
    0xFF000000, 0xFF182129, 0xFFEF3110, 0xFFCE2910, 0xFFD65A00, 0xFFD66600, 0xFFD67300, 0xFFC67B08,
    0xFFDEA518, 0xFFD6B521, 0xFFBDDE29, 0xFF94DE21, 0xFF29CE10, 0xFF32BE10, 0xFF39B510, 0xFF319C08,
    0xFF299400, 0xFF188408, 0xFFFFFFFF, 0xFFD6D6DE, 0xFFB5BDBD, 0xFFA0AAAF, 0xFF949CA5, 0xFF969696,
];

#[derive(Clone, Debug, Default)]
pub struct Skin {
    pub name: String,
    pub sheets: HashMap<Sheet, Image>,
    pub viscolor: Option<[u32; 24]>,
    pub pledit: Option<PlColors>,
    pub region: region::Regions,
}

impl Skin {
    /// Loads a `.wsz`/`.zip` file or a skin folder.
    pub fn load(path: &Path) -> Result<Skin> {
        let name = path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        Ok(Skin::from_files(name, read_skin_files(path)?))
    }

    pub fn from_wsz(name: &str, bytes: &[u8]) -> Result<Skin> {
        Ok(Skin::from_files(name.to_string(), read_zip_files(bytes)?))
    }

    /// Builds a skin from (lower-cased basename → bytes), as returned by
    /// [`read_skin_files`].
    pub fn from_files(name: String, files: HashMap<String, Vec<u8>>) -> Skin {
        let mut sheets = HashMap::new();
        for sheet in Sheet::ALL {
            if let Some(img) = files.get(sheet.file()).and_then(|b| Image::decode(b)) {
                sheets.insert(sheet, img);
            } else if files.contains_key(sheet.file()) {
                log::warn!(
                    "{name}: {} is unreadable, using the base skin's",
                    sheet.file()
                );
            }
        }
        let text = |f: &str| files.get(f).map(|b| decode_text(b));
        Skin {
            name,
            sheets,
            viscolor: text("viscolor.txt").and_then(|t| parse_viscolor(&t)),
            pledit: text("pledit.txt").map(|t| parse_pledit(&t)),
            region: text("region.txt")
                .map(|t| region::parse(&t))
                .unwrap_or_default(),
        }
    }

    /// Fills every sheet and setting this skin lacks from `base`.
    pub fn with_fallback(mut self, base: &Skin) -> Skin {
        for (sheet, img) in &base.sheets {
            // numbers.bmp and nums_ex.bmp are alternatives: a skin that has
            // either one keeps its own digits.
            let has_digits = self.sheets.contains_key(&Sheet::Numbers)
                || self.sheets.contains_key(&Sheet::NumsEx);
            if matches!(sheet, Sheet::Numbers | Sheet::NumsEx) && has_digits {
                continue;
            }
            self.sheets.entry(*sheet).or_insert_with(|| img.clone());
        }
        if self.viscolor.is_none() {
            self.viscolor = base.viscolor;
        }
        if self.pledit.is_none() {
            self.pledit = base.pledit.clone();
        }
        self
    }

    /// Whether this is a skin at all: main.bmp is there and decodes.
    pub fn has_main(&self) -> bool {
        self.sheets.contains_key(&Sheet::Main)
    }

    pub fn sheet(&self, sheet: Sheet) -> Option<&Image> {
        self.sheets.get(&sheet)
    }

    /// The digit sheet: nums_ex.bmp when present (its cells differ from
    /// numbers.bmp: a full-size minus lives at cell 11).
    pub fn digits(&self) -> Option<(&Image, bool)> {
        if let Some(img) = self.sheets.get(&Sheet::NumsEx) {
            return Some((img, true));
        }
        self.sheets.get(&Sheet::Numbers).map(|img| (img, false))
    }

    pub fn viscolors(&self) -> [u32; 24] {
        self.viscolor.unwrap_or(DEFAULT_VISCOLOR)
    }

    pub fn pl_colors(&self) -> PlColors {
        self.pledit.clone().unwrap_or_default()
    }
}

fn basename(name: &str) -> String {
    name.rsplit(['/', '\\'])
        .next()
        .unwrap_or(name)
        .to_ascii_lowercase()
}

/// The files Llama reads; everything else in an archive or folder is
/// skipped unread.
fn wanted(name: &str) -> bool {
    Sheet::ALL.iter().any(|s| s.file() == name)
        || matches!(name, "viscolor.txt" | "pledit.txt" | "region.txt")
}

/// Limits for hostile archives and folders. A real skin is well under 1 MB
/// unpacked; the biggest sheets are a few hundred KB.
const MAX_ENTRY: u64 = 8 << 20;
const MAX_TOTAL: u64 = 32 << 20;
const MAX_ENTRIES: usize = 4096;
/// The largest `.wsz` read from disk or the Museum.
pub const MAX_WSZ: u64 = 16 << 20;
const MAX_DEPTH: usize = 2;

/// Reads a `.wsz`/`.zip` file or a skin folder into (lower-cased basename →
/// bytes), keeping only the files Llama uses, within the limits above.
pub fn read_skin_files(path: &Path) -> Result<HashMap<String, Vec<u8>>> {
    if path.is_dir() {
        return read_dir_files(path);
    }
    read_zip_files(&read_capped(path, MAX_WSZ)?)
}

/// Reads a file, failing if it is larger than `max` bytes.
pub fn read_capped(path: &Path, max: u64) -> Result<Vec<u8>> {
    let f = std::fs::File::open(path).with_context(|| format!("reading {}", path.display()))?;
    let mut buf = Vec::new();
    f.take(max + 1)
        .read_to_end(&mut buf)
        .with_context(|| format!("reading {}", path.display()))?;
    if buf.len() as u64 > max {
        bail!("{} is too large to be a skin", path.display());
    }
    Ok(buf)
}

fn read_zip_files(bytes: &[u8]) -> Result<HashMap<String, Vec<u8>>> {
    let mut zip = zip::ZipArchive::new(Cursor::new(bytes)).context("not a zip archive")?;
    let mut files = HashMap::new();
    let mut budget = MAX_TOTAL;
    for i in 0..zip.len().min(MAX_ENTRIES) {
        let Ok(mut entry) = zip.by_index(i) else {
            continue;
        };
        if entry.is_dir() || entry.size() > MAX_ENTRY {
            continue;
        }
        let name = basename(entry.name());
        // The first file of a name wins; skins sometimes ship stray copies
        // in nested folders.
        if !wanted(&name) || files.contains_key(&name) {
            continue;
        }
        // The declared size may lie; `take` bounds what is really inflated,
        // and everything inflated counts, kept or not, so repeated
        // oversized entries can't spin either.
        let cap = MAX_ENTRY.min(budget);
        let mut buf = Vec::with_capacity(entry.size().min(cap) as usize);
        let ok = entry.by_ref().take(cap + 1).read_to_end(&mut buf).is_ok();
        budget = budget.saturating_sub(buf.len() as u64);
        if ok && buf.len() as u64 <= cap {
            files.insert(name, buf);
        }
        if budget == 0 {
            break;
        }
    }
    if files.is_empty() {
        bail!("no skin files in archive");
    }
    Ok(files)
}

/// Walks a skin folder at most `MAX_DEPTH` levels down, never following
/// symlinks, under the same limits as an archive.
fn read_dir_files(dir: &Path) -> Result<HashMap<String, Vec<u8>>> {
    let mut files = HashMap::new();
    let mut budget = MAX_TOTAL;
    let mut seen = 0;
    let mut stack = vec![(dir.to_path_buf(), 0)];
    while let Some((d, depth)) = stack.pop() {
        for entry in std::fs::read_dir(&d)?.flatten() {
            seen += 1;
            if seen > MAX_ENTRIES {
                return Ok(files);
            }
            let Ok(ft) = entry.file_type() else {
                continue;
            };
            let path = entry.path();
            if ft.is_dir() {
                if depth < MAX_DEPTH {
                    stack.push((path, depth + 1));
                }
                continue;
            }
            if !ft.is_file() {
                continue; // symlinks, sockets, fifos
            }
            let name = basename(&path.to_string_lossy());
            if !wanted(&name) || files.contains_key(&name) {
                continue;
            }
            if let Ok(b) = read_capped(&path, MAX_ENTRY.min(budget)) {
                budget -= b.len() as u64;
                files.insert(name, b);
            }
        }
    }
    Ok(files)
}

/// Skin text files are mostly Windows-1252; keep ASCII and map the rest
/// byte-for-byte to Latin-1, which is close enough for colors and names.
fn decode_text(bytes: &[u8]) -> String {
    match std::str::from_utf8(bytes) {
        Ok(s) => s.to_string(),
        Err(_) => bytes.iter().map(|&b| b as char).collect(),
    }
}

fn parse_viscolor(text: &str) -> Option<[u32; 24]> {
    let mut colors = DEFAULT_VISCOLOR;
    let mut n = 0;
    for line in text.lines() {
        if n == 24 {
            break;
        }
        let nums: Vec<u32> = line
            .split(|c: char| !c.is_ascii_digit())
            .filter(|s| !s.is_empty())
            .take(3)
            .filter_map(|s| s.parse().ok())
            .collect();
        // Comments may contain numbers too ("// 3"); only the leading triple
        // counts, and a line must start with a digit.
        if nums.len() == 3 && line.trim_start().starts_with(|c: char| c.is_ascii_digit()) {
            let [r, g, b] = [nums[0], nums[1], nums[2]].map(|v| v.min(255));
            colors[n] = 0xFF00_0000 | r << 16 | g << 8 | b;
            n += 1;
        }
    }
    (n > 0).then_some(colors)
}

fn parse_color(v: &str) -> Option<u32> {
    let hex = v.trim().trim_start_matches('#');
    let hex: String = hex.chars().take_while(|c| c.is_ascii_hexdigit()).collect();
    (hex.len() == 6).then(|| u32::from_str_radix(&hex, 16).ok().map(|c| 0xFF00_0000 | c))?
}

fn parse_pledit(text: &str) -> PlColors {
    let mut pl = PlColors::default();
    for line in text.lines() {
        let Some((k, v)) = line.split_once('=') else {
            continue;
        };
        let k = k.trim().to_ascii_lowercase();
        match k.as_str() {
            "normal" => pl.normal = parse_color(v).unwrap_or(pl.normal),
            "current" => pl.current = parse_color(v).unwrap_or(pl.current),
            "normalbg" => pl.normal_bg = parse_color(v).unwrap_or(pl.normal_bg),
            "selectedbg" => pl.selected_bg = parse_color(v).unwrap_or(pl.selected_bg),
            "font" if !v.trim().is_empty() => pl.font = v.trim().to_string(),
            _ => {}
        }
    }
    pl
}

/// A skin from tests/skins/ (fetched by tests/fetch-skins.sh, not shipped),
/// or `None` with a hint when it is missing, so tests that need one skip.
#[cfg(test)]
pub fn test_skin(name: &str) -> Option<Skin> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/skins")
        .join(format!("{name}.wsz"));
    if !path.exists() {
        eprintln!(
            "skipping: {} is missing; run tests/fetch-skins.sh",
            path.display()
        );
        return None;
    }
    Some(Skin::load(&path).unwrap())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Skips the calling test when the fixture skins are missing.
    macro_rules! skin {
        ($name:expr) => {
            match test_skin($name) {
                Some(s) => s,
                None => return,
            }
        };
    }

    #[test]
    fn base_skin_has_every_sheet() {
        let s = skin!("base-2.91");
        for sheet in Sheet::ALL {
            if sheet == Sheet::NumsEx {
                continue;
            }
            assert!(s.sheet(sheet).is_some(), "missing {sheet:?}");
        }
        let main = s.sheet(Sheet::Main).unwrap();
        assert_eq!((main.w, main.h), (275, 116));
        assert_eq!(s.viscolors()[2], 0xFFEF3110);
        assert_eq!(s.pl_colors().selected_bg, 0xFF0000C6);
    }

    #[test]
    fn nested_folder_and_odd_case() {
        // XMMS-Turquoise keeps everything in a subfolder, "Pledit.bmp" capitalised.
        let s = skin!("XMMS-Turquoise");
        assert!(s.sheet(Sheet::Pledit).is_some());
        assert!(s.sheet(Sheet::Main).is_some());
    }

    #[test]
    fn nums_ex_preferred() {
        let s = skin!("Green-Dimension-V2");
        let (_, ex) = s.digits().unwrap();
        assert!(ex);
    }

    #[test]
    fn fallback_fills_missing_sheets_but_keeps_digits() {
        let base = skin!("base-2.91");
        // MacOSXAqua ships nums_ex but no numbers.bmp, and no titlebar cursor files.
        let s = skin!("MacOSXAqua1-5").with_fallback(&base);
        assert!(s.sheet(Sheet::Numbers).is_none());
        assert!(s.digits().unwrap().1);
        for sheet in Sheet::ALL {
            if matches!(sheet, Sheet::Numbers | Sheet::NumsEx) {
                continue;
            }
            assert!(s.sheet(sheet).is_some(), "missing {sheet:?}");
        }
    }

    #[test]
    fn all_test_skins_load() {
        for name in [
            "base-2.91",
            "Green-Dimension-V2",
            "MacOSXAqua1-5",
            "TopazAmp1-2",
            "Vizor1-01",
            "XMMS-Turquoise",
        ] {
            let s = skin!(name);
            assert!(s.sheet(Sheet::Main).is_some(), "{name}");
        }
    }

    #[test]
    fn viscolor_ignores_comment_numbers() {
        let c = parse_viscolor("1,2,3, // color 0\n 4, 5, 6 // 1\n// 7,8,9\n").unwrap();
        assert_eq!(c[0], 0xFF010203);
        assert_eq!(c[1], 0xFF040506);
        assert_eq!(c[2], DEFAULT_VISCOLOR[2]);
    }

    #[test]
    fn pledit_parses_case_insensitive() {
        let p =
            parse_pledit("[Text]\nnormal=#112233\nCURRENT=#ffffff\nFont=Tahoma\nNormalBG=bogus\n");
        assert_eq!(p.normal, 0xFF112233);
        assert_eq!(p.font, "Tahoma");
        assert_eq!(p.normal_bg, 0xFF000000);
    }

    #[test]
    fn corrupt_sheet_is_skipped() {
        let mut files = HashMap::new();
        files.insert("main.bmp".to_string(), b"BMnot really".to_vec());
        let s = Skin::from_files("x".into(), files);
        assert!(s.sheet(Sheet::Main).is_none());
    }

    fn bmp(w: u32, h: u32) -> Vec<u8> {
        let mut out = Cursor::new(Vec::new());
        image::RgbImage::new(w, h)
            .write_to(&mut out, image::ImageFormat::Bmp)
            .unwrap();
        out.into_inner()
    }

    fn zip_of(entries: &[(&str, &[u8])]) -> Vec<u8> {
        use std::io::Write;
        let mut w = zip::ZipWriter::new(Cursor::new(Vec::new()));
        let opts = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated)
            .large_file(true);
        for (name, bytes) in entries {
            w.start_file(*name, opts).unwrap();
            w.write_all(bytes).unwrap();
        }
        w.finish().unwrap().into_inner()
    }

    #[test]
    fn huge_declared_bmp_is_refused() {
        // A 54-byte header claiming 100000×100000 at 24 bpp: 30 GB decoded.
        let mut b = bmp(1, 1);
        b[18..22].copy_from_slice(&100_000u32.to_le_bytes());
        b[22..26].copy_from_slice(&100_000u32.to_le_bytes());
        let t = std::time::Instant::now();
        assert!(Image::decode(&b).is_none());
        assert!(Image::decode(&bmp(MAX_SHEET_DIM + 1, 1)).is_none());
        assert!(t.elapsed() < std::time::Duration::from_secs(1));
        assert!(Image::decode(&bmp(275, 116)).is_some());
    }

    #[test]
    fn zip_total_is_bounded() {
        // 15 wanted sheets of just under 8 MB each (they deflate to a few KB):
        // 120 MB unpacked, more than the archive budget.
        let big = vec![0u8; (MAX_ENTRY - 1) as usize];
        let oversized = vec![0u8; (MAX_ENTRY + 1) as usize];
        let names: Vec<String> = (0..5).map(|i| format!("x{i}/region.txt")).collect();
        // Unused files are never inflated; entries declared oversized are
        // skipped without inflating them.
        let mut entries: Vec<(&str, &[u8])> = vec![("readme.png", &big)];
        entries.extend(names.iter().map(|n| (n.as_str(), &oversized[..])));
        entries.extend(Sheet::ALL.iter().map(|s| (s.file(), &big[..])));
        let z = zip_of(&entries);
        let t = std::time::Instant::now();
        let files = read_zip_files(&z).unwrap();
        assert!(t.elapsed() < std::time::Duration::from_secs(5));
        assert!(!files.contains_key("region.txt"));
        let total: usize = files.values().map(Vec::len).sum();
        assert!(total as u64 <= MAX_TOTAL, "{total}");
        // Four sheets fit the budget; the fifth ran it out.
        assert_eq!(files.len(), 4);
    }

    #[test]
    fn zip_keeps_only_used_files() {
        let main = bmp(275, 116);
        let z = zip_of(&[
            ("Skin/Main.BMP", &main),
            ("skin/readme.txt", b"hi"),
            ("skin/extra.bmp", &main),
            ("skin/region.txt", b""),
        ]);
        let files = read_zip_files(&z).unwrap();
        let mut names: Vec<_> = files.keys().cloned().collect();
        names.sort();
        assert_eq!(names, ["main.bmp", "region.txt"]);
        assert!(Skin::from_wsz("x", &z).unwrap().has_main());
    }

    #[test]
    fn folder_walk_skips_symlinks_and_deep_dirs() {
        let dir = std::env::temp_dir().join(format!("llama-test-folder-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("a/b/c")).unwrap();
        std::os::unix::fs::symlink(&dir, dir.join("loop")).unwrap();
        std::os::unix::fs::symlink(&dir, dir.join("a/b/loop")).unwrap();
        std::os::unix::fs::symlink("/dev/zero", dir.join("pledit.txt")).unwrap();
        std::fs::write(dir.join("a/b/main.bmp"), bmp(275, 116)).unwrap();
        std::fs::write(dir.join("a/b/c/viscolor.txt"), b"1,2,3").unwrap();
        std::fs::write(dir.join("a/notes.txt"), b"hi").unwrap();
        let files = read_dir_files(&dir).unwrap();
        let mut names: Vec<_> = files.keys().cloned().collect();
        names.sort();
        assert_eq!(names, ["main.bmp"]);
        assert!(Skin::load(&dir).unwrap().has_main());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}

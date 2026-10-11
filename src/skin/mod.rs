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

    fn decode(bytes: &[u8]) -> Option<Image> {
        let img = image::load_from_memory(bytes).ok()?.to_rgba8();
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
        let files = if path.is_dir() {
            read_dir_files(path)?
        } else {
            let bytes =
                std::fs::read(path).with_context(|| format!("reading {}", path.display()))?;
            read_zip_files(&bytes)?
        };
        Ok(Skin::from_files(name, files))
    }

    pub fn from_wsz(name: &str, bytes: &[u8]) -> Result<Skin> {
        Ok(Skin::from_files(name.to_string(), read_zip_files(bytes)?))
    }

    /// Builds a skin from (lower-cased basename → bytes).
    fn from_files(name: String, files: HashMap<String, Vec<u8>>) -> Skin {
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

fn wanted(name: &str) -> bool {
    name.ends_with(".bmp") || name.ends_with(".txt") || name.ends_with(".png")
}

const MAX_ENTRY: u64 = 8 << 20;

fn read_zip_files(bytes: &[u8]) -> Result<HashMap<String, Vec<u8>>> {
    let mut zip = zip::ZipArchive::new(Cursor::new(bytes)).context("not a zip archive")?;
    let mut files = HashMap::new();
    for i in 0..zip.len() {
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
        let mut buf = Vec::with_capacity(entry.size() as usize);
        if entry.by_ref().take(MAX_ENTRY).read_to_end(&mut buf).is_ok() {
            files.insert(name, buf);
        }
    }
    if files.is_empty() {
        bail!("no skin files in archive");
    }
    Ok(files)
}

fn read_dir_files(dir: &Path) -> Result<HashMap<String, Vec<u8>>> {
    let mut files = HashMap::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for entry in std::fs::read_dir(&d)?.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            let name = basename(&path.to_string_lossy());
            if wanted(&name)
                && !files.contains_key(&name)
                && let Ok(b) = std::fs::read(&path)
            {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn skin(name: &str) -> Skin {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/skins")
            .join(format!("{name}.wsz"));
        Skin::load(&path).unwrap()
    }

    #[test]
    fn base_skin_has_every_sheet() {
        let s = skin("base-2.91");
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
        let s = skin("XMMS-Turquoise");
        assert!(s.sheet(Sheet::Pledit).is_some());
        assert!(s.sheet(Sheet::Main).is_some());
    }

    #[test]
    fn nums_ex_preferred() {
        let s = skin("Green-Dimension-V2");
        let (_, ex) = s.digits().unwrap();
        assert!(ex);
    }

    #[test]
    fn fallback_fills_missing_sheets_but_keeps_digits() {
        let base = skin("base-2.91");
        // MacOSXAqua ships nums_ex but no numbers.bmp, and no titlebar cursor files.
        let s = skin("MacOSXAqua1-5").with_fallback(&base);
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
            let s = skin(name);
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
}

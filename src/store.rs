//! Where Llama keeps things, and the small state file the running window and
//! the bar widget share.
//!
//! - `~/.local/state/llama/state.json`: the worn skin and window layout. The
//!   window re-reads it when it changes, so `llama skin use` from the bar
//!   re-dresses a running player.
//! - `~/.local/share/llama/skins/`: installed skins, plus the base skin
//!   that fills in sheets a skin lacks.
//! - `~/.cache/llama/`: Museum listings and screenshots.

use std::path::{Path, PathBuf};

use anyhow::Result;
use serde::{Deserialize, Serialize};

fn env_dir(var: &str, fallback: &str) -> PathBuf {
    std::env::var_os(var)
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            let home = std::env::var_os("HOME")
                .map(PathBuf::from)
                .unwrap_or_default();
            home.join(fallback)
        })
}

pub fn state_dir() -> PathBuf {
    env_dir("XDG_STATE_HOME", ".local/state").join("llama")
}

pub fn data_dir() -> PathBuf {
    env_dir("XDG_DATA_HOME", ".local/share").join("llama")
}

pub fn cache_dir() -> PathBuf {
    env_dir("XDG_CACHE_HOME", ".cache").join("llama")
}

pub fn skins_dir() -> PathBuf {
    data_dir().join("skins")
}

pub fn base_skin_path() -> PathBuf {
    data_dir().join("base-2.91.wsz")
}

pub fn state_path() -> PathBuf {
    state_dir().join("state.json")
}

pub fn runtime_dir() -> PathBuf {
    std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct State {
    /// The worn skin file; `None` wears the base skin.
    pub skin: Option<PathBuf>,
    /// Display name for the bar widget.
    pub skin_name: Option<String>,
    /// Museum md5 when the skin came from the Museum.
    pub skin_md5: Option<String>,
    /// Physical pixels per skin pixel at scale 1: 2 normal, 4 double size.
    pub zoom: i32,
    pub shaded: bool,
    pub eq_open: bool,
    pub pl_open: bool,
    pub pl_h: i32,
    pub time_remaining: bool,
    pub vis_on: bool,
    /// Where the window last sat (Hyprland layout pixels), restored when it
    /// opens or comes back from being minimized.
    pub pos: Option<(i32, i32)>,
}

impl Default for State {
    fn default() -> Self {
        State {
            skin: None,
            skin_name: None,
            skin_md5: None,
            zoom: 2,
            shaded: false,
            eq_open: true,
            pl_open: true,
            pl_h: 116 + 29 * 2,
            time_remaining: false,
            vis_on: true,
            pos: None,
        }
    }
}

impl State {
    pub fn load() -> State {
        std::fs::read(state_path())
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) -> Result<()> {
        write_atomic(&state_path(), &serde_json::to_vec_pretty(self)?)
    }
}

/// Writes through a temp file and a rename, so a reader never sees half a
/// file.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension(format!("tmp{}", std::process::id()));
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

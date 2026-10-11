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
use std::sync::atomic::{AtomicU64, Ordering};

use anyhow::{Result, bail};
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

/// `$XDG_RUNTIME_DIR`, or without one a private `/tmp/llama-<uid>`. A
/// shared /tmp would let another user squat the control socket.
pub fn runtime_dir() -> PathBuf {
    if let Some(d) = std::env::var_os("XDG_RUNTIME_DIR").filter(|v| !v.is_empty()) {
        return PathBuf::from(d);
    }
    let uid = unsafe { libc::getuid() };
    let dir = std::env::temp_dir().join(format!("llama-{uid}"));
    match private_dir(&dir, uid) {
        Ok(()) => dir,
        Err(e) => {
            // Somebody else owns it: fall back to our own state folder
            // rather than trusting theirs.
            log::warn!("{}: {e:#}", dir.display());
            let dir = state_dir();
            let _ = std::fs::create_dir_all(&dir);
            dir
        }
    }
}

/// Makes `dir` a 0700 folder owned by `uid`, or checks an existing one is.
fn private_dir(dir: &Path, uid: u32) -> Result<()> {
    use std::os::unix::fs::{DirBuilderExt, MetadataExt};
    match std::fs::DirBuilder::new().mode(0o700).create(dir) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(e) => return Err(e.into()),
    }
    // symlink_metadata: a symlink planted there is not ours either.
    let m = std::fs::symlink_metadata(dir)?;
    if !m.is_dir() || m.uid() != uid || m.mode() & 0o077 != 0 {
        bail!("not a private folder owned by uid {uid}");
    }
    Ok(())
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
    /// Double size is on: the window shows at twice `zoom`.
    pub double: bool,
    /// Shown on every workspace.
    pub pinned: bool,
    /// The EQ is switched off, and this is the curve ON puts back.
    pub eq_saved: Option<Vec<f64>>,
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
            double: false,
            pinned: false,
            eq_saved: None,
        }
    }
}

impl State {
    pub fn load() -> State {
        std::fs::read(state_path())
            .ok()
            .and_then(|b| serde_json::from_slice::<State>(&b).ok())
            .unwrap_or_default()
            .sane()
    }

    /// Clamps what a hand-edited state file could blow up: the zoom and the
    /// playlist height size the window's pixel buffer.
    fn sane(mut self) -> State {
        use crate::skin::sprites::{PL_MIN_H, PL_STEP_H};
        // Twenty steps past the minimum is taller than any screen.
        self.zoom = self.zoom.clamp(1, 8);
        self.pl_h = self.pl_h.clamp(PL_MIN_H, PL_MIN_H + PL_STEP_H * 20);
        if self.double {
            self.zoom = self.zoom.min(4);
        }
        if let Some(eq) = self.eq_saved.as_mut() {
            eq.truncate(10);
            for v in eq.iter_mut() {
                *v = if v.is_finite() {
                    v.clamp(-12.0, 12.0)
                } else {
                    0.0
                };
            }
        }
        self
    }

    pub fn save(&self) -> Result<()> {
        write_atomic(&state_path(), &serde_json::to_vec_pretty(self)?)
    }
}

/// Writes through a temp file and a rename, so a reader never sees half a
/// file. The temp name is unique per process and per call, so threads
/// writing the same file don't interleave.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    static SEQ: AtomicU64 = AtomicU64::new(0);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(format!(
        ".tmp{}-{}",
        std::process::id(),
        SEQ.fetch_add(1, Ordering::Relaxed)
    ));
    let tmp = PathBuf::from(tmp);
    if let Err(e) = std::fs::write(&tmp, bytes).and_then(|()| std::fs::rename(&tmp, path)) {
        let _ = std::fs::remove_file(&tmp);
        return Err(e.into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_is_clamped_on_load() {
        let st: State = serde_json::from_str(r#"{"zoom": 1000, "pl_h": 2000000000}"#).unwrap();
        let st = st.sane();
        assert_eq!(st.zoom, 8);
        assert_eq!(st.pl_h, 116 + 29 * 20);
        let st: State = serde_json::from_str(r#"{"zoom": -3, "pl_h": -1}"#).unwrap();
        let st = st.sane();
        assert_eq!((st.zoom, st.pl_h), (1, 116));
        // Missing fields keep their defaults.
        let st: State = serde_json::from_str(r#"{"shaded": true}"#).unwrap();
        assert_eq!(
            st.sane(),
            State {
                shaded: true,
                ..State::default()
            }
        );
    }

    #[test]
    fn concurrent_atomic_writes_never_interleave() {
        let dir = std::env::temp_dir().join(format!("llama-test-atomic-{}", std::process::id()));
        let path = dir.join("f");
        let threads: Vec<_> = (0..8u8)
            .map(|i| {
                let path = path.clone();
                std::thread::spawn(move || {
                    for _ in 0..20 {
                        write_atomic(&path, &vec![i; 64 << 10]).unwrap();
                    }
                })
            })
            .collect();
        for t in threads {
            t.join().unwrap();
        }
        let b = std::fs::read(&path).unwrap();
        assert_eq!(b.len(), 64 << 10);
        assert!(b.iter().all(|&x| x == b[0]));
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn private_dir_refuses_loose_folders() {
        use std::os::unix::fs::PermissionsExt;
        let uid = unsafe { libc::getuid() };
        let dir = std::env::temp_dir().join(format!("llama-test-rt-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        private_dir(&dir, uid).unwrap();
        private_dir(&dir, uid).unwrap();
        assert!(private_dir(&dir, uid + 1).is_err());
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o777)).unwrap();
        assert!(private_dir(&dir, uid).is_err());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}

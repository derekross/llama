//! The few things a Wayland client cannot do for itself on Hyprland: know
//! where its window sits, put it back there, and focus it.
//!
//! Everything goes through `hyprctl`. Hyprland 0.56 with a Lua config wants
//! `hl.dsp.*` expressions and rejects the classic dispatcher syntax, while
//! older versions only know the classic one, so each dispatch tries both.
//! Without Hyprland every call is a quiet no-op.

use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use serde_json::Value;

fn hyprctl(args: &[&str]) -> Option<Vec<u8>> {
    let out = Command::new("hyprctl")
        .args(args)
        .stderr(Stdio::null())
        .output()
        .ok()?;
    out.status.success().then_some(out.stdout)
}

fn dispatch(lua: &str, classic: &[&str]) {
    if hyprctl(&["dispatch", lua]).is_none() {
        let mut args = vec!["dispatch"];
        args.extend_from_slice(classic);
        let _ = hyprctl(&args);
    }
}

fn json(args: &[&str]) -> Option<Value> {
    serde_json::from_slice(&hyprctl(args)?).ok()
}

/// This process's mapped window.
struct Own {
    address: String,
    at: (i32, i32),
    size: (i32, i32),
    monitor: i64,
}

fn own_window() -> Option<Own> {
    let pid = std::process::id() as u64;
    let clients = json(&["clients", "-j"])?;
    clients.as_array()?.iter().find_map(|c| {
        if c["pid"].as_u64() != Some(pid) || c["mapped"].as_bool() == Some(false) {
            return None;
        }
        let pair = |k: &str| -> Option<(i32, i32)> {
            let a = c[k].as_array()?;
            Some((a.first()?.as_i64()? as i32, a.get(1)?.as_i64()? as i32))
        };
        Some(Own {
            address: c["address"].as_str()?.to_string(),
            at: pair("at")?,
            size: pair("size")?,
            monitor: c["monitor"].as_i64()?,
        })
    })
}

/// Where this process's window is, in Hyprland's global layout pixels.
pub fn position() -> Option<(i32, i32)> {
    own_window().map(|w| w.at)
}

/// A monitor's area in layout pixels: (x, y, w, h).
fn monitor_area(id: i64) -> Option<(i32, i32, i32, i32)> {
    let monitors = json(&["monitors", "-j"])?;
    let m = monitors
        .as_array()?
        .iter()
        .find(|m| m["id"].as_i64() == Some(id))?;
    let get = |k: &str| m[k].as_f64().unwrap_or(0.0);
    let scale = get("scale").max(0.1);
    // Layout size is the mode size over the scale (rotated if transformed).
    let (mut w, mut h) = (get("width") / scale, get("height") / scale);
    if m["transform"].as_i64().unwrap_or(0) % 2 == 1 {
        std::mem::swap(&mut w, &mut h);
    }
    Some((get("x") as i32, get("y") as i32, w as i32, h as i32))
}

/// Puts (x, y) back once Hyprland has mapped the window, but only on the
/// monitor it opened on: the window belongs to the workspace it mapped on,
/// and dragging it onto another monitor's coordinates would leave it on
/// the wrong output at the wrong scale. Elsewhere, Hyprland's own placement
/// (centred) stands. The window is kept wholly on its monitor. Runs in the
/// background: mapping follows the first commit asynchronously.
pub fn restore(x: i32, y: i32) {
    std::thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(2);
        while Instant::now() < deadline {
            let Some(w) = own_window() else {
                std::thread::sleep(Duration::from_millis(40));
                continue;
            };
            let Some(area) = monitor_area(w.monitor) else {
                return;
            };
            if let Some((tx, ty)) = place(area, w.size, (x, y))
                && (tx, ty) != w.at
            {
                let window = format!("address:{}", w.address);
                dispatch(
                    &format!("hl.dsp.window.move({{ window = \"{window}\", x = {tx}, y = {ty} }})"),
                    &["movewindowpixel", &format!("exact {tx} {ty},{window}")],
                );
            }
            return;
        }
    });
}

/// Where a window of `size` goes on monitor `area` for a saved top-left
/// `pos`: `None` when `pos` is not on that monitor, else `pos` nudged so the
/// whole window stays on it.
fn place(area: (i32, i32, i32, i32), size: (i32, i32), pos: (i32, i32)) -> Option<(i32, i32)> {
    let (mx, my, mw, mh) = area;
    let (x, y) = pos;
    if x < mx || x >= mx + mw || y < my || y >= my + mh {
        return None;
    }
    let x = x.min(mx + mw - size.0).max(mx);
    let y = y.min(my + mh - size.1).max(my);
    Some((x, y))
}

/// Focuses the Llama window (from a second `llama`, which is another
/// process, so it goes by class).
pub fn focus(app_id: &str) {
    let window = format!("class:^({app_id})$");
    dispatch(
        &format!("hl.dsp.focus({{ window = \"{window}\" }})"),
        &["focuswindow", &window],
    );
}

/// Pins this process's window (shown on every workspace) or unpins it,
/// once Hyprland has it mapped. Pin is a toggle, so it checks first.
pub fn pin(on: bool) {
    std::thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(2);
        while Instant::now() < deadline {
            let pid = std::process::id() as u64;
            let found = json(&["clients", "-j"]).and_then(|c| {
                c.as_array()?.iter().find_map(|c| {
                    (c["pid"].as_u64() == Some(pid) && c["mapped"].as_bool() != Some(false)).then(
                        || {
                            (
                                c["address"].as_str().unwrap_or_default().to_string(),
                                c["pinned"].as_bool().unwrap_or(false),
                            )
                        },
                    )
                })
            });
            if let Some((address, pinned)) = found {
                if pinned != on && !address.is_empty() {
                    let window = format!("address:{address}");
                    dispatch(
                        &format!("hl.dsp.window.pin({{ window = \"{window}\" }})"),
                        &["pin", &window],
                    );
                }
                return;
            }
            std::thread::sleep(Duration::from_millis(40));
        }
    });
}

#[cfg(test)]
mod tests {
    use super::place;

    #[test]
    fn restores_only_on_the_same_monitor() {
        let laptop = (3440, 0, 1280, 800);
        // Saved on the laptop, opened on the laptop: back where it was.
        assert_eq!(place(laptop, (368, 542), (4261, 123)), Some((4261, 123)));
        // Saved on the other monitor: Hyprland's placement stands.
        assert_eq!(place(laptop, (368, 542), (1445, 327)), None);
        // Near the edge: kept wholly on screen.
        assert_eq!(place(laptop, (368, 542), (4600, 500)), Some((4352, 258)));
    }
}

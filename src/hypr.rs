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

/// This process's mapped window: (address, x, y).
fn own_window() -> Option<(String, i32, i32)> {
    let pid = std::process::id() as u64;
    let clients = json(&["clients", "-j"])?;
    clients.as_array()?.iter().find_map(|c| {
        if c["pid"].as_u64() != Some(pid) || c["mapped"].as_bool() == Some(false) {
            return None;
        }
        let at = c["at"].as_array()?;
        Some((
            c["address"].as_str()?.to_string(),
            at.first()?.as_i64()? as i32,
            at.get(1)?.as_i64()? as i32,
        ))
    })
}

/// Where this process's window is, in Hyprland's global layout pixels.
pub fn position() -> Option<(i32, i32)> {
    own_window().map(|(_, x, y)| (x, y))
}

/// True when (x, y) lies on a connected monitor, so a position saved on a
/// display that has since gone away is not restored off-screen.
fn on_screen(x: i32, y: i32) -> bool {
    let Some(monitors) = json(&["monitors", "-j"]) else {
        return false;
    };
    monitors.as_array().into_iter().flatten().any(|m| {
        let get = |k: &str| m[k].as_f64().unwrap_or(0.0);
        let scale = get("scale").max(0.1);
        // Layout size is the mode size over the scale (rotated if transformed).
        let (mut w, mut h) = (get("width") / scale, get("height") / scale);
        if m["transform"].as_i64().unwrap_or(0) % 2 == 1 {
            std::mem::swap(&mut w, &mut h);
        }
        let (mx, my) = (get("x"), get("y"));
        let (x, y) = (x as f64, y as f64);
        x >= mx && x < mx + w && y >= my && y < my + h
    })
}

/// Moves this process's window to (x, y) once Hyprland has mapped it. Runs
/// in the background: mapping follows the first commit asynchronously.
pub fn restore(x: i32, y: i32) {
    std::thread::spawn(move || {
        if !on_screen(x, y) {
            return;
        }
        let deadline = Instant::now() + Duration::from_secs(2);
        while Instant::now() < deadline {
            if let Some((address, cx, cy)) = own_window() {
                if (cx, cy) != (x, y) {
                    let window = format!("address:{address}");
                    dispatch(
                        &format!(
                            "hl.dsp.window.move({{ window = \"{window}\", x = {x}, y = {y} }})"
                        ),
                        &["movewindowpixel", &format!("exact {x} {y},{window}")],
                    );
                }
                return;
            }
            std::thread::sleep(Duration::from_millis(40));
        }
    });
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

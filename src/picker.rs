//! The desktop's file chooser, for eject, L and the playlist's ADD menu.
//!
//! It asks xdg-desktop-portal through `gdbus` (always there on Omarchy) and
//! falls back to zenity. Both block until the person answers, so callers run
//! this on a thread. Folders are walked here: cliamp queues one entry per
//! path and does not expand a directory.

use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::{Result, bail};

/// Audio cliamp can decode, by extension.
const AUDIO: &[&str] = &[
    "mp3", "flac", "ogg", "oga", "opus", "m4a", "m4b", "aac", "wav", "wv", "ape", "mpc", "aif",
    "aiff", "alac", "wma", "mka", "webm",
];

/// Most files one folder pick adds.
const MAX_FILES: usize = 10_000;

pub fn is_audio(p: &Path) -> bool {
    p.extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
        .is_some_and(|e| AUDIO.contains(&e.as_str()))
}

/// Asks for files (or one folder). `Ok(None)` means the person cancelled;
/// `Err` means no chooser is available.
pub fn pick(title: &str, folder: bool) -> Result<Option<Vec<PathBuf>>> {
    match portal(title, folder) {
        Ok(r) => Ok(r),
        Err(e) => {
            log::info!("file chooser portal: {e:#}; trying zenity");
            zenity(title, folder)
        }
    }
}

fn portal(title: &str, folder: bool) -> Result<Option<Vec<PathBuf>>> {
    let token = format!("llama{}{}", std::process::id(), rand_suffix());
    // Watch for the answer before asking, so it cannot slip past.
    let mut monitor = Command::new("gdbus")
        .args([
            "monitor",
            "--session",
            "--dest",
            "org.freedesktop.portal.Desktop",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()?;
    let stdout = monitor.stdout.take().expect("piped");
    let mut lines = BufReader::new(stdout).lines();
    // The first line confirms the subscription.
    let _ = lines.next();

    let filters = if folder {
        String::new()
    } else {
        ", 'filters': <[('Audio', [(uint32 1, 'audio/*')]), ('All files', [(uint32 0, '*')])]>"
            .into()
    };
    let options = format!(
        "{{'handle_token': <'{token}'>, 'multiple': <{}>, 'directory': <{folder}>{filters}}}",
        !folder
    );
    let call = Command::new("gdbus")
        .args([
            "call",
            "--session",
            "--dest",
            "org.freedesktop.portal.Desktop",
            "--object-path",
            "/org/freedesktop/portal/desktop",
            "--method",
            "org.freedesktop.portal.FileChooser.OpenFile",
            "",
            title,
            &options,
        ])
        .stderr(Stdio::piped())
        .output();
    let ok = matches!(&call, Ok(o) if o.status.success());
    if !ok {
        let _ = monitor.kill();
        let _ = monitor.wait();
        let why = call
            .map(|o| String::from_utf8_lossy(&o.stderr).trim().to_string())
            .unwrap_or_else(|e| e.to_string());
        bail!("OpenFile failed: {why}");
    }

    let mut answer = None;
    for line in lines.map_while(Result::ok) {
        if line.contains(&token) && line.contains("Request.Response") {
            answer = Some(line);
            break;
        }
    }
    let _ = monitor.kill();
    let _ = monitor.wait();
    let Some(line) = answer else {
        bail!("the portal never answered");
    };
    Ok(parse_response(&line))
}

/// Reads `(uint32 0, {'uris': <['file:///a', 'file:///b']>, ...})`.
fn parse_response(line: &str) -> Option<Vec<PathBuf>> {
    let body = line.split_once("Response (")?.1;
    if !body.trim_start().starts_with("uint32 0") {
        return None; // 1 = cancelled, 2 = failed
    }
    let uris = body.split_once("'uris': <[")?.1;
    let uris = &uris[..uris.find("]>")?];
    let list: String = uris
        .split(", ")
        .map(|u| u.trim().trim_matches('\''))
        .filter(|u| !u.is_empty())
        .map(|u| format!("{u}\n"))
        .collect();
    Some(crate::wl::uri_files(&list))
}

fn zenity(title: &str, folder: bool) -> Result<Option<Vec<PathBuf>>> {
    let mut cmd = Command::new("zenity");
    cmd.args(["--file-selection", "--title", title, "--separator", "\n"]);
    if folder {
        cmd.arg("--directory");
    } else {
        cmd.arg("--multiple");
    }
    let out = cmd.stderr(Stdio::null()).output();
    let out = match out {
        Ok(o) => o,
        Err(e) => bail!("no file chooser (portal or zenity): {e}"),
    };
    if !out.status.success() {
        return Ok(None);
    }
    let files: Vec<PathBuf> = String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter(|l| !l.is_empty())
        .map(PathBuf::from)
        .collect();
    Ok((!files.is_empty()).then_some(files))
}

fn rand_suffix() -> u32 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0)
}

/// The audio files under `dir`, sorted by path, without following links.
pub fn audio_in(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![(dir.to_path_buf(), 0)];
    while let Some((d, depth)) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&d) else {
            continue;
        };
        for e in entries.flatten() {
            let Ok(ft) = e.file_type() else { continue };
            let p = e.path();
            if ft.is_dir() && depth < 16 {
                stack.push((p, depth + 1));
            } else if ft.is_file() && is_audio(&p) {
                out.push(p);
                if out.len() >= MAX_FILES {
                    out.sort();
                    return out;
                }
            }
        }
    }
    out.sort();
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn portal_answers() {
        let ok = "/org/freedesktop/portal/desktop/request/1_9/llama1: org.freedesktop.portal.Request.Response (uint32 0, {'uris': <['file:///home/a/x%20y.mp3', 'file:///home/a/z.flac']>, 'choices': <@a(ss) []>})";
        assert_eq!(
            parse_response(ok).unwrap(),
            [
                PathBuf::from("/home/a/x y.mp3"),
                PathBuf::from("/home/a/z.flac")
            ]
        );
        let cancelled = "/x/llama1: org.freedesktop.portal.Request.Response (uint32 1, @a{sv} {})";
        assert!(parse_response(cancelled).is_none());
    }

    #[test]
    fn walks_folders_for_audio_only() {
        let d = std::env::temp_dir().join(format!("llama-walk-{}", std::process::id()));
        std::fs::create_dir_all(d.join("b")).unwrap();
        for f in ["b/2.mp3", "1.FLAC", "cover.jpg"] {
            std::fs::write(d.join(f), b"").unwrap();
        }
        let got = audio_in(&d);
        std::fs::remove_dir_all(&d).unwrap();
        assert_eq!(got, [d.join("1.FLAC"), d.join("b/2.mp3")]);
    }
}

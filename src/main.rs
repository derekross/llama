//! `llama`: the Winamp window, and the small CLI the bar widget drives.

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::OnceLock;
use std::time::{Duration, Instant, SystemTime};

use anyhow::{Context, Result, bail};
use serde_json::{Value, json};

use llama::museum;
use llama::skin::Skin;
use llama::store::{self, State};
use smithay_client_toolkit::reexports::calloop::ping::{Ping, make_ping};

const HELP: &str = "\
llama — Winamp 2.x for Omarchy, with cliamp as the engine

  llama                       open the player (or focus it if it is open)
  llama --skin <md5|link|file>  wear a skin, then open
  llama --zoom N              physical pixels per skin pixel (default 2)
  llama --no-engine           do not start a cliamp daemon
  llama --hide                minimize the player (llama brings it back)
  llama --quit                close a running player

  llama skin use <md5|museum link|file.wsz>   wear a skin (live)
  llama skin base                             go back to the base skin
  llama skin current                          the worn skin, as JSON
  llama skin featured [--page N]              Museum top skins, as JSON
  llama skin search <text> [--page N]         search ~100k skins, as JSON
  llama skin list                             installed skins, as JSON
";

const PAGE: usize = 30;

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn")).init();
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match args.first().map(String::as_str) {
        Some("skin") => skin_cmd(&args[1..]),
        Some("-h" | "--help" | "help") => {
            print!("{HELP}");
            Ok(())
        }
        _ => player(&args),
    };
    if let Err(e) = result {
        eprintln!("llama: {e:#}");
        std::process::exit(1);
    }
}

// ---- the player --------------------------------------------------------------

fn control_path() -> PathBuf {
    store::runtime_dir().join("llama.sock")
}

fn send_control(cmd: &str) -> Option<String> {
    let mut s = UnixStream::connect(control_path()).ok()?;
    s.set_read_timeout(Some(Duration::from_secs(2))).ok()?;
    writeln!(s, "{cmd}").ok()?;
    let mut line = String::new();
    BufReader::new(s).read_line(&mut line).ok()?;
    Some(line.trim().to_string())
}

fn focus_existing() {
    llama::hypr::focus(llama::wl::APP_ID);
}

static QUIT: OnceLock<Ping> = OnceLock::new();

extern "C" fn on_signal(_: libc::c_int) {
    if let Some(p) = QUIT.get() {
        p.ping();
    }
}

fn player(args: &[String]) -> Result<()> {
    let mut engine = true;
    let mut skin_arg = None;
    let mut zoom = None;
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--skin" => skin_arg = Some(it.next().context("--skin needs a value")?.clone()),
            "--zoom" => {
                zoom = Some(
                    it.next()
                        .context("--zoom needs a number")?
                        .parse::<i32>()
                        .context("--zoom")?,
                )
            }
            "--no-engine" => engine = false,
            "--hide" => {
                if send_control("hide").is_none() {
                    println!("Llama is not running");
                }
                return Ok(());
            }
            "--quit" => {
                match send_control("quit") {
                    Some(_) => println!("Llama closed"),
                    None => println!("Llama was not running"),
                }
                return Ok(());
            }
            other => bail!("unknown option {other} (see llama --help)"),
        }
    }

    if let Some(s) = &skin_arg {
        llama::skins::wear(s)?;
    }
    if let Some(z) = zoom {
        let mut st = State::load();
        st.zoom = z.clamp(1, 8);
        st.save()?;
    }

    // One window at a time: a second launch focuses the first.
    if send_control("show").is_some() {
        // A minimized window maps again asynchronously; give it a moment
        // before asking Hyprland to focus it.
        std::thread::sleep(Duration::from_millis(150));
        focus_existing();
        return Ok(());
    }
    let _ = std::fs::remove_file(control_path());
    let listener = UnixListener::bind(control_path()).context("control socket")?;

    let base = load_base();

    let (ping, quit_src) = make_ping()?;
    let (show, show_src) = make_ping()?;
    let (hide, hide_src) = make_ping()?;
    let _ = QUIT.set(ping.clone());
    unsafe {
        libc::signal(libc::SIGTERM, on_signal as *const () as libc::sighandler_t);
        libc::signal(libc::SIGINT, on_signal as *const () as libc::sighandler_t);
        libc::signal(libc::SIGHUP, on_signal as *const () as libc::sighandler_t);
    }
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let mut line = String::new();
            let mut r = BufReader::new(&stream);
            if r.read_line(&mut line).is_err() {
                continue;
            }
            let mut w = &stream;
            let _ = writeln!(w, "ok");
            match line.trim() {
                "quit" => ping.ping(),
                "show" => show.ping(),
                "hide" => hide.ping(),
                _ => {}
            }
        }
    });

    // OmaAmp's rule, and Winamp's: closing the player stops the music, but
    // only a cliamp this window started. A TUI the user opened is theirs.
    let started = if engine && !llama::cliamp::is_running() {
        start_engine()
    } else {
        None
    };

    let result = llama::wl::run(llama::wl::Opts {
        base,
        quit: quit_src,
        show: show_src,
        hide: hide_src,
    });

    if let Some(mut child) = started {
        // Signal it only while it is still our child: once reaped, its pid
        // could belong to anyone.
        if let Ok(None) = child.try_wait() {
            unsafe {
                libc::kill(child.id() as libc::pid_t, libc::SIGTERM);
            }
        }
        // A busy cliamp finishes its current job before it exits; reap it if
        // that is quick, and otherwise leave it to finish on its own.
        let deadline = Instant::now() + Duration::from_secs(2);
        while Instant::now() < deadline && matches!(child.try_wait(), Ok(None)) {
            std::thread::sleep(Duration::from_millis(50));
        }
    }
    let _ = std::fs::remove_file(control_path());
    result
}

fn load_base() -> Skin {
    let path = match museum::ensure_base() {
        Ok(p) => p,
        Err(e) => {
            log::warn!("base skin unavailable: {e:#}");
            store::base_skin_path()
        }
    };
    Skin::load(&path).unwrap_or_else(|e| {
        log::warn!("base skin unreadable: {e:#}");
        Skin::default()
    })
}

/// Starts `cliamp -d` in its own session and waits for its socket.
/// Starts `cliamp -d` and returns it while it is the daemon serving the
/// socket. One that exits at once (another cliamp got there first) is reaped
/// and not claimed.
fn start_engine() -> Option<std::process::Child> {
    use std::os::unix::process::CommandExt;
    let child = Command::new("cliamp")
        .arg("-d")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .process_group(0)
        .spawn();
    let child = match child {
        Ok(c) => c,
        Err(e) => {
            log::warn!("could not start cliamp: {e}");
            return None;
        }
    };
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut child = child;
    while Instant::now() < deadline && !llama::cliamp::is_running() {
        if let Ok(Some(status)) = child.try_wait() {
            log::warn!("cliamp -d exited at once ({status})");
            return None;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    match child.try_wait() {
        Ok(None) => Some(child),
        _ => None,
    }
}

// ---- skins -------------------------------------------------------------------

fn print(v: Value) {
    println!("{v}");
}

fn page_arg(args: &[String]) -> usize {
    args.windows(2)
        .find(|w| w[0] == "--page")
        .and_then(|w| w[1].parse().ok())
        .unwrap_or(0)
}

fn listing(refs: Vec<museum::SkinRef>) -> Value {
    json!({
        "skins": refs.iter().map(|r| json!({
            "md5": r.md5,
            "name": r.name(),
            "screenshot": r.screenshot_url,
        })).collect::<Vec<_>>()
    })
}

fn skin_cmd(args: &[String]) -> Result<()> {
    match args.first().map(String::as_str) {
        Some("use") => {
            let target = args.get(1).context("llama skin use <md5|link|file>")?;
            let st = llama::skins::wear(target)?;
            print(current_json(&st));
        }
        Some("base") => {
            let mut st = State::load();
            st.skin = None;
            st.skin_name = None;
            st.skin_md5 = None;
            st.save()?;
            print(current_json(&st));
        }
        Some("current") => print(current_json(&State::load())),
        Some("featured") => {
            let page = page_arg(args);
            print(listing(cached_featured(page)?));
        }
        Some("search") => {
            let q = args
                .get(1)
                .filter(|q| !q.starts_with("--"))
                .context("llama skin search <text>")?;
            print(listing(museum::search(q, PAGE, page_arg(args) * PAGE)?));
        }
        Some("list") => {
            let mut out = Vec::new();
            if let Ok(dir) = std::fs::read_dir(store::skins_dir()) {
                for e in dir.flatten() {
                    let p = e.path();
                    if p.extension().is_some_and(|x| x == "wsz") {
                        let name = std::fs::read_to_string(p.with_extension("name"))
                            .unwrap_or_else(|_| {
                                p.file_stem()
                                    .unwrap_or_default()
                                    .to_string_lossy()
                                    .into_owned()
                            });
                        out.push(json!({"name": name, "path": p}));
                    }
                }
            }
            print(json!({"skins": out}));
        }
        _ => bail!("llama skin use|base|current|featured|search|list (see llama --help)"),
    }
    Ok(())
}

fn current_json(st: &State) -> Value {
    json!({
        "name": st.skin_name.clone().unwrap_or_else(|| "Base Skin".into()),
        "md5": st.skin_md5,
        "path": st.skin,
        "running": send_control("ping").is_some(),
    })
}

/// The Museum's top skins, cached for a day so browsing works offline.
fn cached_featured(page: usize) -> Result<Vec<museum::SkinRef>> {
    let path = store::cache_dir().join(format!("featured-{page}.json"));
    let fresh = std::fs::metadata(&path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| SystemTime::now().duration_since(t).ok())
        .is_some_and(|age| age < Duration::from_secs(86_400));
    let cached = || -> Option<Vec<museum::SkinRef>> {
        serde_json::from_slice(&std::fs::read(&path).ok()?).ok()
    };
    if fresh && let Some(c) = cached() {
        return Ok(c);
    }
    match museum::featured(PAGE, page * PAGE) {
        Ok(list) => {
            let _ = store::write_atomic(&path, &serde_json::to_vec(&list)?);
            Ok(list)
        }
        Err(e) => cached().ok_or(e),
    }
}

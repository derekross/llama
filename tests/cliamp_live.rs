//! Talks to a real headless cliamp. Starts its own daemon in a throwaway
//! config dir, so it never touches the user's player:
//!
//!     cargo test --test cliamp_live -- --ignored

use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use llama::cliamp::{Client, is_running};
use serde_json::json;

struct Daemon(Child, PathBuf);

impl Drop for Daemon {
    fn drop(&mut self) {
        unsafe {
            libc::kill(self.0.id() as i32, libc::SIGTERM);
        }
        let _ = self.0.wait();
        let _ = std::fs::remove_dir_all(&self.1);
    }
}

fn daemon() -> Daemon {
    // The socket path must stay short (107 bytes), so not under target/.
    let runtime = std::env::var("XDG_RUNTIME_DIR").unwrap_or_else(|_| "/tmp".into());
    let dir = PathBuf::from(runtime).join(format!("llama-it-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    // SAFETY: tests in this file run one daemon each, before any client.
    unsafe { std::env::set_var("CLIAMP_CONFIG_DIR", &dir) };
    let music = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests");
    let child = Command::new("cliamp")
        .arg("-d")
        .arg(&music)
        .env("CLIAMP_CONFIG_DIR", &dir)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("cliamp on PATH");
    // Guard first: a panic while waiting must still stop the daemon.
    let guard = Daemon(child, dir);
    let deadline = Instant::now() + Duration::from_secs(5);
    while !is_running() {
        if Instant::now() >= deadline {
            let err = Client::connect().and_then(|mut c| c.state()).err();
            panic!("cliamp did not come up: {err:?}");
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    guard
}

#[test]
#[ignore]
fn state_queue_eq_and_volume() {
    let _d = daemon();
    let mut c = Client::connect().unwrap();
    let s = c.state().unwrap();
    assert_eq!(s.state, "stopped");
    assert_eq!(s.eq_bands.len(), 10);

    c.run("volume", json!({"value": -12})).unwrap();
    c.run("eq", json!({"band": 3, "value": 4.5})).unwrap();
    c.run("shuffle", json!({"name": "on"})).unwrap();
    let s = c.state().unwrap();
    assert_eq!(s.volume, Some(-12.0));
    assert_eq!(s.eq_bands[3], 4.5);
    assert!(s.shuffle);

    // The queue starts empty (tests/ has no audio); adding a URL and
    // clearing it moves the playlist revision.
    let before = s.playlist_revision;
    c.run("queue", json!({"path": "https://example.com/stream.mp3"}))
        .unwrap();
    let q = c.queue().unwrap();
    assert_eq!(q.len(), 1);
    c.run("queue.clear", json!({})).unwrap();
    assert!(c.queue().unwrap().is_empty());
    assert!(c.state().unwrap().playlist_revision > before);

    // Spectrum answers even while stopped.
    assert!(c.spectrum().is_ok());
}

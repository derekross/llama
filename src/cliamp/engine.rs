//! The worker thread that keeps a live picture of cliamp.
//!
//! It owns the socket connection, runs the window's operations in order,
//! polls the snapshot four times a second (cliamp sends no events for
//! position ticks), refetches the queue when its revision moves, and pulls
//! spectrum frames while the visualizer is showing.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, channel};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde_json::Value;

use super::{Client, Snapshot, Track, job_outcome};

#[derive(Clone, Debug, Default)]
pub struct Live {
    pub connected: bool,
    pub snap: Snapshot,
    /// When `snap` was taken, to run the clock between polls.
    pub at: Option<Instant>,
    pub queue: Vec<Track>,
    pub queue_raw: Vec<Value>,
    pub spectrum: Vec<f32>,
    /// Failed operations, for the window to show and clear.
    pub errors: Vec<String>,
    /// Answers to `Engine::query`, as (tag, result), for the window to take.
    pub replies: Vec<(String, Value)>,
    /// When the request now waiting on cliamp was sent.
    pub waiting_since: Option<Instant>,
}

impl Live {
    /// The playback position, advanced by the time since the last poll.
    pub fn position(&self) -> f64 {
        let mut p = self.snap.position;
        if self.snap.playing()
            && let Some(at) = self.at
        {
            p += at.elapsed().as_secs_f64();
        }
        if self.snap.duration > 0.0 {
            p.min(self.snap.duration)
        } else {
            p
        }
    }

    /// True when cliamp has left a request unanswered for a while: it serves
    /// one request at a time, and a slow provider job holds up everything
    /// behind it, Stop included. Clicks made meanwhile still go through, late.
    pub fn busy(&self) -> bool {
        self.waiting_since
            .is_some_and(|t| t.elapsed() > Duration::from_millis(1500))
    }
}

pub enum Cmd {
    Op(String, Value),
    Query(String, String, Value),
}

pub struct Engine {
    pub live: Arc<Mutex<Live>>,
    tx: Sender<Cmd>,
    pub want_spectrum: Arc<AtomicBool>,
}

impl Engine {
    /// Starts the worker. `wake` is called whenever `live` changes.
    pub fn start(wake: impl Fn() + Send + 'static) -> Engine {
        let live = Arc::new(Mutex::new(Live::default()));
        let (tx, rx) = channel();
        let want_spectrum = Arc::new(AtomicBool::new(true));
        let worker = Worker {
            live: live.clone(),
            rx,
            want_spectrum: want_spectrum.clone(),
            wake: Box::new(wake),
        };
        std::thread::Builder::new()
            .name("cliamp".into())
            .spawn(move || worker.run())
            .expect("spawn cliamp thread");
        Engine {
            live,
            tx,
            want_spectrum,
        }
    }

    pub fn op(&self, name: &str, params: Value) {
        let _ = self.tx.send(Cmd::Op(name.to_string(), params));
    }

    /// Runs an operation to completion; its result lands in `Live::replies`.
    pub fn query(&self, tag: &str, name: &str, params: Value) {
        let _ = self
            .tx
            .send(Cmd::Query(tag.to_string(), name.to_string(), params));
    }
}

struct Worker {
    live: Arc<Mutex<Live>>,
    rx: Receiver<Cmd>,
    want_spectrum: Arc<AtomicBool>,
    wake: Box<dyn Fn() + Send>,
}

const STATE_EVERY: Duration = Duration::from_millis(250);
const SPECTRUM_EVERY: Duration = Duration::from_millis(33);
/// How long one request may wait on a busy cliamp before the connection is
/// given up as dead.
const PATIENCE: Duration = Duration::from_secs(600);

impl Worker {
    fn run(self) {
        loop {
            match Client::connect_waiting(PATIENCE) {
                Ok(client) => {
                    if self.session(client).is_err() {
                        return; // the window went away
                    }
                }
                Err(_) => {
                    let changed = {
                        let mut l = self.live.lock().unwrap();
                        std::mem::replace(&mut l.connected, false)
                    };
                    if changed {
                        (self.wake)();
                    }
                }
            }
            // Commands wait in the channel for the next session: a session
            // that ended because cliamp restarted, or after a long busy
            // spell, must not lose the clicks made meanwhile. Only while
            // nothing is listening at all are they dropped, so they do not
            // replay as a burst much later.
            std::thread::sleep(Duration::from_secs(1));
            if !super::is_running() {
                let mut dropped = 0;
                loop {
                    match self.rx.try_recv() {
                        Ok(_) => dropped += 1,
                        Err(std::sync::mpsc::TryRecvError::Empty) => break,
                        Err(std::sync::mpsc::TryRecvError::Disconnected) => return,
                    }
                }
                if dropped > 0 {
                    self.note("cliamp is not running".into());
                }
            }
        }
    }

    /// Runs one request, noting how long cliamp leaves it unanswered.
    fn ask<T>(&self, c: &mut Client, f: impl FnOnce(&mut Client) -> T) -> T {
        self.live.lock().unwrap().waiting_since = Some(Instant::now());
        let out = f(c);
        let was_busy = {
            let mut l = self.live.lock().unwrap();
            let busy = l.busy();
            l.waiting_since = None;
            busy
        };
        if was_busy {
            (self.wake)();
        }
        out
    }

    /// Hands a failure to the window.
    fn note(&self, text: String) {
        self.live.lock().unwrap().errors.push(text);
        (self.wake)();
    }

    /// Checks on submitted jobs, reporting the ones that failed. Jobs are
    /// fire-and-forget for the window, but a bad path or a stale index
    /// should not fail in silence.
    fn check_jobs(&self, c: &mut Client, pending: &mut Vec<(String, String, Instant)>) {
        let mut i = 0;
        // A few per poll: each is a round trip.
        let mut budget = 8;
        while i < pending.len() && budget > 0 {
            budget -= 1;
            let (op, id, at) = &pending[i];
            match c.job(id).ok().and_then(|j| job_outcome(op, &j)) {
                Some(Err(e)) => {
                    self.note(e.to_string());
                    pending.remove(i);
                }
                Some(Ok(_)) => {
                    pending.remove(i);
                }
                None if at.elapsed() > Duration::from_secs(600) => {
                    pending.remove(i);
                }
                None => i += 1,
            }
        }
    }

    /// Runs until the connection breaks (Ok) or the window is gone (Err).
    fn session(&self, mut c: Client) -> Result<(), ()> {
        let mut pending: Vec<(String, String, Instant)> = Vec::new();
        let mut queue_rev = u64::MAX;
        let mut next_state = Instant::now();
        let mut next_spec = Instant::now();
        loop {
            let now = Instant::now();
            if now >= next_state {
                next_state = now + STATE_EVERY;
                let Ok(snap) = self.ask(&mut c, Client::state) else {
                    return Ok(());
                };
                let queue = if snap.playlist_revision != queue_rev {
                    match self.ask(&mut c, Client::queue) {
                        Ok(q) => {
                            queue_rev = snap.playlist_revision;
                            Some(q)
                        }
                        Err(_) => None,
                    }
                } else {
                    None
                };
                let changed = {
                    let mut l = self.live.lock().unwrap();
                    let changed = !l.connected || l.snap != snap || queue.is_some();
                    l.connected = true;
                    l.snap = snap;
                    l.at = Some(Instant::now());
                    if let Some(q) = queue {
                        l.queue = q
                            .iter()
                            .map(|v| serde_json::from_value(v.clone()).unwrap_or_default())
                            .collect();
                        l.queue_raw = q;
                    }
                    changed
                };
                if changed {
                    (self.wake)();
                }
                if !pending.is_empty() {
                    self.ask(&mut c, |c| self.check_jobs(c, &mut pending));
                }
            }
            if self.want_spectrum.load(Ordering::Relaxed) && now >= next_spec {
                next_spec = now + SPECTRUM_EVERY;
                let playing = self.live.lock().unwrap().snap.playing();
                let bands = if playing {
                    self.ask(&mut c, Client::spectrum).unwrap_or_default()
                } else {
                    Vec::new()
                };
                let mut l = self.live.lock().unwrap();
                if !(bands.is_empty() && l.spectrum.is_empty()) {
                    l.spectrum = bands;
                    drop(l);
                    (self.wake)();
                }
            }

            let deadline = if self.want_spectrum.load(Ordering::Relaxed) {
                next_state.min(next_spec)
            } else {
                next_state
            };
            let wait = deadline.saturating_duration_since(Instant::now());
            match self.rx.recv_timeout(wait) {
                Ok(Cmd::Op(name, params)) => {
                    match self.ask(&mut c, |c| c.submit(&name, params)) {
                        Ok(job) => match job_outcome(&name, &job) {
                            Some(Err(e)) => self.note(e.to_string()),
                            Some(Ok(_)) => {}
                            None => {
                                let id = job["id"].as_str().unwrap_or_default().to_string();
                                pending.push((name, id, Instant::now()));
                            }
                        },
                        Err(e) => self.note(format!("{name}: {e}")),
                    }
                    // See the effect right away instead of at the next poll.
                    next_state = Instant::now();
                }
                Ok(Cmd::Query(tag, name, params)) => {
                    let result = self.ask(&mut c, |c| c.run(&name, params));
                    {
                        let mut l = self.live.lock().unwrap();
                        match result {
                            Ok(v) => l.replies.push((tag, v)),
                            Err(e) => l.errors.push(e.to_string()),
                        }
                    }
                    (self.wake)();
                }
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => return Err(()),
            }
        }
    }
}

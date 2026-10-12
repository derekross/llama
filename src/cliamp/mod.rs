//! cliamp's IPC v2: newline-delimited JSON over a local Unix socket.
//!
//! Llama never decodes audio. Every action is an `operation.submit` to
//! cliamp, and the screen is drawn from `state.get` snapshots, `queue.list`
//! and `spectrum.get` frames.

pub mod engine;

use std::io::{BufRead, BufReader, ErrorKind, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, anyhow, bail};
use serde::Deserialize;
use serde_json::{Value, json};

pub fn config_dir() -> PathBuf {
    if let Some(d) = std::env::var_os("CLIAMP_CONFIG_DIR") {
        return PathBuf::from(d);
    }
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_default();
    home.join(".config/cliamp")
}

pub fn socket_path() -> PathBuf {
    config_dir().join("cliamp.sock")
}

/// True when a cliamp is listening on the socket (a stale socket file from a
/// crashed player refuses the connection, so it does not count).
///
/// Only the connection is checked, not an answer: cliamp serves its socket
/// one request at a time, so a daemon busy with a slow provider listing can
/// leave a `state.get` unanswered for minutes while still being very much
/// alive.
pub fn is_running() -> bool {
    UnixStream::connect(socket_path()).is_ok()
}

pub struct Client {
    reader: BufReader<UnixStream>,
    writer: UnixStream,
    next_id: u64,
    patience: Duration,
    /// Set once a reply is known lost: the connection's request/response
    /// pairing can no longer be trusted, so every later call fails and the
    /// owner reconnects.
    broken: bool,
}

/// How often a read waiting on cliamp wakes to check on it.
const READ_POLL: Duration = Duration::from_secs(1);
/// How late a reply must be before cliamp is probed on a second connection.
const LOST_AFTER: Duration = Duration::from_secs(3);
/// The least time between two probes, so a cliamp that really is busy is not
/// piled up with them.
const PROBE_EVERY: Duration = Duration::from_secs(5);
/// How long a probe waits for its answer.
const PROBE_PATIENCE: Duration = Duration::from_secs(1);

/// True when cliamp answers a `state.get` on a fresh connection right away.
///
/// cliamp gives up writing a reply it cannot deliver in time (its log says
/// `ipc: write response: … i/o timeout`), which happens to a long-lived
/// connection while a big provider load runs. The request on that connection
/// then never gets an answer. A cliamp that is still busy does not answer the
/// probe either; one that answers it promptly has dropped the reply.
fn answers_now() -> bool {
    let probe = || -> Result<bool> {
        let stream = UnixStream::connect(socket_path())?;
        stream.set_read_timeout(Some(PROBE_PATIENCE))?;
        stream.set_write_timeout(Some(PROBE_PATIENCE))?;
        (&stream)
            .write_all(b"{\"version\":2,\"id\":\"llama-probe\",\"method\":\"state.get\"}\n")?;
        let mut line = String::new();
        BufReader::new(&stream).read_line(&mut line)?;
        let resp: Value = serde_json::from_str(&line)?;
        Ok(resp.get("ok").and_then(Value::as_bool) == Some(true))
    };
    probe().unwrap_or(false)
}

impl Client {
    pub fn connect() -> Result<Client> {
        Client::connect_waiting(Duration::from_secs(10))
    }

    /// Connects with a read timeout of `patience`. cliamp answers nothing
    /// while a provider job runs (listing a large YouTube playlist can take
    /// minutes), so a long-lived connection should wait that out rather than
    /// mistake it for a dead daemon.
    pub fn connect_waiting(patience: Duration) -> Result<Client> {
        let path = socket_path();
        let stream = UnixStream::connect(&path)
            .with_context(|| format!("connecting to {}", path.display()))?;
        // Reads wake every READ_POLL; `call` waits out `patience` itself.
        stream.set_read_timeout(Some(READ_POLL.min(patience)))?;
        let writer = stream.try_clone()?;
        Ok(Client {
            reader: BufReader::new(stream),
            writer,
            next_id: 0,
            patience,
            broken: false,
        })
    }

    fn call(&mut self, mut req: Value) -> Result<Value> {
        if self.broken {
            bail!("cliamp dropped a reply; reconnecting");
        }
        self.next_id += 1;
        let id = format!("llama-{}", self.next_id);
        req["version"] = json!(2);
        req["id"] = json!(id);
        let mut line = serde_json::to_vec(&req)?;
        line.push(b'\n');
        self.writer.write_all(&line)?;
        let sent = Instant::now();
        let mut probed: Option<Instant> = None;
        // Kept across read timeouts: a timed-out read keeps what it got.
        let mut buf = Vec::new();
        loop {
            match self.reader.read_until(b'\n', &mut buf) {
                Ok(0) => bail!("cliamp closed the connection"),
                Ok(_) => {}
                Err(e) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {
                    let late = sent.elapsed();
                    if late >= self.patience {
                        bail!("cliamp did not answer in {}s", self.patience.as_secs());
                    }
                    if late >= LOST_AFTER && probed.is_none_or(|t| t.elapsed() >= PROBE_EVERY) {
                        probed = Some(Instant::now());
                        if answers_now() {
                            self.broken = true;
                            bail!("cliamp dropped a reply; reconnecting");
                        }
                    }
                    continue;
                }
                Err(e) => return Err(e.into()),
            }
            let resp: Value = serde_json::from_slice(&buf).context("bad JSON from cliamp")?;
            buf.clear();
            // Responses come back in order on a request connection, but be
            // strict anyway: skip anything that is not ours.
            if resp.get("id").and_then(Value::as_str) != Some(id.as_str()) {
                continue;
            }
            if resp.get("ok").and_then(Value::as_bool) != Some(true) {
                let err = resp.get("error").cloned().unwrap_or(Value::Null);
                bail!("cliamp: {err}");
            }
            return Ok(resp);
        }
    }

    pub fn state(&mut self) -> Result<Snapshot> {
        let resp = self.call(json!({"method": "state.get"}))?;
        Ok(serde_json::from_value(resp["snapshot"].clone())?)
    }

    pub fn spectrum(&mut self) -> Result<Vec<f32>> {
        let resp = self.call(json!({"method": "spectrum.get"}))?;
        Ok(serde_json::from_value(resp["result"]["bands"].clone()).unwrap_or_default())
    }

    /// Submits an operation without waiting; returns the job as submitted
    /// (it may already have finished).
    pub fn submit(&mut self, op: &str, params: Value) -> Result<Value> {
        let resp =
            self.call(json!({"method": "operation.submit", "operation": op, "params": params}))?;
        let job = resp["job"].clone();
        if job["id"].as_str().is_none() {
            bail!("no job id");
        }
        Ok(job)
    }

    /// Submits an operation and polls its job until it finishes, returning
    /// the job's `result`.
    pub fn run(&mut self, op: &str, params: Value) -> Result<Value> {
        let resp =
            self.call(json!({"method": "operation.submit", "operation": op, "params": params}))?;
        let mut job = resp["job"].clone();
        let id = job["id"]
            .as_str()
            .map(str::to_string)
            .ok_or_else(|| anyhow!("no job id"))?;
        // Poll quickly at first, then back off: a provider listing can run
        // for minutes, and the deadline matches the engine's patience.
        let deadline = std::time::Instant::now() + Duration::from_secs(600);
        let mut wait = Duration::from_millis(50);
        loop {
            if let Some(r) = job_outcome(op, &job) {
                return r;
            }
            if std::time::Instant::now() >= deadline {
                bail!("{op} timed out");
            }
            std::thread::sleep(wait);
            wait = (wait * 2).min(Duration::from_millis(500));
            job = self.job(&id)?;
        }
    }

    /// A job's current record.
    pub fn job(&mut self, id: &str) -> Result<Value> {
        Ok(self.call(json!({"method": "job.get", "job_id": id}))?["job"].clone())
    }

    /// The live playlist, as cliamp's own track objects (they carry provider
    /// identity that must survive a round trip, e.g. into a saved playlist).
    pub fn queue(&mut self) -> Result<Vec<Value>> {
        let mut out = Vec::new();
        loop {
            let res = self.run("queue.list", json!({"offset": out.len(), "limit": 500}))?;
            let page: Vec<Value> = res["tracks"].as_array().cloned().unwrap_or_default();
            let total = res["total"].as_u64().map(|t| t as usize);
            let n = page.len();
            out.extend(page);
            // Without a total, a short page is the last one.
            let done = match total {
                Some(t) => out.len() >= t,
                None => n < 500,
            };
            if n == 0 || done {
                return Ok(out);
            }
        }
    }
}

/// `Some` once a job has finished: its result, or why it failed.
pub fn job_outcome(op: &str, job: &Value) -> Option<Result<Value>> {
    match job["state"].as_str() {
        Some("succeeded") => Some(Ok(job["result"].clone())),
        Some("failed") | Some("canceled") => {
            let err = &job["error"];
            let msg = err["message"]
                .as_str()
                .or(err.as_str())
                .map(str::to_string)
                .unwrap_or_else(|| err.to_string());
            Some(Err(anyhow!("{op}: {msg}")))
        }
        _ => None,
    }
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct Track {
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub artist: String,
    #[serde(default)]
    pub album: String,
    #[serde(default)]
    pub path: String,
    #[serde(default)]
    pub duration: Option<f64>,
    #[serde(default)]
    pub station: Option<String>,
}

impl Track {
    /// "Artist - Title", the way Winamp named things.
    pub fn display(&self) -> String {
        let title = if self.title.is_empty() {
            self.path.rsplit('/').next().unwrap_or("")
        } else {
            &self.title
        };
        match (&self.station, self.artist.is_empty()) {
            (Some(st), true) if st != title => format!("{st}: {title}"),
            (Some(st), false) => format!("{st}: {} - {title}", self.artist),
            (_, false) => format!("{} - {title}", self.artist),
            _ => title.to_string(),
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct Snapshot {
    #[serde(default)]
    pub revision: u64,
    #[serde(default)]
    pub playlist_revision: u64,
    #[serde(default)]
    pub state: String,
    #[serde(default)]
    pub track: Option<Track>,
    #[serde(default)]
    pub logical_track: Option<Track>,
    #[serde(default)]
    pub position: f64,
    #[serde(default)]
    pub duration: f64,
    #[serde(default)]
    pub seekable: bool,
    /// dB, -30..=+6. Absent when cliamp has not reported it yet.
    #[serde(default)]
    pub volume: Option<f64>,
    /// Omitted by cliamp when it is 0; -1 when the playlist is empty.
    #[serde(default)]
    pub index: i64,
    #[serde(default)]
    pub total: usize,
    #[serde(default)]
    pub shuffle: bool,
    #[serde(default)]
    pub repeat: String,
    #[serde(default)]
    pub mono: bool,
    #[serde(default)]
    pub eq_preset: String,
    #[serde(default)]
    pub eq_bands: Vec<f64>,
    #[serde(default)]
    pub playlist: Option<String>,
    #[serde(default)]
    pub stream_error: Option<String>,
}

impl Snapshot {
    pub fn playing(&self) -> bool {
        self.state == "playing"
    }

    pub fn paused(&self) -> bool {
        self.state == "paused"
    }

    pub fn now(&self) -> Option<&Track> {
        self.track.as_ref().or(self.logical_track.as_ref())
    }

    /// The playing row of the live playlist.
    pub fn current(&self) -> Option<usize> {
        (self.index >= 0 && self.total > 0).then_some(self.index as usize)
    }

    pub fn repeat_on(&self) -> bool {
        !self.repeat.is_empty() && !self.repeat.eq_ignore_ascii_case("off")
    }
}

pub const VOL_MIN_DB: f64 = -30.0;
pub const VOL_MAX_DB: f64 = 6.0;

/// cliamp's dB volume as Winamp's 0..=1 slider.
pub fn volume_ratio(db: f64) -> f64 {
    ((db - VOL_MIN_DB) / (VOL_MAX_DB - VOL_MIN_DB)).clamp(0.0, 1.0)
}

pub fn volume_db(ratio: f64) -> f64 {
    let db = VOL_MIN_DB + ratio.clamp(0.0, 1.0) * (VOL_MAX_DB - VOL_MIN_DB);
    (db * 2.0).round() / 2.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_tolerates_omitted_fields() {
        let s: Snapshot = serde_json::from_str(
            r#"{"revision":3,"state":"playing","track":{"title":"Song","path":"/m/a.mp3"},"duration":152.6,"seekable":true,"volume":-30,"total":3,"repeat":"Off","eq_bands":[0,0,0,0,0,0,0,0,0,0]}"#,
        )
        .unwrap();
        assert!(s.playing());
        assert_eq!(s.current(), Some(0));
        let empty: Snapshot =
            serde_json::from_str(r#"{"state":"stopped","index":-1,"total":0}"#).unwrap();
        assert_eq!(empty.current(), None);
        assert_eq!(s.position, 0.0);
        assert!(!s.repeat_on());
        assert_eq!(s.now().unwrap().display(), "Song");
    }

    #[test]
    fn display_names() {
        let t = Track {
            title: "T".into(),
            artist: "A".into(),
            ..Default::default()
        };
        assert_eq!(t.display(), "A - T");
        let r = Track {
            title: "Song".into(),
            station: Some("SomaFM".into()),
            ..Default::default()
        };
        assert_eq!(r.display(), "SomaFM: Song");
        let f = Track {
            path: "/x/y.mp3".into(),
            ..Default::default()
        };
        assert_eq!(f.display(), "y.mp3");
    }

    #[test]
    fn volume_round_trip() {
        assert_eq!(volume_ratio(-30.0), 0.0);
        assert_eq!(volume_ratio(6.0), 1.0);
        assert_eq!(volume_db(0.5), -12.0);
        assert_eq!(volume_db(volume_ratio(-3.0)), -3.0);
    }
}

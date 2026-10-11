.pragma library

// Parsing and formatting for the Llama panel. Pure functions, no QML.

function parse(text) {
  try { return JSON.parse(String(text || "").trim()) } catch (e) { return null }
}

// `cliamp remote call ... --wait` prints a v2 response; the payload is the
// finished job's result.
function jobResult(text) {
  var r = parse(text)
  if (!r || !r.job) return null
  if (r.job.state !== "succeeded") return null
  return r.job.result || null
}

function jobError(text) {
  var r = parse(text)
  if (!r) return "cliamp is not running"
  if (r.ok === false) return String(r.error && r.error.message || r.error || "cliamp error")
  if (r.job && r.job.state === "failed") return String(r.job.error && r.job.error.message || r.job.error || "failed")
  return ""
}

function clock(secs) {
  var s = Math.max(0, Math.floor(Number(secs) || 0))
  var h = Math.floor(s / 3600), m = Math.floor(s / 60) % 60, r = s % 60
  var pad = function(n) { return n < 10 ? "0" + n : String(n) }
  return h > 0 ? h + ":" + pad(m) + ":" + pad(r) : m + ":" + pad(r)
}

function trackTitle(t) {
  if (!t) return ""
  if (t.title) return String(t.title)
  var p = String(t.path || "")
  return p.substring(p.lastIndexOf("/") + 1)
}

function trackArtist(t) {
  if (!t) return ""
  if (t.station && t.artist) return t.station + " · " + t.artist
  return String(t.artist || t.station || t.album || "")
}

// The status of `cliamp status --json`, normalised.
function status(text) {
  var s = parse(text)
  if (!s || s.ok === false) return null
  var t = s.track || {}
  return {
    state: String(s.state || "stopped"),
    title: trackTitle(t),
    artist: trackArtist(t),
    position: Number(s.position) || 0,
    duration: Number(s.duration) || 0,
    volume: s.volume === undefined ? 0 : Number(s.volume),
    shuffle: s.shuffle === true,
    repeat: String(s.repeat || "Off"),
    index: Number(s.index) || 0,
    total: Number(s.total) || 0
  }
}

function repeatOn(r) {
  return r !== "" && String(r).toLowerCase() !== "off"
}

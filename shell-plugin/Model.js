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

// A bar label: long titles would stretch the bar without limit.
function shortTitle(t, max) {
  var s = String(t || "")
  var n = max || 30
  return s.length > n ? s.substring(0, n - 1).trim() + "…" : s
}

function isMd5(s) {
  return /^[0-9a-f]{32}$/i.test(String(s || ""))
}

// The Museum listing is remote data: only a real md5 may reach `llama skin
// use` (which also takes paths and links), and only the Museum's own image
// host may reach Image.source.
function validSkins(list) {
  if (!Array.isArray(list)) return []
  return list.filter(function(s) {
    return s && isMd5(s.md5)
      && typeof s.screenshot === "string"
      && s.screenshot.indexOf("https://r2.webampskins.org/") === 0
  })
}

// `llama skin featured|search` failures, from its stderr ("llama: ...").
function skinError(err) {
  var e = String(err || "").trim().replace(/^llama:\s*/, "")
  if (e === "") return "The Skin Museum is unreachable"
  if (/unreachable/i.test(e)) return "The Skin Museum is unreachable"
  if (/^Skin Museum:/.test(e)) return "The Museum could not run that search; try other words"
  return e.split("\n")[0]
}

function firstLine(text) {
  return String(text || "").trim().replace(/^llama:\s*/, "").split("\n")[0]
}

// A `remote call --wait` that finished without error, result or not.
function callOk(text) {
  var r = parse(text)
  if (!r || r.ok === false) return false
  return !r.job || r.job.state === "succeeded"
}

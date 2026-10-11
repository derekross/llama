import QtQuick
import QtQuick.Controls
import Quickshell
import Quickshell.Io
import qs.Commons
import qs.Ui
import "Model.js" as Model

// The Llama dropdown: a mini player, cliamp's sources (search and play
// from Spotify, Tidal, YouTube Music, Plex, radio...), and the Winamp Skin
// Museum for the player window. cliamp does the work; this is a remote.
Panel {
  id: root
  moduleName: "derekross.llama"
  ipcTarget: "derekross.llama"
  manageIpc: false

  property var anchorItem: null
  property var hostWidget: null
  readonly property var barIdentity: hostWidget || root
  // One bar instance per monitor; only the first polls cliamp while closed,
  // and the others read its state (see BarWidget.isLead).
  readonly property bool lead: !hostWidget || hostWidget.isLead !== false
  readonly property color foreground: bar ? bar.foreground : Color.foreground
  readonly property color dim: Qt.darker(foreground, 1.55)
  readonly property color accent: Color.accent
  readonly property string fontFamily: bar ? bar.fontFamily : Style.font.family
  readonly property string home: Quickshell.env("HOME")
  readonly property string llamaBin: home + "/.local/bin/llama"
  readonly property int panelWidth: Style.space(420)

  // The plugin ships dist/install.sh next to shell-plugin/; a marketplace
  // install has the plugin but not the binary until that has run.
  readonly property string installScript: {
    var p = decodeURIComponent(String(Qt.resolvedUrl("../dist/install.sh")).replace(/^file:\/\//, ""))
    return home !== "" && p.indexOf(home + "/") === 0 ? "~" + p.substring(home.length) : p
  }
  readonly property string installText: "Run " + installScript + " to install the player"
  readonly property string cliampMissingText: "cliamp is not installed. Llama plays through it: pacman -S cliamp"

  // Assume both are there until the probe says otherwise, so nothing flashes.
  property bool llamaInstalled: true
  property bool cliampInstalled: true
  property bool probed: false

  property string tab: "now"
  property var now: null
  property bool connected: false
  property string nowTitle: now && now.title ? now.title : ""
  readonly property bool playing: connected && now !== null && now.state === "playing"
  property real seekDrag: -1
  property real volumeDrag: -100

  property var providers: []
  property string provider: ""
  property var results: []
  property string resultsKind: "tracks"
  property string libraryStatus: ""
  readonly property var providerInfo: {
    for (var i = 0; i < providers.length; i++) if (providers[i] && providers[i].key === provider) return providers[i]
    return null
  }
  readonly property string providerName: providerInfo && providerInfo.name ? String(providerInfo.name) : provider
  readonly property bool providerSearchable: !providerInfo || providerInfo.searchable !== false

  property var skins: []
  property int skinPage: 0
  property bool skinsExhausted: false
  property string skinQuery: ""
  property string skinStatus: ""
  property var currentSkin: null
  property string playerStatus: ""

  // In-flight requests. Each one is its own Process (see requestComponent),
  // and each family keeps a generation number: a newer request cancels the
  // older one, and whatever an older one still reports is dropped.
  property var statusReq: null
  property var providersReq: null
  property var libraryReq: null
  property int libraryGen: 0
  property var playReq: null
  property int playGen: 0
  property var skinsReq: null
  property int skinsGen: 0
  property var useReq: null
  property int useGen: 0
  property var currentReq: null

  // Keyboard cursor: "providers", "results" or "skins".
  property bool cursorActive: false
  property string cursorSection: ""
  property int cursorIndex: 0

  function open() {
    root.controller.show()
    probe(function() {
      root.refreshStatus()
      if (root.tab === "library" && root.providers.length === 0) root.loadProviders()
      if (root.tab === "skins") root.ensureSkins()
    })
  }
  function close() { root.controller.hide() }
  function showSkins() {
    setTab("skins")
    if (!root.opened) open()
  }
  function toggle() { root.opened ? close() : open() }
  // The bar knows the widget (hostWidget), not this nested panel.
  function switchPanel(direction) {
    if (root.bar && typeof root.bar.switchPanelFrom === "function")
      return root.bar.switchPanelFrom(root.barIdentity, direction)
    return false
  }
  function setTab(t) {
    tab = t
    cursorActive = false
    cursorSection = ""
    cursorIndex = 0
    // A TextField may hold focus; Esc, Tab and the keys below need it back.
    keyCatcher.forceActiveFocus()
    if (t === "library" && providers.length === 0) loadProviders()
    if (t === "skins") ensureSkins()
  }

  // ---- requests ---------------------------------------------------------------

  // Runs `command` in a Process of its own and calls back once with
  // { code, out, err, timedOut, started }. code is -1 when it never exited
  // (failed to start, or killed by the timeout).
  function run(command, timeoutMs, callback) {
    var req = requestComponent.createObject(root, { command: command, timeoutMs: timeoutMs || 30000, callback: callback })
    if (!req) {
      callback({ code: -1, out: "", err: "", timedOut: false, started: false })
      return null
    }
    req.watchdog.start()
    req.running = true
    return req
  }

  function cliampCall(op, params) {
    var cmd = ["cliamp", "remote", "call", op, "--wait"]
    return params ? cmd.concat(["--params", JSON.stringify(params)]) : cmd
  }

  function cliampError(r, fallback) {
    if (!root.cliampInstalled) return root.cliampMissingText
    if (r.timedOut) return "cliamp took too long to answer"
    var e = Model.parse(r.out) ? Model.jobError(r.out) : ""
    return e || fallback
  }

  // Is the llama binary installed, and is cliamp on PATH?
  function probe(then) {
    run(["sh", "-c", 'test -x "$1" && echo llama; command -v cliamp >/dev/null 2>&1 && echo cliamp; exit 0', "sh", llamaBin], 5000, function(r) {
      if (r.code === 0) {
        var words = String(r.out).split(/\s+/)
        root.llamaInstalled = words.indexOf("llama") >= 0
        root.cliampInstalled = words.indexOf("cliamp") >= 0
        root.probed = true
      }
      if (then) then()
    })
  }

  Component.onCompleted: probe(null)

  function notify(title, body) {
    Quickshell.execDetached(["sh", "-c",
      'if command -v omarchy-notification-send >/dev/null 2>&1; then exec omarchy-notification-send -g 󰎈 "$1" "$2"; else exec notify-send "$1" "$2"; fi',
      "sh", title, body])
  }

  // Opens (or shows) the player window. From the bar, a missing binary is a
  // notification; from the panel, it shows on the Now Playing tab.
  function launchPlayer(fromPanel) {
    probe(function() {
      if (root.llamaInstalled) {
        root.playerStatus = ""
        Quickshell.execDetached([root.llamaBin])
        if (fromPanel) root.close()
      } else if (fromPanel) {
        root.playerStatus = root.installText
      } else {
        root.notify("The Llama player is not installed", root.installText)
      }
    })
  }

  // Fire-and-forget cliamp commands; the status poll shows their effect.
  function cliamp(args) {
    Quickshell.execDetached(["cliamp"].concat(args))
    statusSoon.restart()
  }
  function remote(op, params) {
    cliamp(["remote", "call", op, "--params", JSON.stringify(params || {})])
  }
  function volumeStep(dir) { remote("volume.adjust", { value: dir }) }

  function refreshStatus() {
    if (statusReq) return
    if (probed && !cliampInstalled) { connected = false; now = null; return }
    statusReq = run(["cliamp", "status", "--json"], 5000, function(r) {
      root.statusReq = null
      // A busy cliamp (a big playlist loading) answers late: keep what we had.
      if (r.timedOut) return
      var s = r.code === 0 ? Model.status(r.out) : null
      root.connected = s !== null
      root.now = s
    })
  }

  function loadProviders() {
    if (!cliampInstalled) { libraryStatus = cliampMissingText; return }
    if (providersReq) return
    libraryStatus = "Loading sources…"
    providersReq = run(cliampCall("provider.list", null), 20000, function(r) {
      root.providersReq = null
      var res = Model.jobResult(r.out)
      if (!res) { root.libraryStatus = root.cliampError(r, "cliamp is not running — open Llama first"); return }
      root.providers = res.providers || []
      root.libraryStatus = root.providers.length === 0 ? "cliamp has no sources set up" : ""
      if (root.provider === "" && root.providers.length > 0) root.selectProvider(root.providers[0].key)
    })
  }

  // One library request at a time: a newer one (another search, another
  // provider) replaces it. kind and provider are the request's own.
  function libraryRequest(op, params, kind, status, timeoutMs) {
    if (libraryReq) { libraryReq.cancel(); libraryReq = null }
    libraryGen += 1
    var gen = libraryGen
    var prov = provider
    libraryStatus = status
    libraryReq = run(cliampCall(op, params), timeoutMs, function(r) {
      if (gen !== root.libraryGen || prov !== root.provider) return
      root.libraryReq = null
      var res = Model.jobResult(r.out)
      root.resultsKind = kind
      if (!res) { root.results = []; root.libraryStatus = root.cliampError(r, "Nothing came back"); return }
      root.results = kind === "playlists" ? (res.playlists || []) : (res.tracks || [])
      root.libraryStatus = root.results.length === 0 ? "Nothing here" : ""
      if (root.cursorSection === "results") root.cursorIndex = 0
    })
  }

  function selectProvider(key) {
    if (libraryReq) { libraryReq.cancel(); libraryReq = null }
    provider = key
    results = []
    libraryStatus = ""
    searchField.text = ""
    browsePlaylists()
  }
  function search(text) {
    var q = String(text).trim()
    if (provider === "" || q === "" || !providerSearchable) return
    libraryRequest("provider.search", { provider: provider, query: q, limit: 50 }, "tracks", "Searching…", 60000)
  }
  function browsePlaylists() {
    if (provider === "") return
    libraryRequest("provider.playlists", { provider: provider, limit: 100 }, "playlists", "Loading playlists…", 120000)
  }
  function openPlaylist(item) {
    // cliamp lists the whole playlist (a big YouTube one takes minutes) and
    // answers nothing else meanwhile, Stop included.
    libraryRequest("provider.tracks", { provider: provider, playlist: item.id, limit: 200 }, "tracks",
      "Loading " + item.name + "… A big playlist can take minutes, and playback controls wait until it is done.",
      600000)
  }
  // Load, and play once the load is done (not alongside it).
  function playPlaylist(item) {
    if (playReq) { playReq.cancel(); playReq = null }
    playGen += 1
    var gen = playGen
    var msg = "Loading " + item.name + " to play it… A big playlist can take minutes."
    libraryStatus = msg
    playReq = run(cliampCall("provider.load", { provider: provider, playlist: item.id }), 600000, function(r) {
      if (gen !== root.playGen) return
      root.playReq = null
      if (r.code === 0 && !r.timedOut && Model.callOk(r.out)) {
        root.remote("play", {})
        if (root.libraryStatus === msg) root.libraryStatus = ""
      } else {
        root.libraryStatus = root.cliampError(r, "That playlist would not load")
      }
    })
  }
  function playResult(item, alternate) {
    if (!item) return
    if (resultsKind === "playlists") {
      if (alternate) playPlaylist(item)
      else openPlaylist(item)
    } else if (alternate) {
      remote("track.queue", { track: item })
    } else {
      remote("track.play", { track: item })
    }
  }

  function ensureSkins() {
    if (!llamaInstalled) { skinStatus = installText; return }
    if (skins.length === 0 && !skinsReq) loadSkins(false)
    if (!currentReq) {
      currentReq = run([llamaBin, "skin", "current"], 10000, function(r) {
        root.currentReq = null
        var res = r.code === 0 ? Model.parse(r.out) : null
        if (res) root.currentSkin = res
      })
    }
  }
  function loadSkins(more) {
    if (!llamaInstalled) { skinStatus = installText; return }
    if (more) {
      if (skinsReq || skinsExhausted || skins.length === 0) return
    } else {
      if (skinsReq) { skinsReq.cancel(); skinsReq = null }
      skinsExhausted = false
    }
    skinsGen += 1
    var gen = skinsGen
    var page = more ? skinPage + 1 : 0
    var query = skinQuery
    skinStatus = more ? "Loading more…" : (query === "" ? "Loading the Museum's favorites…" : "Searching ~100,000 skins…")
    var cmd = query === ""
      ? [llamaBin, "skin", "featured", "--page", String(page)]
      : [llamaBin, "skin", "search", query, "--page", String(page)]
    skinsReq = run(cmd, 60000, function(r) {
      if (gen !== root.skinsGen) return
      root.skinsReq = null
      var res = r.code === 0 ? Model.parse(r.out) : null
      if (!res || !Array.isArray(res.skins)) {
        root.skinStatus = r.timedOut ? "The Skin Museum took too long to answer" : Model.skinError(r.err)
        return
      }
      // The page only counts once it has arrived.
      root.skinPage = page
      if (res.skins.length === 0) root.skinsExhausted = true
      var list = Model.validSkins(res.skins)
      root.skins = more ? root.skins.concat(list) : list
      root.skinStatus = root.skins.length > 0 ? ""
        : (query === "" ? "No skins found" : "No skins match “" + query + "”")
      if (!more && root.cursorSection === "skins") root.cursorIndex = 0
    })
  }
  // The newest click wins: a second skin replaces one still downloading.
  function skinCommand(args, status) {
    if (!llamaInstalled) { skinStatus = installText; return }
    if (useReq) { useReq.cancel(); useReq = null }
    useGen += 1
    var gen = useGen
    skinStatus = status
    useReq = run([llamaBin, "skin"].concat(args), 90000, function(r) {
      if (gen !== root.useGen) return
      root.useReq = null
      var res = r.code === 0 ? Model.parse(r.out) : null
      if (!res) {
        var why = Model.firstLine(r.err)
        root.skinStatus = r.timedOut ? "That skin took too long to load" : "That skin would not load" + (why !== "" ? ": " + why : "")
        return
      }
      root.currentSkin = res
      root.skinStatus = !res.running ? "Worn — right-click the bar icon to open Llama" : ""
    })
  }
  function wearSkin(md5) {
    if (!Model.isMd5(md5)) return
    skinCommand(["use", String(md5).toLowerCase()], "Dressing the llama…")
  }

  // ---- keyboard ---------------------------------------------------------------

  function focusSearch() {
    if (tab === "library" && provider !== "" && providerSearchable) searchField.forceActiveFocus()
    else if (tab === "skins" && llamaInstalled) skinField.forceActiveFocus()
  }

  function cursorTo(section, index) {
    cursorActive = true
    cursorSection = section
    var n = section === "providers" ? providers.length : (section === "results" ? results.length : skins.length)
    cursorIndex = Math.max(0, Math.min(index, n - 1))
    if (section === "results") resultList.positionViewAtIndex(cursorIndex, ListView.Contain)
    else if (section === "skins") skinGrid.positionViewAtIndex(cursorIndex, GridView.Contain)
  }

  function providerIndex() {
    for (var i = 0; i < providers.length; i++) if (providers[i].key === provider) return i
    return 0
  }

  function moveCursor(dx, dy) {
    if (tab === "now") {
      if (dx !== 0 && connected) volumeStep(dx)
      return
    }
    if (tab === "library") {
      if (!cursorActive || cursorSection === "") {
        if (results.length > 0) cursorTo("results", 0)
        else if (providers.length > 0) cursorTo("providers", providerIndex())
        return
      }
      if (cursorSection === "providers") {
        if (dx !== 0) cursorTo("providers", cursorIndex + dx)
        else if (dy > 0 && results.length > 0) cursorTo("results", 0)
      } else if (cursorSection === "results") {
        if (dy < 0 && cursorIndex === 0) { if (providers.length > 0) cursorTo("providers", providerIndex()) }
        else if (dy !== 0) cursorTo("results", cursorIndex + dy)
      }
      return
    }
    if (tab === "skins") {
      if (skins.length === 0) return
      if (!cursorActive || cursorSection !== "skins") { cursorTo("skins", 0); return }
      cursorTo("skins", cursorIndex + dx + dy * 2)
    }
  }

  function activateCursor(alternate) {
    if (tab === "now") { if (!alternate) cliamp(["toggle"]); return }
    if (!cursorActive) return
    if (cursorSection === "providers" && cursorIndex < providers.length) {
      selectProvider(providers[cursorIndex].key)
    } else if (cursorSection === "results" && cursorIndex < results.length) {
      playResult(results[cursorIndex], alternate)
    } else if (cursorSection === "skins" && cursorIndex < skins.length && !alternate) {
      wearSkin(skins[cursorIndex].md5)
    }
  }

  // ---- status ---------------------------------------------------------------

  // Polls while the panel is open, and in the background on the lead bar
  // only (the bar dims the note when nothing plays). Positions are not
  // pushed by cliamp, so an open panel still needs a poll for the clock.
  Timer {
    interval: root.opened ? 1000 : 4000
    running: root.opened || root.lead
    repeat: true
    triggeredOnStart: true
    onTriggered: root.refreshStatus()
  }

  Timer {
    id: statusSoon
    interval: 250
    onTriggered: root.refreshStatus()
  }

  Component {
    id: requestComponent

    Process {
      id: request
      property var callback: null
      property int timeoutMs: 30000
      property string out: ""
      property string err: ""
      property bool outDone: false
      property bool errDone: false
      property bool exitSeen: false
      property bool didStart: false
      property int code: -1

      property Timer watchdog: Timer {
        interval: request.timeoutMs
        onTriggered: request.finish(true)
      }
      // Covers a process that never started (no exited signal) and streams
      // that close after the exit.
      property Timer settle: Timer {
        interval: 200
        onTriggered: request.finish(false)
      }

      function finish(timedOut) {
        watchdog.stop()
        settle.stop()
        var cb = callback
        if (!cb) return
        callback = null
        if (running) running = false
        var result = { code: exitSeen ? code : -1, out: out, err: err, timedOut: timedOut === true, started: didStart }
        try {
          cb(result)
        } finally {
          request.destroy()
        }
      }
      function cancel() {
        callback = null
        watchdog.stop()
        settle.stop()
        running = false
        request.destroy()
      }
      function check() { if (exitSeen && outDone && errDone) finish(false) }

      stdout: StdioCollector {
        waitForEnd: true
        onStreamFinished: { request.out = text; request.outDone = true; request.check() }
      }
      stderr: StdioCollector {
        waitForEnd: true
        onStreamFinished: { request.err = text; request.errDone = true; request.check() }
      }
      onStarted: didStart = true
      onExited: function(exitCode, exitStatus) {
        code = exitCode
        exitSeen = true
        check()
        if (callback) settle.restart()
      }
      onRunningChanged: if (!running && callback && !settle.running) settle.restart()
    }
  }

  // ---- the panel ------------------------------------------------------------

  component Label: Text {
    textFormat: Text.PlainText
    color: root.foreground
    font.family: root.fontFamily
    font.pixelSize: Style.font.body
    elide: Text.ElideRight
  }

  component Hint: Label {
    width: parent ? parent.width : 0
    color: root.dim
    font.pixelSize: Style.font.caption
    wrapMode: Text.Wrap
    elide: Text.ElideNone
  }

  component TabButton: Button {
    property string key: ""
    foreground: root.foreground
    accent: root.accent
    selected: root.tab === key
    fontFamily: root.fontFamily
    fontSize: Style.font.bodySmall
    onClicked: root.setTab(key)
  }

  KeyboardPanel {
    id: panel
    anchorItem: root.anchorItem
    owner: root.barIdentity
    bar: root.bar
    open: root.opened
    centerOnBar: false
    focusTarget: keyCatcher
    contentWidth: panel.fittedContentWidth(
      root.panelWidth + panel.padding * 2 + Border.left(panel.borderSpec) + Border.right(panel.borderSpec)
    )
    contentHeight: panel.fittedContentHeight(column.implicitHeight, Style.space(640))

    PanelKeyCatcher {
      id: keyCatcher
      anchors.fill: parent
      blocked: searchField.activeFocus || skinField.activeFocus
      onCloseRequested: root.close()
      // Tab moves between bar panels, as in every built-in panel; the
      // number keys switch tabs here.
      onTabRequested: function(direction) { root.switchPanel(direction) }
      onMoveRequested: function(dx, dy) { root.moveCursor(dx, dy) }
      // Space and Enter both arrive here (PanelKeyCatcher never passes
      // Space on as text).
      onActivateRequested: root.activateCursor(false)
      onTextKey: function(t) {
        if (t === "1") root.setTab("now")
        else if (t === "2") root.setTab("library")
        else if (t === "3") root.setTab("skins")
        else if (t === "/") root.focusSearch()
        else if (root.tab === "now") {
          if (t === "b") root.cliamp(["next"])
          else if (t === "z") root.cliamp(["prev"])
          else if (t === "v") root.cliamp(["stop"])
          else if (t === "o") root.launchPlayer(true)
        } else if (root.tab === "library") {
          if (t === "a") root.activateCursor(true)
          else if (t === "p") root.browsePlaylists()
        } else if (root.tab === "skins") {
          if (t === "0") root.skinCommand(["base"], "Back to the base skin…")
        }
      }

      Flickable {
        id: scroll
        anchors.fill: parent
        contentWidth: width
        contentHeight: column.implicitHeight
        clip: true
        boundsBehavior: Flickable.StopAtBounds
        interactive: contentHeight > height
        ScrollBar.vertical: ScrollBar { policy: scroll.contentHeight > scroll.height ? ScrollBar.AsNeeded : ScrollBar.AlwaysOff }

      Column {
        id: column
        width: scroll.width
        spacing: Style.space(10)

        Row {
          spacing: Style.space(6)
          TabButton { key: "now"; text: "Now Playing"; tooltipText: "1" }
          TabButton { key: "library"; text: "Library"; tooltipText: "2" }
          TabButton { key: "skins"; text: "Skins"; tooltipText: "3" }
        }

        PanelSeparator { width: parent.width; foreground: root.foreground }

        // ---- Now Playing ------------------------------------------------
        Column {
          visible: root.tab === "now"
          width: parent.width
          spacing: Style.space(10)

          Label {
            width: parent.width
            text: !root.cliampInstalled ? "cliamp is not installed"
              : (!root.connected ? "cliamp is not running" : (root.nowTitle !== "" ? root.nowTitle : "Nothing playing"))
            font.pixelSize: Style.font.heading
            font.bold: true
          }
          Label {
            width: parent.width
            visible: text !== ""
            text: !root.cliampInstalled ? "Llama plays through it: pacman -S cliamp" : (root.now ? root.now.artist : "")
            color: root.dim
            wrapMode: Text.Wrap
          }

          Row {
            width: parent.width
            spacing: Style.space(8)
            visible: root.connected && root.now && root.now.duration > 0
            Label { id: posLabel; text: Model.clock(root.seekDrag >= 0 ? root.seekDrag : (root.now ? root.now.position : 0)); color: root.dim; font.pixelSize: Style.font.caption; anchors.verticalCenter: parent.verticalCenter }
            PanelSlider {
              width: parent.width - posLabel.width - durLabel.width - parent.spacing * 2
              anchors.verticalCenter: parent.verticalCenter
              bar: root.bar
              minimum: 0
              maximum: root.now ? Math.max(1, root.now.duration) : 1
              step: 1
              value: root.now ? root.now.position : 0
              onMoved: function(v) { root.seekDrag = v }
              onReleased: function(v) { root.seekDrag = -1; root.remote("seek.absolute", { value: Math.round(v) }) }
            }
            Label { id: durLabel; text: Model.clock(root.now ? root.now.duration : 0); color: root.dim; font.pixelSize: Style.font.caption; anchors.verticalCenter: parent.verticalCenter }
          }

          Row {
            spacing: Style.space(6)
            visible: root.connected
            Button { iconText: "󰒮"; foreground: root.foreground; tooltipText: "Previous (z)"; onClicked: root.cliamp(["prev"]) }
            Button { iconText: root.playing ? "󰏤" : "󰐊"; foreground: root.foreground; tooltipText: "Play/pause (Space)"; onClicked: root.cliamp(["toggle"]) }
            Button { iconText: "󰓛"; foreground: root.foreground; tooltipText: "Stop (v)"; onClicked: root.cliamp(["stop"]) }
            Button { iconText: "󰒭"; foreground: root.foreground; tooltipText: "Next (b)"; onClicked: root.cliamp(["next"]) }
            Button {
              iconText: "󰒟"; foreground: root.foreground; accent: root.accent
              selected: root.now ? root.now.shuffle : false
              tooltipText: "Shuffle"
              onClicked: root.cliamp(["shuffle", "toggle"])
            }
            Button {
              iconText: "󰑖"; foreground: root.foreground; accent: root.accent
              selected: root.now ? Model.repeatOn(root.now.repeat) : false
              tooltipText: "Repeat"
              onClicked: root.remote("repeat", { name: root.now && Model.repeatOn(root.now.repeat) ? "off" : "all" })
            }
          }

          Row {
            width: parent.width
            spacing: Style.space(8)
            visible: root.connected
            Label { id: volLabel; text: "󰕾"; color: root.dim; anchors.verticalCenter: parent.verticalCenter }
            PanelSlider {
              width: parent.width - volLabel.width - volValue.width - parent.spacing * 2
              anchors.verticalCenter: parent.verticalCenter
              bar: root.bar
              minimum: -30
              maximum: 6
              step: 0.5
              value: root.now ? root.now.volume : 0
              onMoved: function(v) { root.volumeDrag = v }
              onReleased: function(v) { root.volumeDrag = -100; root.cliamp(["volume", String(v)]) }
            }
            Label {
              id: volValue
              text: (root.volumeDrag > -100 ? root.volumeDrag : (root.now ? root.now.volume : 0)).toFixed(1) + " dB"
              color: root.dim
              font.pixelSize: Style.font.caption
              anchors.verticalCenter: parent.verticalCenter
            }
          }

          Button {
            text: "Open the Llama player"
            iconText: "󰎈"
            bordered: true
            foreground: root.foreground
            tooltipText: "o"
            onClicked: root.launchPlayer(true)
          }

          Label {
            visible: !root.llamaInstalled || root.playerStatus !== ""
            width: parent.width
            text: !root.llamaInstalled ? root.installText : root.playerStatus
            color: root.dim
            font.pixelSize: Style.font.bodySmall
            wrapMode: Text.Wrap
          }

          Hint { text: "Space play/pause · z/b previous/next · v stop · ←/→ volume · o player · 1-3 tabs" }
        }

        // ---- Library ------------------------------------------------------
        Column {
          visible: root.tab === "library"
          width: parent.width
          spacing: Style.space(8)

          Flow {
            width: parent.width
            spacing: Style.space(4)
            Repeater {
              model: root.providers
              Button {
                text: modelData.name
                fontSize: Style.font.caption
                foreground: root.foreground
                accent: root.accent
                selected: root.provider === modelData.key
                hasCursor: root.cursorActive && root.cursorSection === "providers" && root.cursorIndex === index
                onClicked: root.selectProvider(modelData.key)
              }
            }
          }

          Row {
            width: parent.width
            spacing: Style.space(6)
            visible: root.provider !== ""
            TextField {
              id: searchField
              width: parent.width - playlistsButton.width - parent.spacing
              enabled: root.providerSearchable
              placeholderText: root.providerSearchable
                ? "Search " + root.providerName + "… (/)"
                : root.providerName + " cannot be searched; browse its playlists"
              foreground: root.foreground
              font.family: root.fontFamily
              font.pixelSize: Style.font.bodySmall
              onAccepted: { root.search(text); keyCatcher.forceActiveFocus() }
              Keys.onEscapePressed: keyCatcher.forceActiveFocus()
              Keys.onTabPressed: keyCatcher.forceActiveFocus()
              Keys.onDownPressed: {
                keyCatcher.forceActiveFocus()
                if (root.results.length > 0) root.cursorTo("results", 0)
              }
            }
            Button {
              id: playlistsButton
              text: "Playlists"
              fontSize: Style.font.caption
              foreground: root.foreground
              tooltipText: "p"
              onClicked: root.browsePlaylists()
            }
          }

          Label {
            visible: root.libraryStatus !== ""
            width: parent.width
            text: root.libraryStatus
            color: root.dim
            font.pixelSize: Style.font.bodySmall
            wrapMode: Text.Wrap
          }

          ListView {
            id: resultList
            width: parent.width
            height: Math.min(contentHeight, Style.space(380))
            clip: true
            model: root.results
            boundsBehavior: Flickable.StopAtBounds
            ScrollBar.vertical: ScrollBar { policy: ScrollBar.AsNeeded }
            delegate: Rectangle {
              readonly property bool cursorHere: root.cursorActive && root.cursorSection === "results" && root.cursorIndex === index
              width: resultList.width
              height: Style.space(40)
              radius: Style.cornerRadius
              color: rowMouse.containsMouse || cursorHere ? Qt.rgba(root.foreground.r, root.foreground.g, root.foreground.b, 0.08) : "transparent"
              border.width: cursorHere ? 1 : 0
              border.color: root.accent
              Column {
                anchors.left: parent.left
                anchors.right: parent.right
                anchors.verticalCenter: parent.verticalCenter
                anchors.leftMargin: Style.space(6)
                anchors.rightMargin: Style.space(6)
                Label {
                  width: parent.width
                  text: root.resultsKind === "playlists" ? String(modelData.name || "") : Model.trackTitle(modelData)
                  font.pixelSize: Style.font.bodySmall
                }
                Label {
                  width: parent.width
                  visible: text !== ""
                  text: root.resultsKind === "playlists"
                    ? (modelData.track_count ? modelData.track_count + " tracks" : "")
                    : Model.trackArtist(modelData)
                  color: root.dim
                  font.pixelSize: Style.font.caption
                }
              }
              MouseArea {
                id: rowMouse
                anchors.fill: parent
                hoverEnabled: true
                acceptedButtons: Qt.LeftButton | Qt.RightButton
                cursorShape: Qt.PointingHandCursor
                onClicked: function(mouse) { root.playResult(modelData, mouse.button === Qt.RightButton) }
              }
            }
          }

          Hint {
            text: !root.cliampInstalled || root.provider === "" ? "1-3 tabs"
              : root.resultsKind === "playlists"
              ? "Click or Enter opens a playlist · right-click or a plays all of it · / search · ↑/↓ move"
              : "Click or Enter plays · right-click or a adds to the queue · / search · p playlists · ↑/↓ move"
          }
        }

        // ---- Skins ---------------------------------------------------------
        Column {
          visible: root.tab === "skins"
          width: parent.width
          spacing: Style.space(8)

          Label {
            visible: !root.llamaInstalled
            width: parent.width
            text: "The Llama player is not installed. " + root.installText + "."
            wrapMode: Text.Wrap
            font.pixelSize: Style.font.bodySmall
          }

          Row {
            width: parent.width
            spacing: Style.space(6)
            visible: root.llamaInstalled
            Label {
              width: parent.width - baseButton.width - parent.spacing
              anchors.verticalCenter: parent.verticalCenter
              text: "Wearing: " + (root.currentSkin ? root.currentSkin.name : "…")
              font.pixelSize: Style.font.bodySmall
            }
            Button {
              id: baseButton
              text: "Base skin"
              fontSize: Style.font.caption
              foreground: root.foreground
              tooltipText: "0"
              onClicked: root.skinCommand(["base"], "Back to the base skin…")
            }
          }

          TextField {
            id: skinField
            visible: root.llamaInstalled
            width: parent.width
            placeholderText: "Search the Winamp Skin Museum… (/)"
            foreground: root.foreground
            font.family: root.fontFamily
            font.pixelSize: Style.font.bodySmall
            onAccepted: { root.skinQuery = text.trim(); root.loadSkins(false); keyCatcher.forceActiveFocus() }
            Keys.onEscapePressed: keyCatcher.forceActiveFocus()
            Keys.onTabPressed: keyCatcher.forceActiveFocus()
            Keys.onDownPressed: {
              keyCatcher.forceActiveFocus()
              if (root.skins.length > 0) root.cursorTo("skins", 0)
            }
          }

          Label {
            visible: root.llamaInstalled && root.skinStatus !== ""
            width: parent.width
            text: root.skinStatus
            color: root.dim
            font.pixelSize: Style.font.bodySmall
            wrapMode: Text.Wrap
          }

          GridView {
            id: skinGrid
            visible: root.llamaInstalled
            width: parent.width
            height: Style.space(400)
            clip: true
            cellWidth: width / 2
            cellHeight: cellWidth * 116 / 275 + Style.space(22)
            model: root.skins
            boundsBehavior: Flickable.StopAtBounds
            ScrollBar.vertical: ScrollBar { policy: ScrollBar.AsNeeded }
            onAtYEndChanged: if (atYEnd && count > 0) root.loadSkins(true)
            delegate: Item {
              width: skinGrid.cellWidth
              height: skinGrid.cellHeight
              readonly property bool cursorHere: root.cursorActive && root.cursorSection === "skins" && root.cursorIndex === index
              Rectangle {
                anchors.fill: parent
                anchors.margins: Style.space(3)
                radius: Style.cornerRadius
                color: skinMouse.containsMouse || parent.cursorHere ? Qt.rgba(root.foreground.r, root.foreground.g, root.foreground.b, 0.1) : "transparent"
                border.width: root.currentSkin && root.currentSkin.md5 === modelData.md5 ? 2 : (parent.cursorHere ? 1 : 0)
                border.color: root.accent
                Image {
                  id: shot
                  anchors.top: parent.top
                  anchors.horizontalCenter: parent.horizontalCenter
                  anchors.topMargin: Style.space(3)
                  width: parent.width - Style.space(6)
                  height: width * 116 / 275
                  source: modelData.screenshot
                  // Museum screenshots are the main window at 1x: crop to it.
                  sourceClipRect: Qt.rect(0, 0, 275, 116)
                  fillMode: Image.PreserveAspectFit
                  smooth: false
                  asynchronous: true
                  cache: true
                }
                Label {
                  anchors.top: shot.bottom
                  anchors.left: parent.left
                  anchors.right: parent.right
                  anchors.margins: Style.space(3)
                  text: String(modelData.name || "")
                  font.pixelSize: Style.font.caption
                  horizontalAlignment: Text.AlignHCenter
                }
                MouseArea {
                  id: skinMouse
                  anchors.fill: parent
                  hoverEnabled: true
                  cursorShape: Qt.PointingHandCursor
                  onClicked: root.wearSkin(modelData.md5)
                }
              }
            }
          }

          Hint {
            visible: root.llamaInstalled
            text: "Click or Enter wears a skin · / search · 0 base skin · arrows move · 1-3 tabs"
          }
        }
      }
      }
    }
  }
}

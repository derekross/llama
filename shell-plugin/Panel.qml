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
  readonly property color foreground: bar ? bar.foreground : Color.foreground
  readonly property color dim: Qt.darker(foreground, 1.55)
  readonly property color accent: Color.accent
  readonly property string fontFamily: bar ? bar.fontFamily : Style.font.family
  readonly property string home: Quickshell.env("HOME")
  readonly property string llamaBin: home + "/.local/bin/llama"
  readonly property int panelWidth: Style.space(420)

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

  property var skins: []
  property int skinPage: 0
  property string skinQuery: ""
  property string skinStatus: ""
  property var currentSkin: null

  function open() {
    root.controller.show()
    refreshStatus()
    if (tab === "library" && providers.length === 0) loadProviders()
    if (tab === "skins") ensureSkins()
  }
  function close() { root.controller.hide() }
  function toggle() { root.opened ? close() : open() }
  function setTab(t) {
    tab = t
    if (t === "library" && providers.length === 0) loadProviders()
    if (t === "skins") ensureSkins()
  }

  function launchPlayer() { Quickshell.execDetached([llamaBin]) }

  // Fire-and-forget cliamp commands; the status poll shows their effect.
  function cliamp(args) {
    Quickshell.execDetached(["cliamp"].concat(args))
    statusSoon.restart()
  }
  function remote(op, params) {
    cliamp(["remote", "call", op, "--params", JSON.stringify(params || {})])
  }
  function volumeStep(dir) { remote("volume.adjust", { value: dir }) }

  function refreshStatus() { if (!statusProc.running) statusProc.running = true }

  function loadProviders() {
    libraryStatus = "Loading sources…"
    providersProc.running = true
  }
  function selectProvider(key) {
    provider = key
    results = []
    libraryStatus = ""
    browsePlaylists()
  }
  function search(text) {
    if (provider === "" || String(text).trim() === "") return
    libraryStatus = "Searching…"
    resultsKind = "tracks"
    libraryProc.command = ["cliamp", "remote", "call", "provider.search", "--wait", "--params",
      JSON.stringify({ provider: provider, query: String(text).trim(), limit: 50 })]
    libraryProc.running = true
  }
  function browsePlaylists() {
    if (provider === "") return
    libraryStatus = "Loading playlists…"
    resultsKind = "playlists"
    libraryProc.command = ["cliamp", "remote", "call", "provider.playlists", "--wait", "--params",
      JSON.stringify({ provider: provider, limit: 100 })]
    libraryProc.running = true
  }
  function openPlaylist(item) {
    // cliamp lists the whole playlist (a big YouTube one takes minutes) and
    // answers nothing else meanwhile, Stop included.
    libraryStatus = "Loading " + item.name + "… A big playlist can take minutes, and playback controls wait until it is done."
    resultsKind = "tracks"
    libraryProc.command = ["cliamp", "remote", "call", "provider.tracks", "--wait", "--params",
      JSON.stringify({ provider: provider, playlist: item.id, limit: 200 })]
    libraryProc.running = true
  }
  function playPlaylist(item) {
    remote("provider.load", { provider: provider, playlist: item.id })
    remote("play", {})
  }

  function ensureSkins() {
    if (skins.length === 0 && !skinsProc.running) loadSkins(false)
    currentProc.running = true
  }
  function loadSkins(more) {
    skinPage = more ? skinPage + 1 : 0
    skinStatus = more ? "Loading more…" : (skinQuery === "" ? "Loading the Museum's favorites…" : "Searching ~100,000 skins…")
    var cmd = skinQuery === ""
      ? [llamaBin, "skin", "featured", "--page", String(skinPage)]
      : [llamaBin, "skin", "search", skinQuery, "--page", String(skinPage)]
    skinsProc.append = more
    skinsProc.command = cmd
    skinsProc.running = true
  }
  function wearSkin(md5) {
    skinStatus = "Dressing the llama…"
    useProc.command = [llamaBin, "skin", "use", md5]
    useProc.running = true
  }

  // ---- processes ----------------------------------------------------------

  Process {
    id: statusProc
    command: ["cliamp", "status", "--json"]
    stdout: StdioCollector {
      waitForEnd: true
      onStreamFinished: {
        var s = Model.status(text)
        root.connected = s !== null
        root.now = s
      }
    }
    onExited: function(code) { if (code !== 0) { root.connected = false; root.now = null } }
  }

  Timer {
    interval: root.opened ? 1000 : 4000
    running: true
    repeat: true
    triggeredOnStart: true
    onTriggered: root.refreshStatus()
  }

  Timer {
    id: statusSoon
    interval: 250
    onTriggered: root.refreshStatus()
  }

  Process {
    id: providersProc
    command: ["cliamp", "remote", "call", "provider.list", "--wait"]
    stdout: StdioCollector {
      waitForEnd: true
      onStreamFinished: {
        var r = Model.jobResult(text)
        if (!r) { root.libraryStatus = Model.jobError(text) || "cliamp is not running — open Llama first"; return }
        root.providers = r.providers || []
        root.libraryStatus = ""
        if (root.provider === "" && root.providers.length > 0) root.selectProvider(root.providers[0].key)
      }
    }
  }

  Process {
    id: libraryProc
    stdout: StdioCollector {
      waitForEnd: true
      onStreamFinished: {
        var r = Model.jobResult(text)
        if (!r) { root.libraryStatus = Model.jobError(text) || "Nothing came back"; root.results = []; return }
        root.results = root.resultsKind === "playlists" ? (r.playlists || []) : (r.tracks || [])
        root.libraryStatus = root.results.length === 0 ? "Nothing here" : ""
      }
    }
  }

  Process {
    id: skinsProc
    property bool append: false
    stdout: StdioCollector {
      waitForEnd: true
      onStreamFinished: {
        var r = Model.parse(text)
        if (!r || !r.skins) { root.skinStatus = "The Skin Museum is unreachable"; return }
        root.skins = skinsProc.append ? root.skins.concat(r.skins) : r.skins
        root.skinStatus = root.skins.length === 0 ? "No skins found" : ""
      }
    }
  }

  Process {
    id: useProc
    stdout: StdioCollector {
      waitForEnd: true
      onStreamFinished: {
        var r = Model.parse(text)
        if (r) root.currentSkin = r
        root.skinStatus = r && !r.running ? "Worn — middle-click the bar icon to open Llama" : ""
      }
    }
    onExited: function(code) { if (code !== 0) root.skinStatus = "That skin would not load" }
  }

  Process {
    id: currentProc
    command: [root.llamaBin, "skin", "current"]
    stdout: StdioCollector {
      waitForEnd: true
      onStreamFinished: { var r = Model.parse(text); if (r) root.currentSkin = r }
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
      onCloseRequested: root.close()
      onTabRequested: function(direction) {
        var tabs = ["now", "library", "skins"]
        var i = (tabs.indexOf(root.tab) + direction + tabs.length) % tabs.length
        root.setTab(tabs[i])
      }
      onTextKey: function(t) {
        if (root.tab !== "now") return
        if (t === " " ) root.cliamp(["toggle"])
        else if (t === "b") root.cliamp(["next"])
        else if (t === "z") root.cliamp(["prev"])
      }

      Column {
        id: column
        width: root.panelWidth
        spacing: Style.space(10)

        Row {
          spacing: Style.space(6)
          TabButton { key: "now"; text: "Now Playing" }
          TabButton { key: "library"; text: "Library" }
          TabButton { key: "skins"; text: "Skins" }
        }

        PanelSeparator { width: parent.width; foreground: root.foreground }

        // ---- Now Playing ------------------------------------------------
        Column {
          visible: root.tab === "now"
          width: parent.width
          spacing: Style.space(10)

          Label {
            width: parent.width
            text: !root.connected ? "cliamp is not running" : (root.nowTitle !== "" ? root.nowTitle : "Nothing playing")
            font.pixelSize: Style.font.heading
            font.bold: true
          }
          Label {
            width: parent.width
            visible: text !== ""
            text: root.now ? root.now.artist : ""
            color: root.dim
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
            Button { iconText: root.playing ? "󰏤" : "󰐊"; foreground: root.foreground; tooltipText: "Play/pause (space)"; onClicked: root.cliamp(["toggle"]) }
            Button { iconText: "󰓛"; foreground: root.foreground; tooltipText: "Stop"; onClicked: root.cliamp(["stop"]) }
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
            onClicked: { root.launchPlayer(); root.close() }
          }
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
              placeholderText: "Search " + root.provider + "…"
              foreground: root.foreground
              font.family: root.fontFamily
              font.pixelSize: Style.font.bodySmall
              onAccepted: root.search(text)
            }
            Button {
              id: playlistsButton
              text: "Playlists"
              fontSize: Style.font.caption
              foreground: root.foreground
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
              width: resultList.width
              height: Style.space(40)
              radius: Style.cornerRadius
              color: rowMouse.containsMouse ? Qt.rgba(root.foreground.r, root.foreground.g, root.foreground.b, 0.08) : "transparent"
              Column {
                anchors.left: parent.left
                anchors.right: parent.right
                anchors.verticalCenter: parent.verticalCenter
                anchors.leftMargin: Style.space(6)
                anchors.rightMargin: Style.space(6)
                Label {
                  width: parent.width
                  text: root.resultsKind === "playlists" ? modelData.name : Model.trackTitle(modelData)
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
                onClicked: function(mouse) {
                  if (root.resultsKind === "playlists") {
                    if (mouse.button === Qt.RightButton) root.playPlaylist(modelData)
                    else root.openPlaylist(modelData)
                  } else if (mouse.button === Qt.RightButton) {
                    root.remote("track.queue", { track: modelData })
                  } else {
                    root.remote("track.play", { track: modelData })
                  }
                }
              }
            }
          }

          Label {
            width: parent.width
            text: root.resultsKind === "playlists"
              ? "Click a playlist to open it · right-click to play all of it"
              : "Click to play · right-click to add to the queue"
            color: root.dim
            font.pixelSize: Style.font.caption
          }
        }

        // ---- Skins ---------------------------------------------------------
        Column {
          visible: root.tab === "skins"
          width: parent.width
          spacing: Style.space(8)

          Row {
            width: parent.width
            spacing: Style.space(6)
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
              onClicked: { useProc.command = [root.llamaBin, "skin", "base"]; useProc.running = true }
            }
          }

          TextField {
            width: parent.width
            placeholderText: "Search the Winamp Skin Museum…"
            foreground: root.foreground
            font.family: root.fontFamily
            font.pixelSize: Style.font.bodySmall
            onAccepted: { root.skinQuery = text.trim(); root.loadSkins(false) }
          }

          Label {
            visible: root.skinStatus !== ""
            width: parent.width
            text: root.skinStatus
            color: root.dim
            font.pixelSize: Style.font.bodySmall
          }

          GridView {
            id: skinGrid
            width: parent.width
            height: Style.space(400)
            clip: true
            cellWidth: width / 2
            cellHeight: cellWidth * 116 / 275 + Style.space(22)
            model: root.skins
            boundsBehavior: Flickable.StopAtBounds
            ScrollBar.vertical: ScrollBar { policy: ScrollBar.AsNeeded }
            onAtYEndChanged: if (atYEnd && count > 0 && !skinsProc.running) root.loadSkins(true)
            delegate: Item {
              width: skinGrid.cellWidth
              height: skinGrid.cellHeight
              Rectangle {
                anchors.fill: parent
                anchors.margins: Style.space(3)
                radius: Style.cornerRadius
                color: skinMouse.containsMouse ? Qt.rgba(root.foreground.r, root.foreground.g, root.foreground.b, 0.1) : "transparent"
                border.width: root.currentSkin && root.currentSkin.md5 === modelData.md5 ? 2 : 0
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
                  text: modelData.name
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
        }
      }
    }
  }
}

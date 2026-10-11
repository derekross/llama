import QtQuick
import Quickshell
import Quickshell.Io
import qs.Commons
import qs.Ui
import "Model.js" as Model

// The bar pill: a note while cliamp plays, dimmed when it does not. Left
// click opens the panel, right click opens (or focuses) the Llama window,
// middle click plays/pauses, the wheel changes the volume.
BarWidget {
  id: root
  moduleName: "derekross.llama"

  readonly property var panel: panelLoader.item

  // The bar is built once per monitor. The first live instance leads: it
  // owns the IPC target and polls cliamp in the background; the others show
  // its state. If the bar cannot list its instances, everyone leads, as before.
  readonly property var instances: bar && typeof bar.moduleWidgets === "function" ? bar.moduleWidgets(moduleName) : []
  readonly property bool isLead: instances.length === 0 || instances.indexOf(root) < 0 || instances[0] === root
  readonly property var statusPanel: !isLead && instances[0] && instances[0].panel ? instances[0].panel : panel

  readonly property bool playing: statusPanel ? statusPanel.playing : false
  readonly property string nowTitle: statusPanel ? statusPanel.nowTitle : ""
  readonly property bool showTitle: setting("showTitle", false) === true
  readonly property string label: {
    var icon = playing ? "󰝚" : "󰎈"
    if (showTitle && nowTitle !== "") return icon + "  " + Model.shortTitle(nowTitle, 30)
    return icon
  }

  function injectPanel() {
    var target = panelLoader.item
    if (!target) return
    if ("bar" in target) target.bar = root.bar
    if ("settings" in target) target.settings = root.settings
    if ("anchorItem" in target) target.anchorItem = button
    if ("hostWidget" in target) target.hostWidget = root
  }

  onBarChanged: injectPanel()
  onSettingsChanged: injectPanel()

  readonly property bool opened: panel ? panel.opened === true : false
  readonly property bool popoutSwitchClosing: panel ? panel.popoutSwitchClosing === true : false
  function open() { if (panel) panel.open() }
  function close() { if (panel) panel.close() }
  function togglePanel() { if (panel) panel.toggle() }
  function closeForPopoutSwitch() { if (panel) panel.closeForPopoutSwitch() }

  implicitWidth: button.implicitWidth
  implicitHeight: button.implicitHeight

  Loader {
    id: panelLoader
    active: true
    source: Qt.resolvedUrl("Panel.qml")
    visible: false
    onLoaded: {
      root.injectPanel()
      Qt.callLater(root.injectPanel)
    }
  }

  // Only the lead registers, so a second monitor does not warn about a
  // duplicate target. `omarchy-shell shell summon|toggle derekross.llama`
  // reaches the panel on the focused monitor instead.
  IpcHandler {
    enabled: root.isLead
    target: "derekross.llama"
    function open(): void { root.open() }
    function close(): void { root.close() }
    function toggle(): void { root.togglePanel() }
    function player(): void { if (root.panel) root.panel.launchPlayer(false) }
    function skins(): void { if (root.panel) root.panel.showSkins() }
  }

  // Touchpads send many small wheel deltas; a step is a full notch (120).
  property real wheelAccum: 0

  WidgetButton {
    id: button
    anchors.fill: parent
    bar: root.bar
    text: root.label
    fontSize: root.showTitle ? Style.font.caption : Style.bar.iconFont
    dimmed: !root.playing
    tooltipText: root.nowTitle !== ""
      ? root.nowTitle + "\nClick: Llama · Right: player window · Middle: play/pause"
      : "Llama — click to browse, right-click for the player window"

    onPressed: function(b) {
      if (!root.panel) return
      if (b === Qt.RightButton) root.panel.launchPlayer(false)
      else if (b === Qt.MiddleButton) root.panel.cliamp(["toggle"])
      else root.togglePanel()
    }
    onWheelMoved: function(delta) {
      // 0 is a horizontal swipe: not a volume change.
      if (!root.panel || delta === 0) return
      if ((delta > 0) !== (root.wheelAccum > 0)) root.wheelAccum = 0
      root.wheelAccum += delta
      while (Math.abs(root.wheelAccum) >= 120) {
        var dir = root.wheelAccum > 0 ? 1 : -1
        root.wheelAccum -= dir * 120
        root.panel.volumeStep(dir)
      }
    }
  }
}

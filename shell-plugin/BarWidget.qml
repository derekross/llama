import QtQuick
import Quickshell
import Quickshell.Io
import qs.Commons
import qs.Ui

// The bar pill: a note while cliamp plays, dimmed when it does not. Left
// click opens the panel, middle click opens (or focuses) the Llama window,
// right click plays/pauses, the wheel changes the volume.
BarWidget {
  id: root
  moduleName: "derekross.llama"

  readonly property var panel: panelLoader.item
  readonly property bool playing: panel ? panel.playing : false
  readonly property bool showTitle: setting("showTitle", false) === true
  readonly property string label: {
    var icon = playing ? "󰝚" : "󰎈"
    if (showTitle && panel && panel.nowTitle !== "") return icon + "  " + panel.nowTitle
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

  IpcHandler {
    target: "derekross.llama"
    function open(): void { root.open() }
    function close(): void { root.close() }
    function toggle(): void { root.togglePanel() }
    function player(): void { if (root.panel) root.panel.launchPlayer() }
  }

  WidgetButton {
    id: button
    anchors.fill: parent
    bar: root.bar
    text: root.label
    fontSize: root.showTitle ? Style.font.caption : Style.bar.iconFont
    dimmed: !root.playing
    tooltipText: root.panel && root.panel.nowTitle !== ""
      ? root.panel.nowTitle + "\nClick: Llama · Middle: player window · Right: play/pause"
      : "Llama — click to browse, middle-click for the player window"

    onPressed: function(b) {
      if (!root.panel) return
      if (b === Qt.MiddleButton) root.panel.launchPlayer()
      else if (b === Qt.RightButton) root.panel.cliamp(["toggle"])
      else root.togglePanel()
    }
    onWheelMoved: function(delta) {
      if (root.panel) root.panel.volumeStep(delta > 0 ? 1 : -1)
    }
  }
}

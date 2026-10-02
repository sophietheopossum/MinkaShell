import QtQuick
import "../../services"

// Workspace pills for one output, bound to ShojiIpc.view (pushed by the
// workspaces.changed broadcast — no polling). Click activates; middle-click
// toggles tiling for this monitor, mirroring shoji-bar-2.
Row {
    id: root

    required property string monitorName

    readonly property var monitor: ShojiIpc.monitorView(monitorName)

    spacing: 5

    Repeater {
        id: pills
        // Desktops off: no pills. Fails open while the compositor still
        // reports several desktops (e.g. before the config is reloaded).
        model: !root.monitor ? []
             : ShellLayout.workspacesEnabled || root.monitor.workspaces.length > 1
               ? root.monitor.workspaces : []

        delegate: Rectangle {
            id: pill

            required property var modelData

            readonly property bool active: modelData.active

            width: Math.max(22, label.implicitWidth + 12)
            height: 20
            radius: 4
            color: active ? Theme.red
                 : pillArea.containsMouse ? Theme.surfaceRaised
                 : "transparent"
            border.width: 1
            border.color: active ? Theme.red
                        : modelData.windowCount > 0 ? Theme.textFaint
                        : Theme.line

            Behavior on color {
                ColorAnimation { duration: 120 }
            }

            Text {
                id: label
                anchors.centerIn: parent
                // Compositor desktop indices are already 1-based.
                text: pill.modelData.index
                font.family: Theme.monoFamily
                font.pixelSize: Theme.fontSize - 1
                color: pill.active ? Theme.ground
                     : pill.modelData.windowCount > 0 ? Theme.text
                     : Theme.textFaint
            }

            MouseArea {
                id: pillArea
                anchors.fill: parent
                hoverEnabled: true
                acceptedButtons: Qt.LeftButton | Qt.MiddleButton
                onClicked: mouse => {
                    if (mouse.button === Qt.MiddleButton)
                        ShojiIpc.toggleTiling(root.monitorName);
                    else
                        ShojiIpc.activateWorkspace(root.monitorName, pill.modelData.index);
                }
            }
        }
    }

    // Tiling-mode marker for the active workspace (shoji-bar-2's LayoutMode).
    Text {
        visible: root.monitor !== null
        anchors.verticalCenter: parent.verticalCenter
        leftPadding: pills.count > 0 ? 4 : 0
        text: {
            const ws = root.monitor
                ? root.monitor.workspaces.find(w => w.active)
                : null;
            return ws && ws.isTiled ? "◫" : "◰";
        }
        font.pixelSize: Theme.fontSize
        color: Theme.purple

        // With the pills hidden (desktops off) this is the mouse's way to
        // toggle tiling; with pills it stays inert, as before.
        MouseArea {
            anchors.fill: parent
            enabled: pills.count === 0
            acceptedButtons: Qt.LeftButton | Qt.MiddleButton
            onClicked: ShojiIpc.toggleTiling(root.monitorName)
        }
    }
}

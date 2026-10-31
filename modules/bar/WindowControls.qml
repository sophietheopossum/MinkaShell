import QtQuick
import "../../services"

// Minimize / maximize-restore controls for the focused window (same lookup
// as FocusedTitle).
// Glass traffic-light artwork by Abhishek (@dcrey7); see
// ../../assets/CREDITS.md. The bodies stay visible, with glyphs on hover.
Row {
    id: root

    required property string monitorName

    readonly property var focusedWindow: ShojiIpc.focusedWindowFor(monitorName)

    visible: focusedWindow !== null
    spacing: 8

    Item {
        anchors.verticalCenter: parent.verticalCenter
        width: 16
        height: 16

        Image {
            anchors.fill: parent
            source: "../../assets/mac-glass-minimize.svg"
            sourceSize: Qt.size(16, 16)
        }

        Image {
            anchors.fill: parent
            source: "../../assets/mac-glass-minus.svg"
            sourceSize: Qt.size(16, 16)
            visible: minimizeArea.containsMouse
        }

        MouseArea {
            id: minimizeArea
            anchors.fill: parent
            hoverEnabled: true
            onClicked: {
                if (!root.focusedWindow)
                    return;
                // A minimized window keeps focus, so the same button restores
                // it: windows.activate unminimizes and refocuses.
                if (root.focusedWindow.minimized)
                    ShojiIpc.activateWindow(root.focusedWindow.id);
                else
                    ShojiIpc.minimizeWindow(root.focusedWindow.id);
            }
        }
    }

    Item {
        anchors.verticalCenter: parent.verticalCenter
        width: 16
        height: 16

        Image {
            anchors.fill: parent
            source: "../../assets/mac-glass-maximize.svg"
            sourceSize: Qt.size(16, 16)
        }

        Image {
            anchors.fill: parent
            source: root.focusedWindow && root.focusedWindow.maximized
                ? "../../assets/mac-glass-shrink.svg"
                : "../../assets/mac-glass-expand.svg"
            sourceSize: Qt.size(16, 16)
            visible: maximizeArea.containsMouse
        }

        MouseArea {
            id: maximizeArea
            anchors.fill: parent
            hoverEnabled: true
            onClicked: {
                if (root.focusedWindow)
                    ShojiIpc.maximizeWindow(
                        root.focusedWindow.id,
                        !root.focusedWindow.maximized);
            }
        }
    }
}
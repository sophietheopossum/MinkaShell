import Quickshell
import QtQuick
import "../../services"
import "../bar"

// Duo-mode ScreenPad side column (Sophie's spec, 2/8/2026).
//
// The ScreenPad layout is: dock across the top, then the remaining height
// split into a wide zone (MinkaMon's schematic, a layer surface of its own)
// and this narrow column on the right. The Bar is hidden entirely in duo
// mode and everything it carried lives here instead.
//
// Coordination with MinkaMon is pure layer-shell arithmetic, not IPC: this
// panel reserves its width via exclusiveZone, the dock reserves its height,
// and MinkaMon anchors all four edges with no exclusive zone of its own, so
// it lands in exactly what is left over. Neither app knows the other's size.
PanelWindow {
    id: root

    required property var modelData

    screen: modelData
    // Duo-only surface: in the general layout the Bar does this job.
    visible: ShellLayout.duoMode && ShellLayout.showBarOn(modelData)

    anchors {
        top: true
        bottom: true
        right: true
    }

    // FIXED, not bound to the content's live width. FocusedTitle elides at
    // 320px, so a content-bound width would change every time a window with
    // a different title length takes focus — and because MinkaMon sizes
    // itself from whatever exclusive zone is left, that would shove the
    // schematic sideways on every focus change. Sized once to fit the
    // content's natural maximum instead.
    implicitWidth: 400
    exclusiveZone: implicitWidth
    color: "transparent"

    Rectangle {
        anchors.fill: parent
        color: Theme.barBg

        // Seam against MinkaMon's zone.
        Rectangle {
            anchors.left: parent.left
            anchors.top: parent.top
            anchors.bottom: parent.bottom
            width: 1
            color: Theme.line
        }

        Column {
            id: stack

            anchors.left: parent.left
            anchors.right: parent.right
            anchors.top: parent.top
            anchors.leftMargin: 12
            anchors.rightMargin: 12
            anchors.topMargin: 10
            spacing: 10

            // ── focused window: icon + title, then its controls ──────────
            FocusedTitle {
                monitorName: root.modelData.name
            }

            WindowControls {
                monitorName: root.modelData.name
            }

            Rectangle {
                width: parent.width
                height: 1
                color: Theme.line
            }

            // ── system tray (volume and battery ride along as applets) ───
            SystemTrayWidget {
                monitorName: root.modelData.name
            }

            Rectangle {
                width: parent.width
                height: 1
                color: Theme.line
            }

            // ── everything the Bar used to carry ─────────────────────────
            Rectangle {
                width: clock.implicitWidth + 16
                height: 22
                radius: 5
                color: MenuState.isOpen("calendar", root.modelData.name) ? Theme.redDim
                     : clockArea.containsMouse ? Theme.surfaceRaised
                     : "transparent"

                Clock {
                    id: clock
                    anchors.centerIn: parent
                }

                MouseArea {
                    id: clockArea
                    anchors.fill: parent
                    hoverEnabled: true
                    onClicked: MenuState.toggle("calendar", root.modelData.name)
                }
            }

            Workspaces {
                monitorName: root.modelData.name
            }

            SysUsage {}
        }

        // ── start menu + IPC health, pinned to the bottom ────────────────
        Row {
            anchors.left: parent.left
            anchors.bottom: parent.bottom
            anchors.leftMargin: 12
            anchors.bottomMargin: 10
            spacing: 12

            Rectangle {
                id: startButton

                anchors.verticalCenter: parent.verticalCenter
                width: 24
                height: 22
                radius: 5
                color: MenuState.isOpen("start", root.modelData.name) ? Theme.redDim
                     : startArea.containsMouse ? Theme.surfaceRaised
                     : "transparent"

                Text {
                    anchors.centerIn: parent
                    text: "❖"
                    font.pixelSize: Theme.fontSize + 1
                    color: startArea.containsMouse ? Theme.red : Theme.textMuted
                }

                MouseArea {
                    id: startArea
                    anchors.fill: parent
                    hoverEnabled: true
                    onClicked: MenuState.toggle("start", root.modelData.name)
                }
            }

            // IPC health dot: red until the first workspace view arrives on
            // the current connection. A keeper — deliberate design detail.
            Rectangle {
                anchors.verticalCenter: parent.verticalCenter
                width: 7
                height: 7
                radius: 3.5
                color: ShojiIpc.ready ? Theme.redDim : Theme.red

                Behavior on color {
                    ColorAnimation { duration: 200 }
                }
            }
        }
    }
}
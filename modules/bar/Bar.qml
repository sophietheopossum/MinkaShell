import Quickshell
import QtQuick
import "../../services"

// Top panel, one per output. Instantiated by Variants in shell.qml, which
// injects the screen as modelData. Duo mode hides it on the main display —
// unmapping the surface also releases the exclusive zone.
PanelWindow {
    id: root

    required property var modelData

    screen: modelData
    // Duo mode has no bar at all: the ScreenPad layout is dock across the
    // top, MinkaMon's zone, and SidePanel on the right, which carries
    // everything this bar used to hold.
    visible: !ShellLayout.duoMode && ShellLayout.showBarOn(modelData)

    anchors {
        top: true
        left: true
        right: true
    }
    implicitHeight: Theme.barHeight
    exclusiveZone: Theme.barHeight
    color: "transparent"

    Rectangle {
        anchors.fill: parent
        color: Theme.barBg

        Rectangle {
            anchors.bottom: parent.bottom
            anchors.left: parent.left
            anchors.right: parent.right
            height: 1
            color: Theme.line
        }

        Row {
            anchors.left: parent.left
            anchors.leftMargin: 10
            anchors.verticalCenter: parent.verticalCenter
            spacing: 12

            // Start-menu toggle.
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

            Workspaces {
                anchors.verticalCenter: parent.verticalCenter
                monitorName: root.modelData.name
            }

            FocusedTitle {
                anchors.verticalCenter: parent.verticalCenter
                monitorName: root.modelData.name
            }

            WindowControls {
                anchors.verticalCenter: parent.verticalCenter
                monitorName: root.modelData.name
            }
        }

        // Clock doubles as the calendar-menu toggle.
        Rectangle {
            anchors.horizontalCenter: parent.horizontalCenter
            anchors.verticalCenter: parent.verticalCenter
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

        Row {
            anchors.right: parent.right
            anchors.rightMargin: 12
            anchors.verticalCenter: parent.verticalCenter
            spacing: 12

            SysUsage {
                anchors.verticalCenter: parent.verticalCenter
            }

            // Volume & battery live inside the tray cluster as native
            // applets.
            SystemTrayWidget {
                anchors.verticalCenter: parent.verticalCenter
                monitorName: root.modelData.name
            }

            // IPC health: red until the first workspace view arrives on the
            // current connection. A keeper — Sophie wants this in the final
            // release as a design detail.
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

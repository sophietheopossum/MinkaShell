import QtQuick
import Quickshell
import "../../services"

// Pending-update indicator, fed by SyuState (MinkaSyu's state file).
// Hidden at zero: an up-to-date system needs no furniture. Amber when
// any update switches repository -- the one thing in the list worth a
// look before pressing go. Click opens the MinkaSyu window.
Rectangle {
    id: root

    readonly property bool attention: SyuState.repoChanges > 0

    visible: SyuState.count > 0
    width: label.implicitWidth + 14
    height: 20
    radius: 10
    color: attention ? (area.containsMouse ? Theme.warnAmber : Theme.surfaceRaised)
                     : (area.containsMouse ? Theme.surfaceRaised : "transparent")
    border.width: 1
    border.color: attention ? Theme.warnAmber : Theme.line

    Text {
        id: label
        anchors.centerIn: parent
        text: "↑ " + SyuState.count + (root.attention ? "  ⇄ " + SyuState.repoChanges : "")
        font.family: Theme.fontFamily
        font.pixelSize: Theme.fontSize - 1
        color: root.attention && area.containsMouse ? Theme.ground
             : root.attention ? Theme.warnAmber
             : area.containsMouse ? Theme.text : Theme.textMuted
    }

    MouseArea {
        id: area
        anchors.fill: parent
        hoverEnabled: true
        cursorShape: Qt.PointingHandCursor
        // `minka-syu ui` picks the window: the AntiWebUI front end, or the
        // QML one (MINKA_SYU_DIR in dev, /usr/share/minka-syu/ui installed).
        onClicked: Quickshell.execDetached([Quickshell.env("MINKA_SYU_BIN") || "minka-syu", "ui"])
    }
}

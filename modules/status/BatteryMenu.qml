import Quickshell
import Quickshell.Wayland
import QtQuick
import Quickshell.Services.UPower
import "../../services"

// Battery popover, opened from the bar's battery applet. Percent, gauge,
// and charge/discharge time estimate from UPower's display device.
PanelWindow {
    id: root

    required property var modelData

    readonly property bool open: MenuState.isOpen("battery", modelData?.name ?? "")
    readonly property var device: UPower.displayDevice
    readonly property bool present: device !== null && device.isLaptopBattery
    readonly property bool charging: present && device.state === UPowerDeviceState.Charging
    readonly property int percent: present ? Math.round(device.percentage * 100) : 0
    readonly property bool low: present && !charging && percent <= 20

    function fmtSecs(s) {
        if (s <= 0)
            return "";
        const h = Math.floor(s / 3600);
        const m = Math.round((s % 3600) / 60);
        return h > 0 ? h + "h " + m + "m" : m + "m";
    }

    readonly property string stateText: {
        if (!present)
            return "no battery";
        if (device.state === UPowerDeviceState.FullyCharged)
            return "full";
        if (charging) {
            const t = fmtSecs(device.timeToFull);
            return t ? "charging · " + t + " to full" : "charging";
        }
        const t = fmtSecs(device.timeToEmpty);
        return t ? t + " left" : "discharging";
    }

    screen: modelData
    visible: open
    // Above MenuBackdrop's top layer, so the backdrop can never eat clicks
    // meant for the menu.
    WlrLayershell.layer: WlrLayer.Overlay
    anchors.top: true
    anchors.right: true
    margins.top: Theme.barHeight + 6
    margins.right: 8
    implicitWidth: 240
    implicitHeight: body.implicitHeight + 24
    exclusiveZone: 0
    color: "transparent"

    Rectangle {
        anchors.fill: parent
        radius: 10
        color: Theme.barBg
        border.width: 1
        border.color: Theme.line

        Column {
            id: body

            anchors.left: parent.left
            anchors.right: parent.right
            anchors.top: parent.top
            anchors.margins: 12
            spacing: 10

            Row {
                spacing: 8

                Text {
                    anchors.verticalCenter: parent.verticalCenter
                    text: "bat"
                    font.family: Theme.monoFamily
                    font.pixelSize: Theme.fontSize - 3
                    font.letterSpacing: 1
                    color: Theme.textFaint
                }

                Text {
                    anchors.verticalCenter: parent.verticalCenter
                    text: root.percent + "%"
                    font.family: Theme.monoFamily
                    font.pixelSize: Theme.fontSize + 2
                    color: root.low ? Theme.red : Theme.text
                }

                Text {
                    anchors.verticalCenter: parent.verticalCenter
                    visible: root.charging
                    text: "⚡"
                    font.pixelSize: Theme.fontSize
                    color: Theme.purple
                }
            }

            // Charge gauge; fills can be bright here — no text sits on them.
            Item {
                width: parent.width
                height: 8

                Rectangle {
                    anchors.fill: parent
                    radius: 4
                    color: Theme.surfaceRaised
                    border.width: 1
                    border.color: Theme.line
                }

                Rectangle {
                    anchors.left: parent.left
                    anchors.verticalCenter: parent.verticalCenter
                    anchors.leftMargin: 2
                    width: Math.max(2, (parent.width - 4) * root.percent / 100)
                    height: parent.height - 4
                    radius: 2
                    color: root.low ? Theme.red
                         : root.charging ? Theme.purple
                         : Theme.textMuted
                }
            }

            Text {
                text: root.stateText
                font.family: Theme.fontFamily
                font.pixelSize: Theme.fontSize - 2
                color: root.charging ? Theme.purple : Theme.textMuted
            }
        }
    }
}
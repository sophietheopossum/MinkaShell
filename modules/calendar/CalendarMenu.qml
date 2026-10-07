pragma ComponentBehavior: Bound

import Quickshell
import Quickshell.Wayland
import QtQuick
import "../../services"

// Month calendar, opened from the bar clock. Monday-first week (matching the
// d/M/yyyy date preference), today marked in red. Pure display for now —
// events/agenda would come from the settings utility era, if ever.
PanelWindow {
    id: root

    required property var modelData

    readonly property bool open: MenuState.isOpen("calendar", modelData?.name ?? "")

    // First day of the displayed month.
    property date shown: new Date()

    screen: modelData
    visible: open
    // Above MenuBackdrop's top layer, so the backdrop can never eat clicks
    // meant for the menu.
    WlrLayershell.layer: WlrLayer.Overlay
    anchors.top: true
    margins.top: Theme.barHeight + 6
    implicitWidth: 300
    implicitHeight: body.implicitHeight + 24
    exclusiveZone: 0
    color: "transparent"

    onOpenChanged: {
        if (open)
            shown = new Date();
    }

    function monthTitle() {
        return Qt.locale().monthName(shown.getMonth(), Locale.LongFormat)
            + " " + shown.getFullYear();
    }

    function shiftMonth(delta) {
        shown = new Date(shown.getFullYear(), shown.getMonth() + delta, 1);
    }

    // 42 cells (6 weeks), Monday-first.
    function cells() {
        const first = new Date(shown.getFullYear(), shown.getMonth(), 1);
        const lead = (first.getDay() + 6) % 7; // Sunday=0 -> Monday-first offset
        const out = [];
        const today = new Date();
        for (let i = 0; i < 42; i++) {
            const date = new Date(shown.getFullYear(), shown.getMonth(), 1 + i - lead);
            out.push({
                day: date.getDate(),
                inMonth: date.getMonth() === shown.getMonth(),
                today: date.getFullYear() === today.getFullYear()
                    && date.getMonth() === today.getMonth()
                    && date.getDate() === today.getDate()
            });
        }
        return out;
    }

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
            spacing: 8

            // Header: ‹ month year ›  + today shortcut
            Item {
                width: parent.width
                height: 24

                Rectangle {
                    id: prevButton
                    anchors.left: parent.left
                    width: 22
                    height: 22
                    radius: 5
                    color: prevArea.containsMouse ? Theme.surfaceRaised : "transparent"

                    Text {
                        anchors.centerIn: parent
                        text: "‹"
                        font.pixelSize: Theme.fontSize + 2
                        color: prevArea.containsMouse ? Theme.red : Theme.textMuted
                    }

                    MouseArea {
                        id: prevArea
                        anchors.fill: parent
                        hoverEnabled: true
                        onClicked: root.shiftMonth(-1)
                    }
                }

                Text {
                    anchors.centerIn: parent
                    text: root.monthTitle()
                    font.family: Theme.fontFamily
                    font.pixelSize: Theme.fontSize
                    font.weight: Font.DemiBold
                    color: Theme.text

                    MouseArea {
                        anchors.fill: parent
                        onClicked: root.shown = new Date()
                    }
                }

                Rectangle {
                    id: nextButton
                    anchors.right: parent.right
                    width: 22
                    height: 22
                    radius: 5
                    color: nextArea.containsMouse ? Theme.surfaceRaised : "transparent"

                    Text {
                        anchors.centerIn: parent
                        text: "›"
                        font.pixelSize: Theme.fontSize + 2
                        color: nextArea.containsMouse ? Theme.red : Theme.textMuted
                    }

                    MouseArea {
                        id: nextArea
                        anchors.fill: parent
                        hoverEnabled: true
                        onClicked: root.shiftMonth(1)
                    }
                }
            }

            // Weekday header, Monday-first.
            Grid {
                columns: 7
                width: parent.width

                Repeater {
                    model: ["mo", "tu", "we", "th", "fr", "sa", "su"]

                    delegate: Text {
                        required property var modelData

                        width: Math.floor((body.width) / 7)
                        horizontalAlignment: Text.AlignHCenter
                        text: modelData
                        font.family: Theme.monoFamily
                        font.pixelSize: Theme.fontSize - 3
                        color: Theme.textFaint
                    }
                }
            }

            Grid {
                columns: 7
                width: parent.width

                Repeater {
                    model: root.open ? root.cells() : []

                    delegate: Item {
                        id: cell

                        required property var modelData

                        width: Math.floor(body.width / 7)
                        height: 28

                        Rectangle {
                            anchors.centerIn: parent
                            width: 24
                            height: 24
                            radius: 12
                            color: cell.modelData.today ? Theme.red : "transparent"
                        }

                        Text {
                            anchors.centerIn: parent
                            text: cell.modelData.day
                            font.family: Theme.monoFamily
                            font.pixelSize: Theme.fontSize - 1
                            color: cell.modelData.today ? Theme.ground
                                 : cell.modelData.inMonth ? Theme.text
                                 : Theme.textFaint
                        }
                    }
                }
            }
        }
    }
}

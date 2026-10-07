pragma ComponentBehavior: Bound

import Quickshell
import Quickshell.Wayland
import QtQuick
import "../../services"

// Application launcher: a fullscreen searchable app grid (KDE "Application
// Dashboard" / Windows 8 style, per Sophie), one per output; MenuState
// decides which instance is open
// (follows the cursor's display, even in
// Duo mode).
// Opened by the bar button or the ui.startMenu IPC broadcast (Super+A /
// Super tap in the ShojiWM config). Escape or a click on empty space closes.
PanelWindow {
    id: root

    required property var modelData

    readonly property bool open: MenuState.isOpen("start", modelData?.name ?? "")
    property string query: ""

    // Search across everything a user might know the app by: display name,
    // description, desktop id, executable, and keywords. (Lesson from CMST:
    // its Name is "Connman UI Setup" — only Exec/id contain "cmst".)
    function entryMatches(entry, q) {
        const fields = [entry.name, entry.genericName, entry.comment, entry.id, entry.execString];
        if (entry.keywords)
            for (const keyword of entry.keywords)
                fields.push(keyword);
        return fields.some(f => f && String(f).toLowerCase().includes(q));
    }

    readonly property var apps: {
        const all = DesktopEntries.applications.values;
        const q = query.trim().toLowerCase();
        const list = q === "" ? all.slice() : all.filter(a => entryMatches(a, q));
        // Name-prefix hits first (so "fir" ranks Firefox above LibreOffice
        // Writer's comment), then alphabetical.
        list.sort((a, b) => {
            if (q !== "") {
                const aPrefix = a.name.toLowerCase().startsWith(q);
                const bPrefix = b.name.toLowerCase().startsWith(q);
                if (aPrefix !== bPrefix)
                    return aPrefix ? -1 : 1;
            }
            return a.name.localeCompare(b.name);
        });
        return list;
    }

    function launch(entry) {
        entry.execute();
        MenuState.closeAll();
    }

    screen: modelData
    visible: open
    focusable: true
    // Above the bar and everything else while open.
    WlrLayershell.layer: WlrLayer.Overlay

    anchors {
        top: true
        bottom: true
        left: true
        right: true
    }
    exclusiveZone: 0
    color: "transparent"

    onOpenChanged: {
        if (open) {
            query = "";
            searchField.text = "";
            searchField.forceActiveFocus();
        }
    }

    Rectangle {
        anchors.fill: parent
        color: Theme.ground
        opacity: root.open ? 0.96 : 0

        Behavior on opacity {
            NumberAnimation { duration: 140; easing.type: Easing.OutCubic }
        }
    }

    // Clicks that land on nothing interactive fall through to here.
    MouseArea {
        anchors.fill: parent
        onClicked: MenuState.closeAll()
    }

    Item {
        id: content

        anchors.fill: parent
        opacity: root.open ? 1 : 0
        scale: root.open ? 1 : 0.98

        Behavior on opacity {
            NumberAnimation { duration: 140; easing.type: Easing.OutCubic }
        }
        Behavior on scale {
            NumberAnimation { duration: 140; easing.type: Easing.OutCubic }
        }

        // Search
        Rectangle {
            id: searchBox

            anchors.top: parent.top
            // Scales down gracefully on the 515px-tall ScreenPad.
            anchors.topMargin: Math.max(20, parent.height * 0.05)
            anchors.horizontalCenter: parent.horizontalCenter
            width: Math.min(520, parent.width - 96)
            height: 36
            radius: 8
            color: Theme.surfaceRaised
            border.width: 1
            border.color: searchField.activeFocus ? Theme.red : Theme.line

            TextInput {
                id: searchField
                anchors.fill: parent
                anchors.leftMargin: 12
                anchors.rightMargin: 12
                verticalAlignment: TextInput.AlignVCenter
                font.family: Theme.fontFamily
                font.pixelSize: Theme.fontSize + 1
                color: Theme.text
                clip: true
                onTextChanged: root.query = text
                Keys.onEscapePressed: MenuState.closeAll()
                Keys.onReturnPressed: {
                    if (root.apps.length > 0)
                        root.launch(root.apps[0]);
                }

                Text {
                    anchors.verticalCenter: parent.verticalCenter
                    visible: searchField.text === ""
                    text: "search…"
                    font: searchField.font
                    color: Theme.textFaint
                }
            }
        }

        // App grid, centered on whole cells so the margins stay symmetric.
        GridView {
            id: grid

            readonly property int columns: Math.max(1,
                Math.floor(Math.min(parent.width - 96, 1400) / cellWidth))

            anchors.top: searchBox.bottom
            anchors.topMargin: Math.max(16, parent.height * 0.04)
            anchors.bottom: sessionRow.top
            anchors.bottomMargin: 16
            anchors.horizontalCenter: parent.horizontalCenter
            width: columns * cellWidth
            cellWidth: 112
            cellHeight: 104
            clip: true
            model: root.apps

            delegate: Item {
                id: cell

                required property var modelData
                required property int index

                // Enter launches the top hit while searching — mark it.
                readonly property bool topHit: index === 0 && root.query.trim() !== ""

                width: grid.cellWidth
                height: grid.cellHeight

                Rectangle {
                    anchors.fill: parent
                    anchors.margins: 4
                    radius: 10
                    color: cellArea.containsMouse ? Theme.surfaceRaised : "transparent"
                    border.width: cell.topHit ? 1 : 0
                    border.color: Theme.redDim
                }

                Column {
                    anchors.centerIn: parent
                    width: parent.width - 16
                    spacing: 8

                    Image {
                        anchors.horizontalCenter: parent.horizontalCenter
                        width: 44
                        height: 44
                        sourceSize.width: 44
                        sourceSize.height: 44
                        fillMode: Image.PreserveAspectFit
                        source: cell.modelData.icon
                            ? Quickshell.iconPath(cell.modelData.icon, "application-x-executable")
                            : Quickshell.iconPath("application-x-executable")
                    }

                    Text {
                        anchors.horizontalCenter: parent.horizontalCenter
                        width: parent.width
                        text: cell.modelData.name
                        font.family: Theme.fontFamily
                        font.pixelSize: Theme.fontSize - 2
                        color: Theme.text
                        horizontalAlignment: Text.AlignHCenter
                        elide: Text.ElideRight
                    }
                }

                MouseArea {
                    id: cellArea
                    anchors.fill: parent
                    hoverEnabled: true
                    onClicked: root.launch(cell.modelData)
                }
            }
        }

        // Session controls
        Row {
            id: sessionRow

            anchors.bottom: parent.bottom
            anchors.bottomMargin: Math.max(14, parent.height * 0.03)
            anchors.horizontalCenter: parent.horizontalCenter
            spacing: 10

            Repeater {
                model: [
                    // Ends the whole login session (compositor included);
                    // the display manager takes over from there.
                    { label: "logout", command: ["sh", "-c", "loginctl terminate-session \"$XDG_SESSION_ID\""] },
                    { label: "sleep", command: ["systemctl", "suspend"] },
                    { label: "reboot", command: ["systemctl", "reboot"] },
                    { label: "off", command: ["systemctl", "poweroff"] }
                ]

                delegate: Rectangle {
                    id: powerButton

                    required property var modelData

                    width: 72
                    height: 30
                    radius: 6
                    color: powerArea.containsMouse ? Theme.redDim : Theme.surfaceRaised
                    border.width: 1
                    border.color: powerArea.containsMouse ? Theme.red : Theme.line

                    Text {
                        anchors.centerIn: parent
                        text: powerButton.modelData.label
                        font.family: Theme.monoFamily
                        font.pixelSize: Theme.fontSize - 2
                        color: powerArea.containsMouse ? Theme.text : Theme.textMuted
                    }

                    MouseArea {
                        id: powerArea
                        anchors.fill: parent
                        hoverEnabled: true
                        onClicked: Quickshell.execDetached(powerButton.modelData.command)
                    }
                }
            }
        }
    }
}

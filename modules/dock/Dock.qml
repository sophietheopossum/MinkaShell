import Quickshell
import QtQuick
import "../../services"

// Persistent taskbar dock (Sophie's spec, 8/7/2026): always visible — the
// dock.proximity auto-hide is gone — with each window's title next to its
// icon and a right-click menu (close). Reserves an exclusive zone so
// maximized windows stop above it. Follows the bar's Duo policy: in duo
// mode only the ScreenPad carries it, and that instance lists every
// monitor's windows (the main display has no dock of its own); otherwise
// each output lists its own windows.
PanelWindow {
    id: root

    required property var modelData

    readonly property var windows: {
        const view = ShojiIpc.view;
        if (!view)
            return [];
        const all = [];
        for (const monitor of view.monitors) {
            if (!ShellLayout.duoMode && monitor.name !== root.modelData.name)
                continue;
            for (const ws of monitor.workspaces)
                for (const win of ws.windows)
                    all.push(win);
        }
        return all;
    }

    screen: modelData
    visible: ShellLayout.showBarOn(modelData) && windows.length > 0
    anchors.bottom: true
    implicitWidth: dockBody.width + 16
    implicitHeight: 58
    // Forbidden zone for maximized windows; released when the dock hides.
    exclusiveZone: implicitHeight
    color: "transparent"

    DockMenu {
        id: dockMenu
    }

    function openMenuFor(item, win) {
        if (dockMenu.visible && dockMenu.windowId === win.id) {
            dockMenu.dismiss();
            return;
        }
        const pos = item.mapToItem(null, item.width / 2, 0);
        dockMenu.openAt(root, pos.x, win);
    }

    Rectangle {
        id: dockBody

        anchors.horizontalCenter: parent.horizontalCenter
        anchors.bottom: parent.bottom
        anchors.bottomMargin: 0
        width: chipRow.width + 16
        height: 44
        radius: 10
        color: Theme.barBg
        border.width: 1
        border.color: Theme.line

        Row {
            id: chipRow

            anchors.centerIn: parent
            spacing: 6

            Repeater {
                model: root.windows

                delegate: Rectangle {
                    id: dockItem

                    required property var modelData

                    readonly property var entry: modelData.appId
                        ? DesktopEntries.heuristicLookup(modelData.appId)
                        : null

                    width: chip.width + 16
                    height: 32
                    radius: 8
                    color: dockItem.modelData.focused ? Theme.surfaceRaised
                         : itemArea.containsMouse ? Theme.surface
                         : "transparent"
                    border.width: 1
                    border.color: dockItem.modelData.focused ? Theme.redDim : "transparent"

                    Row {
                        id: chip

                        anchors.centerIn: parent
                        spacing: 7

                        Image {
                            anchors.verticalCenter: parent.verticalCenter
                            width: 20
                            height: 20
                            sourceSize.width: 20
                            sourceSize.height: 20
                            fillMode: Image.PreserveAspectFit
                            source: dockItem.entry && dockItem.entry.icon
                                ? Quickshell.iconPath(dockItem.entry.icon, "application-x-executable")
                                : Quickshell.iconPath("application-x-executable")
                        }

                        Text {
                            anchors.verticalCenter: parent.verticalCenter
                            text: dockItem.modelData.title || dockItem.modelData.appId || "?"
                            font.family: Theme.fontFamily
                            font.pixelSize: Theme.fontSize - 1
                            color: dockItem.modelData.focused ? Theme.text : Theme.textMuted
                            elide: Text.ElideRight
                            width: Math.min(implicitWidth, 150)
                        }
                    }

                    MouseArea {
                        id: itemArea
                        anchors.fill: parent
                        hoverEnabled: true
                        acceptedButtons: Qt.LeftButton | Qt.RightButton | Qt.MiddleButton
                        onClicked: mouse => {
                            if (mouse.button === Qt.RightButton) {
                                root.openMenuFor(dockItem, dockItem.modelData);
                            } else if (mouse.button === Qt.MiddleButton) {
                                ShojiIpc.closeWindow(dockItem.modelData.id);
                            } else {
                                ShojiIpc.activateWindow(dockItem.modelData.id);
                            }
                        }
                    }
                }
            }
        }
    }
}
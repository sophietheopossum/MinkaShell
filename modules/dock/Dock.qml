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
    // Duo mode puts the dock across the top of the ScreenPad, with the
    // MinkaMon zone and the side column below it; the general layout keeps
    // it at the bottom. DockMenu flips its gravity to match.
    anchors.top: ShellLayout.duoMode
    anchors.bottom: !ShellLayout.duoMode
    // Duo mode spans the full width of the ScreenPad; the general layout
    // stays a centred pill sized to its chips (implicitWidth below).
    anchors.left: ShellLayout.duoMode
    anchors.right: ShellLayout.duoMode
    // Sourced from chipRow, not dockBody: in duo mode dockBody's width binds
    // to this window's width, so going through it would be a binding loop.
    // Same value either way (dockBody is chipRow.width + 16).
    implicitWidth: chipRow.width + 32
    // Exactly the dock body: no padding on any edge, so the dock sits flush
    // against the bottom of the screen and maximized windows come right up to
    // its top edge. Keep this in step with dockBody's height.
    implicitHeight: 44
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
        // Duo mode: a full-width strip, squared off with a single seam along
        // its inner edge, matching the bar it replaces. General layout: a
        // rounded pill hugging its chips.
        width: ShellLayout.duoMode ? parent.width : chipRow.width + 16
        height: 44
        radius: ShellLayout.duoMode ? 0 : 10
        color: Theme.barBg
        border.width: ShellLayout.duoMode ? 0 : 1
        border.color: Theme.line

        // Seam below the strip in duo mode (the dock sits at the top there).
        Rectangle {
            visible: ShellLayout.duoMode
            anchors.left: parent.left
            anchors.right: parent.right
            anchors.bottom: parent.bottom
            height: 1
            color: Theme.line
        }

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
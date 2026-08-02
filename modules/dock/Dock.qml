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
    // Capped at the output width so a long window list overflows into the
    // scroll arrows rather than growing the dock off the side of the screen.
    implicitWidth: Math.min(chipRow.width + 32, root.modelData.width)
    // Exactly the dock body: no padding on any edge, so the dock sits flush
    // against the bottom of the screen and maximized windows come right up to
    // its top edge. Keep this in step with dockBody's height.
    implicitHeight: 44
    // Forbidden zone for maximized windows; released when the dock hides.
    exclusiveZone: implicitHeight
    color: "transparent"

    // Overflow scroll button. Both arrows stay mapped once the chips stop
    // fitting and dim at the ends instead of appearing and disappearing:
    // visibility that depended on scroll position would feed back into the
    // width it is derived from, which is a binding loop.
    component DockArrow: Rectangle {
        id: arrow

        required property int direction // -1 = left, +1 = right
        property bool canScroll: false

        signal activated

        width: 22
        height: 32
        radius: 6
        color: arrowArea.containsMouse && arrow.canScroll
             ? Theme.surfaceRaised
             : "transparent"
        opacity: arrow.canScroll ? 1.0 : 0.3

        Behavior on opacity {
            NumberAnimation { duration: 120 }
        }

        Text {
            anchors.centerIn: parent
            text: arrow.direction < 0 ? "‹" : "›"
            font.family: Theme.fontFamily
            font.pixelSize: Theme.fontSize + 6
            color: arrowArea.containsMouse && arrow.canScroll
                 ? Theme.red
                 : Theme.textMuted
        }

        MouseArea {
            id: arrowArea

            anchors.fill: parent
            hoverEnabled: true
            enabled: arrow.canScroll
            onClicked: arrow.activated()
        }
    }

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

        // Space the chips can use once the body's own padding is removed.
        readonly property real trackWidth: width - 16
        // Derived only from chipRow (its children) and this body's width, so
        // it can never depend on the scroll position or on arrow visibility.
        readonly property bool overflowing: chipRow.width > trackWidth
        // Arrow width plus the row spacing beside it.
        readonly property real arrowSlot: 26

        anchors.horizontalCenter: parent.horizontalCenter
        anchors.bottom: parent.bottom
        anchors.bottomMargin: 0
        // Duo mode: a full-width strip, squared off with a single seam along
        // its inner edge, matching the bar it replaces. General layout: a
        // rounded pill hugging its chips, capped at the output width so it
        // overflows into the arrows instead of running off the screen.
        width: ShellLayout.duoMode
             ? parent.width
             : Math.min(chipRow.width + 16, root.modelData.width - 40)
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
            id: chipArea

            anchors.centerIn: parent
            spacing: 4

            DockArrow {
                anchors.verticalCenter: parent.verticalCenter
                direction: -1
                visible: dockBody.overflowing
                canScroll: chipFlick.contentX > 0.5
                onActivated: chipFlick.scrollBy(-1)
            }

            Flickable {
                id: chipFlick

                // One arrow click moves most of a page, like Firefox's tab
                // strip; the wheel still scrolls freely.
                function scrollBy(dir) {
                    const limit = Math.max(0, contentWidth - width);
                    scrollAnim.to = Math.max(
                        0,
                        Math.min(limit, contentX + dir * width * 0.8)
                    );
                    scrollAnim.restart();
                }

                anchors.verticalCenter: parent.verticalCenter
                // Never derived from the scroll position, so the arrows can
                // read contentX without feeding back into this width.
                width: dockBody.overflowing
                     ? dockBody.trackWidth - 2 * dockBody.arrowSlot
                     : chipRow.width
                height: 32
                contentWidth: chipRow.width
                clip: true
                flickableDirection: Flickable.HorizontalFlick
                boundsBehavior: Flickable.StopAtBounds

                NumberAnimation {
                    id: scrollAnim

                    target: chipFlick
                    property: "contentX"
                    duration: 160
                    easing.type: Easing.OutCubic
                }

                Row {
                    id: chipRow

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

            DockArrow {
                anchors.verticalCenter: parent.verticalCenter
                direction: 1
                visible: dockBody.overflowing
                canScroll: chipFlick.contentX
                    < chipFlick.contentWidth - chipFlick.width - 0.5
                onActivated: chipFlick.scrollBy(1)
            }
        }
    }
}

import Quickshell
import QtQuick
import "../../services"

// Right-click menu for a dock item. Same hard-won rules as TrayMenu: the
// popup window is sized once at open and never resized while mapped, the
// transparent remainder is the click-outside backdrop, and there is no
// hover-timeout dismissal. Content is static (title + close), so this stays
// simple. Anchored to the dock window's top edge with upward gravity, so
// the menu opens above the dock, centered over the clicked item.
PopupWindow {
    id: root

    property string windowId: ""
    property string windowTitle: ""

    visible: false
    implicitWidth: 520
    implicitHeight: 200
    color: "transparent"

    // Follow the dock: it sits at the bottom in the general layout (menu
    // opens upward) and at the top of the ScreenPad in duo mode (menu opens
    // downward). Anchoring to the wrong edge would open the menu off-screen.
    anchor.edges: ShellLayout.duoMode ? Edges.Bottom : Edges.Top
    anchor.gravity: ShellLayout.duoMode ? Edges.Bottom : Edges.Top

    // itemCenterX is in the dock window's coordinates.
    function openAt(window, itemCenterX, win) {
        windowId = win.id;
        windowTitle = win.title || win.appId || "";
        anchor.window = window;
        anchor.rect.x = itemCenterX;
        anchor.rect.y = 0;
        anchor.rect.width = 1;
        anchor.rect.height = 1;
        visible = true;
    }

    function dismiss() {
        visible = false;
        windowId = "";
        windowTitle = "";
    }

    // If the target window vanishes while the menu is open (closed from
    // elsewhere), drop the menu rather than acting on a stale id.
    Connections {
        target: ShojiIpc

        function onViewChanged() {
            if (!root.visible || !ShojiIpc.view)
                return;
            for (const monitor of ShojiIpc.view.monitors)
                for (const ws of monitor.workspaces)
                    for (const win of ws.windows)
                        if (win.id === root.windowId)
                            return;
            root.dismiss();
        }
    }

    // Backdrop: a click anywhere outside the menu box closes.
    MouseArea {
        anchors.fill: parent
        onClicked: root.dismiss()
    }

    Rectangle {
        id: menuBox

        anchors.horizontalCenter: parent.horizontalCenter
        anchors.bottom: parent.bottom
        anchors.bottomMargin: 6
        width: 200
        height: menuColumn.implicitHeight + 12
        radius: 8
        color: Theme.barBg
        border.width: 1
        border.color: Theme.line

        Column {
            id: menuColumn

            anchors.left: parent.left
            anchors.right: parent.right
            anchors.top: parent.top
            anchors.margins: 6
            spacing: 4

            Text {
                width: parent.width
                leftPadding: 8
                text: root.windowTitle
                font.family: Theme.fontFamily
                font.pixelSize: Theme.fontSize - 2
                color: Theme.textMuted
                elide: Text.ElideRight
            }

            Rectangle {
                width: parent.width
                height: 1
                color: Theme.line
            }

            Rectangle {
                width: parent.width
                height: 26
                radius: 5
                color: closeArea.containsMouse ? Theme.redDim : "transparent"

                Text {
                    anchors.left: parent.left
                    anchors.leftMargin: 8
                    anchors.verticalCenter: parent.verticalCenter
                    text: "close window"
                    font.family: Theme.fontFamily
                    font.pixelSize: Theme.fontSize - 1
                    color: closeArea.containsMouse ? Theme.text : Theme.red
                }

                MouseArea {
                    id: closeArea
                    anchors.fill: parent
                    hoverEnabled: true
                    onClicked: {
                        ShojiIpc.closeWindow(root.windowId);
                        root.dismiss();
                    }
                }
            }
        }
    }
}
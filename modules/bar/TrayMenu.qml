pragma ComponentBehavior: Bound

import Quickshell
import QtQuick
import "../../services"

// Self-rendered tray menu. Qt's platform-menu path (QsMenuAnchor /
// SystemTrayItem.display) proved unreliable on ShojiWM, so we read the DBus
// menu tree through QsMenuOpener and draw it ourselves — which also keeps it
// in the Eternal-Darkness theme instead of a foreign Qt popup.
//
// Hard-won rules encoded here:
//  1. The popup window is sized ONCE at open and never resized while
//     mapped — resizing a mapped popup is compositor-hostile. It spans the
//     whole output below the bar; the visible menu box grows inside it and
//     the transparent remainder doubles as a click-outside backdrop.
//  2. The root menu handle stays pinned by a dedicated opener for as long
//     as the popup is open. quickshell destroys the ENTIRE dbusmenu tree
//     when the root handle's refcount drops to zero (dbusmenu.cpp:
//     unrefHandle -> onMenuPathChanged -> deleteLater), so a lone opener
//     that navigates from the root into a child entry frees the very entry
//     it navigated to. The submenu opener only attaches while inside a
//     submenu — two simultaneous refs on the root double-fire AboutToShow
//     and Qt's dbusmenu exporter answers each with a layout update, keeping
//     the children model in permanent churn. Openers live at window scope,
//     never inside delegates: a layout update resets the model and would
//     tear the opener down with its delegate.
//  3. No hover-timeout dismissal. Layout updates rebuild delegates, which
//     momentarily collapses the box and drops hover state, so any
//     "close when the pointer leaves" timer eventually fires while the
//     user is still in the menu. Dismissal is click-outside only.
PopupWindow {
    id: root

    property var menuHandle: null

    // Navigation stack of QsMenuEntry handles; empty = at the root menu.
    property var menuPath: []

    readonly property var currentSubmenu: menuPath.length > 0
        ? menuPath[menuPath.length - 1] : null

    readonly property var currentChildren: menuPath.length > 0
        ? subOpener.children : rootOpener.children

    property int fixedHeight: 440
    property int boxX: 0
    readonly property int boxWidth: 260

    visible: false
    implicitWidth: 800
    implicitHeight: fixedHeight
    color: "transparent"

    // x is the desired box position in the anchor window's coordinates;
    // y is where the popup's top edge goes (just below the bar).
    function openAt(window, x, y, handle) {
        menuHandle = handle;
        menuPath = [];
        anchor.window = window;
        anchor.rect.x = 0;
        anchor.rect.y = y;
        // Cover the output below the bar so the backdrop catches every
        // outside click. Sized here, before mapping, and never again.
        const screen = window.screen;
        implicitWidth = screen ? screen.width : window.width;
        fixedHeight = Math.max(120, (screen ? screen.height : 600) - y - 8);
        boxX = Math.max(8, Math.min(x, implicitWidth - boxWidth - 8));
        visible = true;
    }

    function dismiss() {
        visible = false;
        menuHandle = null;
        menuPath = [];
    }

    function enter(entry) {
        menuPath = menuPath.concat([entry]);
    }

    function back() {
        menuPath = menuPath.slice(0, -1);
    }

    // Pin: holds a ref on the root handle for the popup's lifetime so the
    // tree (and any entry the submenu opener points at) is never freed
    // mid-use. Also the model rendered while at the root level.
    QsMenuOpener {
        id: rootOpener

        menu: root.menuHandle
    }

    // Attached only while navigated into a submenu (see rule 2).
    QsMenuOpener {
        id: subOpener

        menu: root.currentSubmenu
    }

    // Backdrop: a click anywhere outside the menu box closes.
    MouseArea {
        anchors.fill: parent
        onClicked: root.dismiss()
    }

    Rectangle {
        id: menuBox

        x: root.boxX
        anchors.top: parent.top
        width: root.boxWidth
        height: Math.min(menuColumn.implicitHeight + 16, root.fixedHeight)
        radius: 8
        color: Theme.barBg
        border.width: 1
        border.color: Theme.line

        // Swallow clicks on the box chrome so they don't hit the backdrop.
        MouseArea {
            anchors.fill: parent
        }

        Flickable {
            anchors.fill: parent
            anchors.margins: 8
            contentHeight: menuColumn.implicitHeight
            clip: true
            boundsBehavior: Flickable.StopAtBounds

            Column {
                id: menuColumn

                width: parent.width
                spacing: 1

                // Back row while inside a submenu.
                Rectangle {
                    visible: root.menuPath.length > 0
                    width: parent.width
                    height: 26
                    radius: 5
                    color: backArea.containsMouse ? Theme.surfaceRaised : "transparent"

                    Row {
                        anchors.left: parent.left
                        anchors.leftMargin: 8
                        anchors.verticalCenter: parent.verticalCenter
                        spacing: 6

                        Text {
                            anchors.verticalCenter: parent.verticalCenter
                            text: "‹"
                            font.pixelSize: Theme.fontSize
                            color: Theme.red
                        }

                        Text {
                            anchors.verticalCenter: parent.verticalCenter
                            text: "back"
                            font.family: Theme.monoFamily
                            font.pixelSize: Theme.fontSize - 2
                            color: Theme.textMuted
                        }
                    }

                    MouseArea {
                        id: backArea
                        anchors.fill: parent
                        hoverEnabled: true
                        onClicked: root.back()
                    }
                }

                Rectangle {
                    visible: root.menuPath.length > 0
                    width: parent.width
                    height: 1
                    color: Theme.line
                }

                Repeater {
                    model: root.currentChildren

                    delegate: Column {
                        id: entryBlock

                        required property var modelData

                        width: menuColumn.width

                        // Separator
                        Rectangle {
                            visible: entryBlock.modelData.isSeparator
                            width: parent.width
                            height: 1
                            color: Theme.line
                        }

                        // Regular entry
                        Rectangle {
                            visible: !entryBlock.modelData.isSeparator
                            width: parent.width
                            height: 26
                            radius: 5
                            color: entryArea.containsMouse && entryBlock.modelData.enabled
                                ? Theme.surfaceRaised : "transparent"

                            Row {
                                anchors.left: parent.left
                                anchors.leftMargin: 8
                                anchors.verticalCenter: parent.verticalCenter
                                spacing: 6

                                Text {
                                    anchors.verticalCenter: parent.verticalCenter
                                    visible: entryBlock.modelData.checkState === Qt.Checked
                                    text: "✓"
                                    font.pixelSize: Theme.fontSize - 2
                                    color: Theme.red
                                }

                                Image {
                                    anchors.verticalCenter: parent.verticalCenter
                                    visible: source.toString() !== ""
                                    source: entryBlock.modelData.icon || ""
                                    width: 14
                                    height: 14
                                    sourceSize.width: 14
                                    sourceSize.height: 14
                                    fillMode: Image.PreserveAspectFit
                                }

                                Text {
                                    anchors.verticalCenter: parent.verticalCenter
                                    text: entryBlock.modelData.text
                                    font.family: Theme.fontFamily
                                    font.pixelSize: Theme.fontSize - 1
                                    color: entryBlock.modelData.enabled ? Theme.text : Theme.textFaint
                                    elide: Text.ElideRight
                                    width: Math.min(implicitWidth, menuColumn.width - 60)
                                }
                            }

                            Text {
                                anchors.right: parent.right
                                anchors.rightMargin: 8
                                anchors.verticalCenter: parent.verticalCenter
                                visible: entryBlock.modelData.hasChildren
                                text: "▸"
                                font.pixelSize: Theme.fontSize - 3
                                color: Theme.textFaint
                            }

                            MouseArea {
                                id: entryArea
                                anchors.fill: parent
                                hoverEnabled: true
                                enabled: entryBlock.modelData.enabled
                                onClicked: {
                                    if (entryBlock.modelData.hasChildren) {
                                        root.enter(entryBlock.modelData);
                                    } else {
                                        entryBlock.modelData.triggered();
                                        root.dismiss();
                                    }
                                }
                            }
                        }
                    }
                }

                Text {
                    visible: root.currentChildren === null
                        || root.currentChildren.values.length === 0
                    text: root.menuPath.length > 0 ? "empty" : "no actions"
                    font.family: Theme.monoFamily
                    font.pixelSize: Theme.fontSize - 2
                    color: Theme.textFaint
                }
            }
        }
    }
}

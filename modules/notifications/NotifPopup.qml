import Quickshell
import QtQuick
import "../../services"

// Notification popups. Rendered only on panel outputs (the ScreenPad in Duo
// mode), top-right, newest on top. Click a card to dismiss; cards expire
// after 6s. The NotificationServer itself lives in services/Notifs.qml.
PanelWindow {
    id: root

    required property var modelData

    screen: modelData
    visible: ShellLayout.showBarOn(modelData) && Notifs.tracked.values.length > 0
    anchors.top: true
    anchors.right: true
    margins.top: Theme.barHeight + 6
    margins.right: 8
    implicitWidth: 340
    implicitHeight: Math.min(cards.implicitHeight, modelData.height - Theme.barHeight - 24)
    exclusiveZone: 0
    color: "transparent"

    Column {
        id: cards

        width: parent.width
        spacing: 8

        Repeater {
            model: Notifs.tracked

            delegate: Rectangle {
                id: card

                required property var modelData

                width: cards.width
                implicitHeight: content.implicitHeight + 20
                radius: 10
                color: Theme.barBg
                border.width: 1
                border.color: cardArea.containsMouse ? Theme.red : Theme.line

                Timer {
                    interval: 6000
                    // A notification carrying actions is a QUESTION, so it waits for an answer
                    // instead of expiring — a 6s approval prompt is barely better than none.
                    // Click the card to dismiss it without choosing.
                    running: !cardArea.containsMouse && card.modelData.actions.length === 0
                    onTriggered: card.modelData.expire()
                }

                Column {
                    id: content

                    // Above cardArea, which is declared later and fills the card. z orders only
                    // among SIBLINGS, and cardArea's sibling is this Column — not the button Row
                    // inside it, which is where this used to be and ordered nothing at all.
                    // The Texts here accept no mouse events, so hover still reaches cardArea and
                    // a click anywhere except a button still dismisses.
                    z: 1

                    anchors.left: parent.left
                    anchors.right: parent.right
                    anchors.top: parent.top
                    anchors.margins: 10
                    spacing: 3

                    Row {
                        spacing: 6

                        Rectangle {
                            anchors.verticalCenter: parent.verticalCenter
                            width: 6
                            height: 6
                            radius: 3
                            color: Theme.red
                        }

                        Text {
                            text: card.modelData.appName || "notification"
                            font.family: Theme.monoFamily
                            font.pixelSize: Theme.fontSize - 3
                            font.letterSpacing: 1
                            color: Theme.textFaint
                        }
                    }

                    Text {
                        width: parent.width
                        text: card.modelData.summary
                        font.family: Theme.fontFamily
                        font.pixelSize: Theme.fontSize
                        font.weight: Font.DemiBold
                        color: Theme.text
                        wrapMode: Text.Wrap
                        maximumLineCount: 2
                        elide: Text.ElideRight
                    }

                    Text {
                        width: parent.width
                        visible: text !== ""
                        text: card.modelData.body
                        font.family: Theme.fontFamily
                        font.pixelSize: Theme.fontSize - 1
                        color: Theme.textMuted
                        wrapMode: Text.Wrap
                        maximumLineCount: 4
                        elide: Text.ElideRight
                        textFormat: Text.PlainText
                    }

                    // Action buttons. services/Notifs.qml has always set actionsSupported: true,
                    // so senders were told these would render and then never saw them — an
                    // approval prompt arrived as text with no way to answer it.
                    // z: 1 is load-bearing: the card-filling MouseArea is declared AFTER this
                    // Column, so it sits on top and would swallow every button press as a dismiss.
                    Row {
                        width: parent.width
                        spacing: 6
                        visible: card.modelData.actions.length > 0

                        Repeater {
                            model: card.modelData.actions

                            delegate: Rectangle {
                                id: btn

                                required property var modelData

                                implicitWidth: btnLabel.implicitWidth + 18
                                implicitHeight: btnLabel.implicitHeight + 10
                                radius: 6
                                color: btnArea.containsMouse ? Theme.redDim : Theme.surfaceRaised
                                border.width: 1
                                border.color: btnArea.containsMouse ? Theme.red : Theme.line

                                Text {
                                    id: btnLabel

                                    anchors.centerIn: parent
                                    text: btn.modelData.text
                                    font.family: Theme.fontFamily
                                    font.pixelSize: Theme.fontSize - 1
                                    color: Theme.text
                                    textFormat: Text.PlainText
                                }

                                MouseArea {
                                    id: btnArea

                                    anchors.fill: parent
                                    hoverEnabled: true
                                    cursorShape: Qt.PointingHandCursor
                                    onClicked: btn.modelData.invoke()
                                }
                            }
                        }
                    }
                }

                MouseArea {
                    id: cardArea
                    anchors.fill: parent
                    hoverEnabled: true
                    onClicked: card.modelData.dismiss()
                }
            }
        }
    }
}

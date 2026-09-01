import Quickshell
import Quickshell.Wayland
import QtQuick
import Quickshell.Services.Pipewire
import Quickshell.Io
import "../../services"

// Audio status menu (network is out of scope — Sophie runs CMST).
// Volume slider + mute for the default sink
// mute + slider for the default source
// Battery lives in its own popover (BatteryMenu)
PanelWindow {
    id: root

    required property var modelData

    readonly property bool open: MenuState.isOpen("status", modelData.name)
    readonly property var sink: Pipewire.defaultAudioSink
    readonly property var source: Pipewire.defaultAudioSource

    screen: modelData
    visible: open
    // Above MenuBackdrop's top layer, so the backdrop can never eat clicks
    // meant for the menu.
    WlrLayershell.layer: WlrLayer.Overlay
    anchors.top: true
    anchors.right: true
    margins.top: Theme.barHeight + 6
    margins.right: 8
    implicitWidth: 300
    implicitHeight: body.implicitHeight + 24
    exclusiveZone: 0
    color: "transparent"

    // Volume/mute properties are only valid while the nodes are bound.
    PwObjectTracker {
        objects: [root.sink, root.source].filter(node => node !== null)
    }

    // PipeWire and WirePlumber can be left holding stale device nodes after a
    // crash or an OOM kill — duplicate sinks for one output, or playback stuck
    // on the wrong device after a display is plugged in. Restarting the three
    // user units clears that without touching the session.
    Process {
        id: audioRestart
        command: ["systemctl", "--user", "restart", "wireplumber", "pipewire", "pipewire-pulse"]
    }

    component AudioControl: Column {
        id: control

        property string label
        property var node

        readonly property bool present: node !== null && node.audio !== null
        readonly property bool muted: present && node.audio.muted

        visible: present
        spacing: 6

        Row {
            spacing: 8

            Text {
                anchors.verticalCenter: parent.verticalCenter
                text: control.label
                font.family: Theme.monoFamily
                font.pixelSize: Theme.fontSize - 3
                font.letterSpacing: 1
                color: Theme.textFaint
            }

            Text {
                anchors.verticalCenter: parent.verticalCenter
                text: control.present && control.node.nickname ? control.node.nickname : ""
                font.family: Theme.fontFamily
                font.pixelSize: Theme.fontSize - 2
                color: Theme.textMuted
                elide: Text.ElideRight
                width: Math.min(implicitWidth, 170)
            }
        }

        Row {
            width: parent.width
            spacing: 10

            // Mute toggle
            Rectangle {
                anchors.verticalCenter: parent.verticalCenter
                width: 34
                height: 22
                radius: 5
                color: control.muted ? Theme.redDim : Theme.surfaceRaised
                border.width: 1
                border.color: control.muted ? Theme.red : Theme.line

                Text {
                    anchors.centerIn: parent
                    text: control.muted ? "✕" : "on"
                    font.family: Theme.monoFamily
                    font.pixelSize: Theme.fontSize - 3
                    color: control.muted ? Theme.text : Theme.textMuted
                }

                MouseArea {
                    anchors.fill: parent
                    onClicked: control.node.audio.muted = !control.node.audio.muted
                }
            }

            // Volume slider (custom: no QtQuick.Controls styling fights)
            Item {
                id: slider

                anchors.verticalCenter: parent.verticalCenter
                width: parent.width - 34 - 10 - 40 - 10
                height: 22

                readonly property real value: control.present ? Math.min(1, control.node.audio.volume) : 0

                Rectangle {
                    anchors.verticalCenter: parent.verticalCenter
                    width: parent.width
                    height: 4
                    radius: 2
                    color: Theme.surfaceRaised
                    border.width: 1
                    border.color: Theme.line
                }

                Rectangle {
                    anchors.verticalCenter: parent.verticalCenter
                    width: parent.width * slider.value
                    height: 4
                    radius: 2
                    color: control.muted ? Theme.textFaint : Theme.red
                }

                Rectangle {
                    x: Math.max(0, Math.min(parent.width - width, parent.width * slider.value - width / 2))
                    anchors.verticalCenter: parent.verticalCenter
                    width: 12
                    height: 12
                    radius: 6
                    color: sliderArea.containsMouse || sliderArea.pressed ? Theme.text : Theme.textMuted
                }

                MouseArea {
                    id: sliderArea

                    anchors.fill: parent
                    hoverEnabled: true

                    function apply(mouseX) {
                        control.node.audio.volume = Math.max(0, Math.min(1, mouseX / width));
                    }

                    onPressed: mouse => apply(mouse.x)
                    onPositionChanged: mouse => {
                        if (pressed)
                            apply(mouse.x);
                    }
                    onWheel: wheel => {
                        const step = wheel.angleDelta.y > 0 ? 0.05 : -0.05;
                        control.node.audio.volume = Math.max(0, Math.min(1, control.node.audio.volume + step));
                    }
                }
            }

            Text {
                anchors.verticalCenter: parent.verticalCenter
                width: 40
                horizontalAlignment: Text.AlignRight
                text: control.present ? Math.round(control.node.audio.volume * 100) + "%" : ""
                font.family: Theme.monoFamily
                font.pixelSize: Theme.fontSize - 2
                color: control.muted ? Theme.textFaint : Theme.text
            }
        }
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
            spacing: 14

            AudioControl {
                width: parent.width
                label: "out"
                node: root.sink
            }

            AudioControl {
                width: parent.width
                label: "mic"
                node: root.source
            }

            Text {
                visible: root.sink === null && root.source === null
                text: "no audio devices"
                font.family: Theme.monoFamily
                font.pixelSize: Theme.fontSize - 2
                color: Theme.textFaint
            }

            Rectangle {
                width: parent.width
                height: 28
                radius: 6
                color: restartArea.containsMouse ? Theme.purpleDim : Theme.surfaceRaised
                border.width: 1
                border.color: restartArea.containsMouse ? Theme.purple : Theme.line

                Text {
                    anchors.centerIn: parent
                    text: audioRestart.running ? "restarting audio…" : "restart audio"
                    font.family: Theme.monoFamily
                    font.pixelSize: Theme.fontSize - 2
                    color: restartArea.containsMouse ? Theme.text : Theme.textMuted
                }

                MouseArea {
                    id: restartArea
                    anchors.fill: parent
                    hoverEnabled: true
                    enabled: !audioRestart.running
                    onClicked: audioRestart.running = true
                }
            }
        }
    }
}

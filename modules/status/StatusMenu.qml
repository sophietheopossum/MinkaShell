pragma ComponentBehavior: Bound

import Quickshell
import Quickshell.Wayland
import QtQuick
import Quickshell.Services.Pipewire
import Quickshell.Io
import "../../services"

// Audio status menu (network is out of scope — Sophie runs CMST).
// Volume slider + mute for the default sink
// mute + slider for the default source
// mute + slider per app that is playing, and an unmute for apps WirePlumber
// remembers as muted but that are silent right now
// Battery lives in its own popover (BatteryMenu)
PanelWindow {
    id: root

    required property var modelData

    readonly property bool open: MenuState.isOpen("status", modelData.name)
    readonly property var sink: Pipewire.defaultAudioSink
    readonly property var source: Pipewire.defaultAudioSource

    readonly property string keyPrefix: "Output/Audio:"
    readonly property string notificationKey: "Output/Audio:media.role:Notification"

    // Every playback stream. Quickshell reports isSink=true for these (they
    // take audio in from the app), so match the node type, not isSink.
    // app-mute.py's short-lived stand-in streams are left out.
    readonly property var appStreams: Pipewire.nodes.values.filter(n => n.type === PwNodeType.AudioOutStream && !n.name.startsWith("minka-unmute-"))

    // Streams grouped by the key WirePlumber saves an app's volume and mute
    // under, so an app with many streams (a browser opens one per sound) is
    // one row, and a change made on it is the one WirePlumber remembers.
    readonly property var appGroups: {
        let groups = [];
        let index = {};
        for (let i = 0; i < appStreams.length; i++) {
            let node = appStreams[i];
            // properties is only valid once the tracker below has bound it
            if (!node.ready || node.audio === null)
                continue;
            let props = node.properties;
            let key = stateKey(props);
            if (key === "")
                continue;
            if (index[key] === undefined) {
                index[key] = groups.length;
                groups.push({
                    key: key,
                    name: rowTitle(key, props),
                    nodes: []
                });
            }
            groups[index[key]].nodes.push(node);
        }
        return groups;
    }

    // Apps seen playing recently: key -> { name, until }. A browser can close
    // one stream a moment before it opens the next; holding the row through
    // that gap keeps it (and a drag on its slider) from vanishing, coming
    // back at the bottom, or flipping to "not playing" in between.
    property var recentApps: ({})
    readonly property int holdMs: 2500

    // The rows drawn: everything seen recently, in a stable name order.
    readonly property var appRows: {
        let rows = [];
        for (let key in recentApps)
            rows.push({
                key: key,
                name: recentApps[key].name
            });
        rows.sort((a, b) => {
            let x = a.name.toLowerCase();
            let y = b.name.toLowerCase();
            return x < y ? -1 : x > y ? 1 : a.key < b.key ? -1 : a.key > b.key ? 1 : 0;
        });
        return rows;
    }

    // WirePlumber's saved app states (scripts/app-mute.py list), and the ones
    // that are muted with nothing playing: those have no stream to click, so
    // they get their own row.
    property var savedApps: []
    property bool listFresh: false
    readonly property var idleMuted: savedApps.filter(a => a.mute && recentApps[a.key] === undefined)

    // Shown once, after this opening's list has been read and every stream
    // has been bound (until then a playing app would look idle), then left
    // up so later stream churn cannot make it blink.
    property bool idleShown: false
    readonly property bool idleReady: open && listFresh && appStreams.every(n => n.ready)
    onIdleReadyChanged: {
        if (idleReady)
            idleShown = true;
    }

    readonly property string muteHelper: Quickshell.shellPath("scripts/app-mute.py")
    property string unmuteKey: ""
    property string unmuteNote: ""

    // Leaves room for the dock's exclusive zone on the ScreenPad (515 px
    // tall), so the list scrolls instead of the menu running off the screen.
    readonly property real appsMaxHeight: Math.max(120, modelData.height - (Theme.barHeight + 6) - 24 - devicesHeight - 56)
    readonly property real devicesHeight: (outControl.visible ? outControl.height + body.spacing : 0) + (micControl.visible ? micControl.height + body.spacing : 0) + (noDevices.visible ? noDevices.height + body.spacing : 0) + restartButton.height + body.spacing

    // Mirrors formKey() in WirePlumber's scripts/node/state-stream.lua,
    // including Lua's truthiness: an empty string still counts as present.
    function stateKey(props) {
        if (!props)
            return "";
        if (props["media.role"] === "Notification")
            return notificationKey;
        let fields = ["application.id", "application.name", "media.name", "node.name"];
        for (let i = 0; i < fields.length; i++) {
            let value = props[fields[i]];
            if (value !== undefined && value !== null)
                return keyPrefix + fields[i] + ":" + value;
        }
        return "";
    }

    // Named after what the key covers, not whichever stream came first: a
    // shared key (Notifications, a generic media.name) is every app using it.
    function rowTitle(key, props) {
        if (key === notificationKey)
            return "Notifications";
        let rest = key.substring(keyPrefix.length);
        let colon = rest.indexOf(":");
        let kind = rest.substring(0, colon);
        let value = rest.substring(colon + 1);
        if (kind === "application.id" && props["application.name"])
            return props["application.name"];
        return value;
    }

    function groupFor(key) {
        for (let i = 0; i < appGroups.length; i++) {
            if (appGroups[i].key === key)
                return appGroups[i];
        }
        return null;
    }

    function rowName(key) {
        let entry = recentApps[key];
        return entry ? entry.name : "";
    }

    // The programs behind the row when its name doesn't say (Discord's voice
    // streams call themselves "WEBRTC VoiceEngine"), what is playing, and how
    // many streams are open when that is more than one.
    function appDetail(group) {
        if (!group || group.nodes.length === 0)
            return "";
        let parts = [];
        let seen = {};
        for (let i = 0; i < group.nodes.length; i++) {
            let props = group.nodes[i].properties;
            // Native PipeWire clients (HyperCat) may name only their node.
            let binary = props["application.process.binary"] || props["node.name"] || "";
            if (binary !== "" && binary.toLowerCase() !== group.name.toLowerCase() && !seen[binary]) {
                seen[binary] = true;
                parts.push(binary);
            }
        }
        let playing = group.nodes[0].properties["media.name"] || "";
        if (playing !== "" && playing !== group.name)
            parts.push(playing);
        if (group.nodes.length > 1)
            parts.push(group.nodes.length + " streams");
        return parts.join(" · ");
    }

    function savedMute(key) {
        for (let i = 0; i < savedApps.length; i++) {
            if (savedApps[i].key === key)
                return savedApps[i].mute;
        }
        return false;
    }

    // Reflect a mute click at once; the re-read of WirePlumber's file that
    // confirms it lands a moment later.
    function noteSavedMute(key, value) {
        savedApps = savedApps.map(a => a.key === key ? Object.assign({}, a, {
            mute: value
        }) : a);
    }

    // Adds every live app, keeps a stopped one until its hold runs out, and
    // reassigns recentApps (redrawing the list) only when the set changed.
    function noteLiveApps() {
        let now = Date.now();
        let next = {};
        let changed = false;
        for (let key in recentApps) {
            if (groupFor(key) === null && recentApps[key].until <= now) {
                changed = true;
                continue;
            }
            next[key] = recentApps[key];
        }
        for (let i = 0; i < appGroups.length; i++) {
            let group = appGroups[i];
            let entry = next[group.key];
            if (entry === undefined || entry.name !== group.name)
                changed = true;
            next[group.key] = {
                name: group.name,
                until: now + holdMs
            };
        }
        if (changed)
            recentApps = next;
        else
            for (let key in next)
                recentApps[key].until = next[key].until;
    }

    function unmuteSaved(key) {
        if (unmuteProc.running)
            return;
        unmuteKey = key;
        unmuteNote = "";
        unmuteProc.running = true;
    }

    function idleStatus(app, busy, note) {
        if (busy)
            return "unmuting…";
        if (note !== "")
            return note;
        if (app.block === "stale")
            return "muted · old entry, nothing uses it now";
        if (app.block === "while-playing")
            return "muted · unmute it while it plays";
        return "muted · not playing";
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
    implicitWidth: 300
    implicitHeight: body.implicitHeight + 24
    exclusiveZone: 0
    color: "transparent"

    onAppGroupsChanged: noteLiveApps()

    onOpenChanged: {
        if (open) {
            savedList.running = true;
        } else {
            // Nothing stale may greet the next opening.
            savedApps = [];
            listFresh = false;
            idleShown = false;
            unmuteNote = "";
        }
    }

    // Volume/mute properties are only valid while the nodes are bound.
    PwObjectTracker {
        objects: [root.sink, root.source].filter(node => node !== null)
    }

    // App streams are bound only while the menu is open: a browser can open
    // and close a stream every second, and nothing needs them otherwise.
    PwObjectTracker {
        objects: root.open ? root.appStreams : []
    }

    // Expires held rows once their app has really stopped.
    Timer {
        interval: 500
        repeat: true
        running: root.open
        onTriggered: root.noteLiveApps()
    }

    // PipeWire and WirePlumber can be left holding stale device nodes after a
    // crash or an OOM kill — duplicate sinks for one output, or playback stuck
    // on the wrong device after a display is plugged in. Restarting the three
    // user units clears that without touching the session.
    Process {
        id: audioRestart
        command: ["systemctl", "--user", "restart", "wireplumber", "pipewire", "pipewire-pulse"]
    }

    Process {
        id: savedList

        command: ["python3", root.muteHelper, "list"]
        stdout: StdioCollector {
            onStreamFinished: {
                try {
                    root.savedApps = JSON.parse(this.text).apps || [];
                } catch (e) {
                    root.savedApps = [];
                }
                root.listFresh = true;
            }
        }
    }

    // Takes ~1.5 s: it waits for WirePlumber to restore the mute onto a
    // stand-in stream, unmutes that, then waits for the save to land.
    Process {
        id: unmuteProc

        command: ["python3", root.muteHelper, "unmute", root.unmuteKey]
        stdout: StdioCollector {
            onStreamFinished: {
                try {
                    let reply = JSON.parse(this.text);
                    root.unmuteNote = reply.ok ? "" : reply.message;
                } catch (e) {
                    root.unmuteNote = "unmute failed";
                }
            }
        }
        onExited: savedList.running = true
    }

    // WirePlumber rewrites its state file whenever a saved volume or mute
    // changes, including ones made from other mixers. It can do that about
    // once a second while a browser churns streams, so re-read on a short
    // delay, and only while the menu is showing.
    FileView {
        id: stateFile

        path: (Quickshell.env("XDG_STATE_HOME") || Quickshell.env("HOME") + "/.local/state") + "/wireplumber/stream-properties"
        watchChanges: true
        onFileChanged: {
            reload();
            stateRefresh.restart();
        }
    }

    Timer {
        id: stateRefresh

        interval: 300
        onTriggered: {
            if (root.open)
                savedList.running = true;
        }
    }

    component MuteToggle: Rectangle {
        id: toggle

        property bool muted
        property bool busy
        signal clicked

        width: 34
        height: 22
        radius: 5
        color: toggle.muted ? Theme.redDim : Theme.surfaceRaised
        border.width: 1
        border.color: toggle.muted ? Theme.red : Theme.line
        opacity: toggle.enabled ? 1 : 0.5

        Text {
            anchors.centerIn: parent
            text: toggle.busy ? "…" : toggle.muted ? "✕" : "on"
            font.family: Theme.monoFamily
            font.pixelSize: Theme.fontSize - 3
            color: toggle.muted ? Theme.text : Theme.textMuted
        }

        MouseArea {
            anchors.fill: parent
            enabled: toggle.enabled && !toggle.busy
            onClicked: toggle.clicked()
        }
    }

    component AudioControl: Column {
        id: control

        // Short mono tag ("out", "mic"); empty for app rows.
        property string label
        property string title
        property string detail
        // Everything this row controls: one device, or all of an app's streams.
        property var nodes: []
        // An app row kept on screen through a gap between two of its
        // streams: drawn with the last values seen, not clickable.
        property bool held: false
        // The app's saved state is muted, so a stream that has just opened
        // is about to be muted by WirePlumber even if it isn't yet.
        property bool forceMuted: false
        signal muteSet(bool value)

        readonly property var lead: nodes.length > 0 ? nodes[0] : null
        readonly property bool present: lead !== null && lead.audio !== null
        // Muted if ANY stream is: a click then unmutes them all, rather than
        // "on" hiding a stream that is still silent.
        readonly property bool muted: present ? (forceMuted || nodes.some(n => n.audio !== null && n.audio.muted)) : lastMuted
        readonly property real volume: present ? lead.audio.volume : lastVolume
        property bool lastMuted: false
        property real lastVolume: 0

        onMutedChanged: {
            if (present)
                lastMuted = muted;
        }
        onVolumeChanged: {
            if (present)
                lastVolume = volume;
        }

        function setMuted(value) {
            for (let i = 0; i < nodes.length; i++) {
                if (nodes[i].audio !== null)
                    nodes[i].audio.muted = value;
            }
            control.muteSet(value);
        }

        // WirePlumber saves each stream's whole state when its volume moves,
        // mute included, so bring every stream to the row's mute first:
        // otherwise the last stream it processes decides the app's mute.
        function setVolume(value) {
            let clamped = Math.max(0, Math.min(1, value));
            for (let i = 0; i < nodes.length; i++) {
                let audio = nodes[i].audio;
                if (audio === null)
                    continue;
                if (audio.muted !== control.muted)
                    audio.muted = control.muted;
                audio.volume = clamped;
            }
        }

        visible: present || held
        enabled: present
        opacity: present ? 1 : 0.6
        spacing: 6

        Row {
            width: parent.width
            spacing: 8

            Text {
                id: tagText

                anchors.verticalCenter: parent.verticalCenter
                visible: control.label !== ""
                text: control.label
                font.family: Theme.monoFamily
                font.pixelSize: Theme.fontSize - 3
                font.letterSpacing: 1
                color: Theme.textFaint
            }

            Text {
                id: titleText

                anchors.verticalCenter: parent.verticalCenter
                text: control.title
                font.family: Theme.fontFamily
                font.pixelSize: Theme.fontSize - 2
                color: Theme.textMuted
                elide: Text.ElideRight
                width: Math.min(implicitWidth, control.detail !== "" ? 130 : 220)
            }

            Text {
                anchors.verticalCenter: parent.verticalCenter
                visible: control.detail !== ""
                text: control.detail
                font.family: Theme.fontFamily
                font.pixelSize: Theme.fontSize - 3
                color: Theme.textFaint
                elide: Text.ElideRight
                width: Math.max(0, parent.width - titleText.width - (tagText.visible ? tagText.width + parent.spacing : 0) - parent.spacing)
            }
        }

        Row {
            width: parent.width
            spacing: 10

            MuteToggle {
                anchors.verticalCenter: parent.verticalCenter
                muted: control.muted
                onClicked: control.setMuted(!control.muted)
            }

            // Volume slider (custom: no QtQuick.Controls styling fights)
            Item {
                id: slider

                anchors.verticalCenter: parent.verticalCenter
                width: parent.width - 34 - 10 - 40 - 10
                height: 22

                readonly property real value: Math.min(1, control.volume)

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
                    // Inside the scrolling app list: keep a sideways drag
                    // from turning into a scroll.
                    preventStealing: true

                    function apply(mouseX) {
                        control.setVolume(mouseX / width);
                    }

                    onPressed: mouse => apply(mouse.x)
                    onPositionChanged: mouse => {
                        if (pressed)
                            apply(mouse.x);
                    }
                    onWheel: wheel => {
                        const step = wheel.angleDelta.y > 0 ? 0.05 : -0.05;
                        control.setVolume(control.volume + step);
                    }
                }
            }

            Text {
                anchors.verticalCenter: parent.verticalCenter
                width: 40
                horizontalAlignment: Text.AlignRight
                text: control.present || control.held ? Math.round(control.volume * 100) + "%" : ""
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
                id: outControl

                width: parent.width
                label: "out"
                title: root.sink && root.sink.nickname ? root.sink.nickname : ""
                nodes: root.sink ? [root.sink] : []
            }

            AudioControl {
                id: micControl

                width: parent.width
                label: "mic"
                title: root.source && root.source.nickname ? root.source.nickname : ""
                nodes: root.source ? [root.source] : []
            }

            Text {
                id: noDevices

                visible: root.sink === null && root.source === null
                text: "no audio devices"
                font.family: Theme.monoFamily
                font.pixelSize: Theme.fontSize - 2
                color: Theme.textFaint
            }

            // Above the app list, so rows coming and going never move it
            // under the pointer.
            Rectangle {
                id: restartButton

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

            Flickable {
                id: appsView

                width: parent.width
                height: Math.min(appsColumn.implicitHeight, root.appsMaxHeight)
                contentHeight: appsColumn.implicitHeight
                clip: true
                interactive: contentHeight > height
                boundsBehavior: Flickable.StopAtBounds
                visible: root.appRows.length > 0 || (root.idleShown && root.idleMuted.length > 0)

                Column {
                    id: appsColumn

                    width: appsView.width
                    spacing: 10

                    Text {
                        text: "apps"
                        font.family: Theme.monoFamily
                        font.pixelSize: Theme.fontSize - 3
                        font.letterSpacing: 1
                        color: Theme.textFaint
                    }

                    // Keyed on the WirePlumber key, so a row (and a drag on
                    // its slider) survives the app swapping one stream for
                    // another.
                    Repeater {
                        model: ScriptModel {
                            values: root.appRows
                            objectProp: "key"
                        }

                        delegate: AudioControl {
                            id: appRow

                            required property var modelData
                            readonly property var group: root.groupFor(modelData.key)

                            width: appsColumn.width
                            title: root.rowName(modelData.key)
                            detail: group ? root.appDetail(group) : "between streams"
                            nodes: group ? group.nodes : []
                            held: group === null
                            forceMuted: root.savedMute(modelData.key)
                            onMuteSet: value => root.noteSavedMute(appRow.modelData.key, value)
                        }
                    }

                    Repeater {
                        model: ScriptModel {
                            values: root.idleShown ? root.idleMuted : []
                            objectProp: "key"
                        }

                        delegate: Row {
                            id: idleRow

                            required property var modelData
                            readonly property bool busy: unmuteProc.running && root.unmuteKey === modelData.key
                            readonly property string note: !busy && root.unmuteKey === modelData.key ? root.unmuteNote : ""

                            width: appsColumn.width
                            spacing: 10

                            MuteToggle {
                                anchors.verticalCenter: parent.verticalCenter
                                muted: true
                                busy: idleRow.busy
                                enabled: idleRow.modelData.canUnmute && !unmuteProc.running
                                onClicked: root.unmuteSaved(idleRow.modelData.key)
                            }

                            Column {
                                anchors.verticalCenter: parent.verticalCenter
                                width: parent.width - 34 - parent.spacing
                                spacing: 2

                                Text {
                                    width: parent.width
                                    text: idleRow.modelData.key === root.notificationKey ? "Notifications" : idleRow.modelData.name
                                    font.family: Theme.fontFamily
                                    font.pixelSize: Theme.fontSize - 2
                                    color: Theme.textMuted
                                    elide: Text.ElideRight
                                }

                                Text {
                                    width: parent.width
                                    text: root.idleStatus(idleRow.modelData, idleRow.busy, idleRow.note)
                                    font.family: Theme.fontFamily
                                    font.pixelSize: Theme.fontSize - 3
                                    color: idleRow.note !== "" ? Theme.red : Theme.textFaint
                                    elide: Text.ElideRight
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

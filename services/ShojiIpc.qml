pragma Singleton
import Quickshell
import QtQuick
// Through the config-root symlink: Quickshell only honours qmldir
// singleton registration for paths inside the shell root.
import "../MinkaLink"

// MinkaShell's view of the ShojiWM IPC: state mirroring and shell-specific
// helpers layered on MinkaLink's ShojiClient, which owns the socket,
// reconnect-forever and request/response correlation (and keeps design
// rule R1: never block the render loop).
Singleton {
    id: root

    // True once the first workspaces.get response has landed on the current
    // connection. The bar shows a health dot off this.
    property bool ready: false

    // Reactive state mirrored from broadcasts (same shape shoji-bar-2's
    // workspaceState.ts consumed).
    property var view: null                 // workspaces.changed / workspaces.get
    property var dockProximity: ({})        // connector -> bool
    property var snapPreview: ({})          // connector -> rect|null (moves to minka-fx in M3)

    // ui.* broadcasts (start menu toggles etc.) — wired up by M2 modules.
    signal uiEvent(string event, var payload)

    // Request with an id-correlated response. `onResult(result, error)` is
    // optional.
    function request(method, params, onResult) {
        ShojiClient.request(method, params, onResult);
    }

    // Fire-and-forget command.
    function send(method, params) {
        ShojiClient.send(method, params);
    }

    function activateWorkspace(monitorName, index) {
        send("workspaces.activate", { monitor: monitorName, index: index });
    }

    function toggleTiling(monitorName) {
        send("workspaces.toggleTiling", { monitor: monitorName });
    }

    function activateWindow(windowId) {
        send("windows.activate", { windowId: windowId });
    }

    function closeWindow(windowId) {
        send("windows.close", { windowId: windowId });
    }

    // Dock drag-to-reorder: place `windowId` directly before `beforeId` in its
    // workspace's window order (null = last). onResult(result, error) gets
    // {ok, changed}, or "unknown method: windows.reorder" from a ShojiWM
    // config that predates it (Super+Shift+R loads it).
    function reorderWindow(windowId, beforeId, onResult) {
        request("windows.reorder", { windowId: windowId, beforeId: beforeId }, onResult);
    }

    function minimizeWindow(windowId) {
        send("windows.minimize", { windowId: windowId });
    }

    function maximizeWindow(windowId, maximized) {
        send("windows.maximize", { windowId: windowId, maximized: maximized });
    }

    // Focused window for a bar on `monitorName`: per-monitor in the general
    // layout, session-wide in Duo mode (the single ScreenPad bar speaks for
    // every monitor).
    function focusedWindowFor(monitorName) {
        const monitors = ShellLayout.duoMode
            ? (root.view ? root.view.monitors : [])
            : [monitorView(monitorName)];
        for (const monitor of monitors) {
            if (!monitor)
                continue;
            for (const workspace of monitor.workspaces)
                for (const win of workspace.windows)
                    if (win.focused)
                        return win;
        }
        return null;
    }

    function monitorView(connectorName) {
        const v = root.view;
        if (!v)
            return null;
        const matched = v.monitors.find(m => m.name === connectorName);
        if (matched)
            return matched;
        // Unknown connector: fall back like shoji-bar-2 does.
        return v.monitors.find(m => m.name === v.currentMonitor) ?? v.monitors[0] ?? null;
    }

    function refresh() {
        ShojiClient.request("workspaces.get", undefined, (result, error) => {
            if (result !== undefined && result !== null) {
                root.view = result;
                root.ready = true;
            }
        });
    }

    Connections {
        target: ShojiClient

        function onBroadcast(name, payload) {
            if (name === "workspaces.changed") {
                root.view = payload;
                root.ready = true;
            } else if (name === "dock.proximity") {
                const next = Object.assign({}, root.dockProximity);
                next[payload.monitor] = payload.inside;
                root.dockProximity = next;
            } else if (name === "snap.preview") {
                const next = Object.assign({}, root.snapPreview);
                next[payload.monitor] = payload.rect
                    ? Object.assign({ kind: payload.kind }, payload.rect)
                    : null;
                root.snapPreview = next;
            } else if (name.startsWith("ui.")) {
                root.uiEvent(name, payload);
            }
        }

        function onConnectedChanged() {
            if (ShojiClient.connected)
                root.refresh();
            else
                root.ready = false;
        }
    }

    // The client may already be up before our Connections attach (singleton
    // instantiation order isn't guaranteed).
    Component.onCompleted: {
        if (ShojiClient.connected)
            refresh();
    }
}
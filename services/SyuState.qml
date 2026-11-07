pragma Singleton
import QtQuick
import Quickshell
import Quickshell.Io

// MinkaSyu's state file, watched.
//
// The core writes ~/.local/state/minka-syu/state.json atomically on
// every check (timer or manual); the bar indicator is a view over it.
// A missing or malformed file means "no data" -- count zero, indicator
// hidden -- rather than an error anywhere: MinkaSyu is optional
// furniture, and the shell must not care whether it is installed.
Singleton {
    id: root

    property int count: 0
    property int repoChanges: 0
    property int checkedAt: 0      // epoch seconds, 0 = never checked

    readonly property string statePath: {
        const override = Quickshell.env("MINKA_SYU_STATE");
        if (override)
            return override;
        const xdg = Quickshell.env("XDG_STATE_HOME");
        const base = xdg ? xdg : Quickshell.env("HOME") + "/.local/state";
        return base + "/minka-syu/state.json";
    }

    FileView {
        id: stateFile
        path: root.statePath
        watchChanges: true
        onFileChanged: reload()
        onLoadFailed: {
            root.count = 0;
            root.repoChanges = 0;
            root.checkedAt = 0;
        }
        onLoaded: {
            try {
                const data = JSON.parse(stateFile.text());
                root.count = Array.isArray(data.updates) ? data.updates.length : 0;
                root.repoChanges = typeof data.repo_change_count === "number" ? data.repo_change_count : 0;
                root.checkedAt = typeof data.checked_at === "number" ? data.checked_at : 0;
            } catch (e) {
                root.count = 0;
                root.repoChanges = 0;
                root.checkedAt = 0;
            }
        }
    }
}

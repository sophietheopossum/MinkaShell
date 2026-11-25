# MinkaShell

The Minka session shell, an AntiWebUI application and part of MinkaDE's one
binary, `minka` (it runs as `minka minkashell`). It replaced the Quickshell
shell, which is gone (24/11/2026); the git history has it.

In the Duo layout it is a dock across the ScreenPad and a 400 px side column
beside it; elsewhere a top bar and a dock. It carries the start menu, the
clock and calendar, battery, the audio status menu (with per-app mute
through `scripts/app-mute.py`), the tray, the updates pill that opens
MinkaSyu, the IPC health dot, and the session's notification server.

## Running

The compositor config starts `dist/minkashell <this directory>` once per
session. The launcher runs `$MINKA_BIN`, else `/usr/bin/minka`, else a build
in this checkout, appends the output to `~/.local/state/minka/minkashell.log`
(rolled at 2 MB), and starts the shell again when it dies: after 1 s, then
2, 4, 8 and 16, then every 30 s, and after 1 s again once a run has lasted a
minute. It stops when the shell exits cleanly or the compositor is gone.

- To try a build: `cargo build --profile release-fast -p minka` in the
  MinkaDE checkout, then set `MINKA_BIN` to `target/release-fast/minka` for
  the launcher.
- To stop the shell, signal the launcher, not the binary: a killed binary is
  started again. To put a new build live, signal the binary: the launcher
  starts the new one.
- `MINKA_SHELL_DIR` (set in `shojiwm-env.fish`) points the session at this
  checkout instead of `/usr/share/minka/MinkaShell`; the shell reads
  `assets/` and `scripts/` from there.

## Layout

```
src/lib.rs            entry point: surfaces per output, layout rules
src/bar.rs            the top bar and its widgets
src/sidepanel.rs      the Duo side column, built from the bar's widgets
src/dock.rs           window chips, drag to reorder
src/menus.rs          dropdown menus, the start menu, the click-away backdrop
src/status.rs         audio status menu: default sink and source, per-app volume
src/notifications.rs  notification popups (org.freedesktop.Notifications)
src/ipc.rs            ShojiWM IPC: the workspace view model and commands
src/state.rs          shared state: IPC view, menu routing, settings, updates
src/settings.rs       ~/.config/minka-settings.json, MinkaSyu state, layouts
src/theme.rs          Proustite tokens and the shell's own
src/tray_rescue.rs    keeps the tray's icons across a restart of the shell
assets/               window-button and other icons
scripts/app-mute.py   per-application mute
dist/minkashell       the launcher
```

## IPC (served by the Minka config)

Requests: `workspaces.get`, `workspaces.activate`, `workspaces.toggleTiling`,
`windows.activate`, `windows.close`, `windows.minimize`, `windows.maximize`,
`windows.reorder`. Broadcasts consumed: `workspaces.changed`,
`ui.startMenu` (Super+A or a Super tap).

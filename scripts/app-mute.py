#!/usr/bin/env python3
"""Read and clear WirePlumber's saved per-app mutes.

    app-mute.py list          saved playback-app states, as one JSON line
    app-mute.py unmute KEY    clear the saved mute for KEY, e.g.
                              "Output/Audio:application.name:Floorp"

WirePlumber remembers volume and mute per app (keyed by application.id, else
application.name, ...) in ~/.local/state/wireplumber/stream-properties, and
applies them to every new stream the app opens. Muting any one stream in a
mixer therefore mutes the whole app, across restarts.

The catch is undoing it: WirePlumber only saves from a LIVE stream, so an app
that is muted and silent has nothing to click. `unmute` gets round that by
opening a short, silent stand-in stream under the app's identity. WirePlumber
restores the saved mute onto it, we unmute it, WirePlumber saves that for the
app, and the stand-in goes away. Nothing is written to WirePlumber's file
directly: it keeps the state in memory and would overwrite any edit.
"""

import ctypes
import json
import os
import signal
import subprocess
import sys
import time

STATE = os.path.join(
    os.environ.get("XDG_STATE_HOME") or os.path.expanduser("~/.local/state"),
    "wireplumber",
    "stream-properties",
)
PREFIX = "Output/Audio:"
NOTIFICATION_KEY = PREFIX + "media.role:Notification"

# Two backstops so a stand-in never outlives the helper: the kernel kills it
# when the helper dies (see _die_with_parent), and it stops by itself after
# this many frames (20 s at 48 kHz), longer than the helper can run.
STAND_IN_FRAMES = 20 * 48000

PR_SET_PDEATHSIG = 1


def unescape_value(value):
    """Undo GKeyFile value escaping (\\s \\n \\t \\r \\\\)."""
    out, i = [], 0
    while i < len(value):
        c = value[i]
        if c == "\\" and i + 1 < len(value):
            nxt = value[i + 1]
            out.append({"s": " ", "n": "\n", "t": "\t", "r": "\r", "\\": "\\"}.get(nxt, "\\" + nxt))
            i += 2
            continue
        out.append(c)
        i += 1
    return "".join(out)


def unescape_key(key):
    """Undo WirePlumber's own key escaping (wp/state.c escape_string), which
    it applies before GKeyFile sees the key: \\\\ \\s \\e \\o \\c stand for
    backslash, space, '=', '[' and ']'. "Playback Stream" is stored as
    "Playback\\sStream"."""
    out, i = [], 0
    while i < len(key):
        c = key[i]
        if c == "\\" and i + 1 < len(key):
            out.append({"\\": "\\", "s": " ", "e": "=", "o": "[", "c": "]"}.get(key[i + 1], "\\"))
            i += 2
            continue
        out.append(c)
        i += 1
    return "".join(out)


def read_state():
    """Every Output/Audio entry as {unescaped-key: stored-props-dict}."""
    entries = {}
    try:
        with open(STATE, encoding="utf-8", errors="replace") as f:
            section = None
            for line in f:
                line = line.rstrip("\n")
                if line.startswith("["):
                    section = line.strip()
                    continue
                if section != "[stream-properties]" or "=" not in line:
                    continue
                key, _, raw = line.partition("=")
                if not key.startswith(PREFIX):
                    continue
                try:
                    props = json.loads(unescape_value(raw))
                except ValueError:
                    continue
                if isinstance(props, dict):
                    entries[unescape_key(key)] = props
    except FileNotFoundError:
        pass
    return entries


def split_key(key):
    """"Output/Audio:application.name:Floorp" -> ("application.name", "Floorp")."""
    kind, _, value = key[len(PREFIX):].partition(":")
    return kind, value


def form_key(props):
    """The key WirePlumber stores a playback stream's state under: formKey()
    in scripts/node/state-stream.lua. Lua treats "" as present, so do we."""
    if props.get("media.role") == "Notification":
        return NOTIFICATION_KEY
    for k in ("application.id", "application.name", "media.name", "node.name"):
        v = props.get(k)
        if v is not None:
            # pw-dump turns "true"/"false" into JSON booleans; put the text back.
            # (It also turns numeric-looking text into numbers, which cannot be
            # reversed exactly; an app named "007" would not match here.)
            if isinstance(v, bool):
                v = "true" if v else "false"
            return PREFIX + k + ":" + str(v)
    return None


def unmute_block(key):
    """Why KEY cannot be unmuted with a stand-in, or "" when it can.

    A stand-in reproduces a key only if WirePlumber would pick the same
    property for it. pw-cat always carries application.name, so it can pose as
    an application.id or application.name key, and with --media-role as the
    Notification key; WirePlumber 0.5 forms no other media.role key at all."""
    kind, _ = split_key(key)
    if kind in ("application.id", "application.name") or key == NOTIFICATION_KEY:
        return ""
    if kind == "media.role":
        return "stale"          # left by an older WirePlumber; nothing uses it
    return "while-playing"      # media.name / node.name: needs the real stream


def cmd_list():
    apps = []
    for key, props in read_state().items():
        kind, value = split_key(key)
        block = unmute_block(key)
        apps.append({
            "key": key,
            "kind": kind,
            "name": value,
            "mute": props.get("mute") is True,
            "volume": props.get("volume", 1.0),
            "canUnmute": block == "",
            "block": block,
        })
    apps.sort(key=lambda a: a["name"].lower())
    print(json.dumps({"apps": apps}))
    return 0


def dump():
    try:
        return json.loads(subprocess.run(
            ["pw-dump"], capture_output=True, text=True, timeout=5).stdout or "[]")
    except (subprocess.SubprocessError, ValueError):
        return []


def props_of(obj):
    return (obj.get("info") or {}).get("props") or {}


def node_muted(obj):
    params = (obj.get("info") or {}).get("params") or {}
    for p in params.get("Props") or []:
        if "mute" in p:
            return p["mute"] is True
    return None


def find_node(node_name):
    """The stand-in's node, or None while it is not up yet."""
    for obj in dump():
        if props_of(obj).get("node.name") == node_name:
            return obj
    return None


def set_mute(node_id, muted):
    try:
        subprocess.run(["wpctl", "set-mute", str(node_id), "1" if muted else "0"],
                       capture_output=True, timeout=5)
    except subprocess.SubprocessError:
        pass


def unmute_live_streams(key, skip_name):
    """Unmute the app's own streams that WirePlumber restored as muted while
    the stand-in was running: they took the saved mute before our unmute was
    stored, and restore only ever happens when a stream is created. Returns
    how many were still muted."""
    muted = 0
    for obj in dump():
        props = props_of(obj)
        if (props.get("media.class") == "Stream/Output/Audio"
                and props.get("node.name") != skip_name
                and form_key(props) == key
                and node_muted(obj)):
            muted += 1
            set_mute(obj["id"], False)
    return muted


def _die_with_parent(parent):
    def arm():
        # Runs in the child between fork and exec: have the kernel SIGTERM the
        # stand-in when this helper dies, even by SIGKILL (Quickshell kills its
        # Process children that way on a live reload), when no `finally` runs.
        # The child inherits Python's SIGTERM handler until exec, which would
        # only note the signal and lose it; take the default action instead.
        signal.signal(signal.SIGTERM, signal.SIG_DFL)
        ctypes.CDLL(None, use_errno=True).prctl(PR_SET_PDEATHSIG, signal.SIGTERM)
        # The helper may already have died between fork and prctl, in which
        # case no death signal will ever come.
        if os.getppid() != parent:
            os._exit(1)
    return arm


def result(ok, key, message):
    print(json.dumps({"ok": ok, "key": key, "message": message}))
    return 0 if ok else 1


def cmd_unmute(key):
    entries = read_state()
    if key not in entries:
        return result(False, key, "no saved state for this app")
    props = entries[key]
    if props.get("mute") is not True:
        return result(True, key, "already unmuted")
    block = unmute_block(key)
    if block == "stale":
        return result(False, key, "stale entry: nothing uses this key any more")
    if block:
        return result(False, key, "can only be unmuted while it is playing")

    kind, value = split_key(key)
    node_name = "minka-unmute-%d" % os.getpid()
    stream_props = {"node.name": node_name, "media.name": "MinkaShell unmute"}
    cmd = ["pw-cat", "--playback", "--raw", "--sample-count", str(STAND_IN_FRAMES)]
    if key == NOTIFICATION_KEY:
        cmd += ["--media-role", "Notification"]
    else:
        stream_props[kind] = value
    # pw-cat sets the stream's volume once it starts, which WirePlumber would
    # then save for the app. Handing it the app's own saved volume makes that
    # write a no-op, and matching the saved channel layout keeps the saved
    # per-channel volumes from being rewritten for a different layout.
    channel_map = props.get("channelMap")
    if isinstance(channel_map, list) and channel_map and all(isinstance(c, str) for c in channel_map):
        cmd += ["--channels", str(len(channel_map)), "--channel-map", ",".join(channel_map)]
    volume = props.get("volume")
    if isinstance(volume, (int, float)) and volume >= 0:
        cmd += ["--volume", "%.6f" % volume]
    cmd += ["--properties", json.dumps(stream_props), "/dev/zero"]

    proc = subprocess.Popen(cmd, stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL,
                            stderr=subprocess.DEVNULL, preexec_fn=_die_with_parent(os.getpid()))
    try:
        # Wait for WirePlumber to restore the saved mute onto the stand-in.
        # Unmuting before that lands would be overwritten by the restore.
        node, deadline = None, time.monotonic() + 5
        while time.monotonic() < deadline:
            if proc.poll() is not None:
                return result(False, key, "pw-cat exited early")
            node = find_node(node_name)
            if node is not None and node_muted(node):
                break
            time.sleep(0.1)
        else:
            if node is None:
                return result(False, key, "stand-in stream never appeared")
            # Up, but the restore never arrived (restore-props off?). An
            # unmute still gets saved, so carry on rather than fail.

        set_mute(node["id"], False)

        # WirePlumber writes its file on a short timer; wait for it so the
        # menu re-reads the new state rather than the old one. Meanwhile keep
        # unmuting the app's own streams: one restored as muted before our
        # unmute was stored re-saves "muted" on its next change and would undo
        # it, so success needs the file AND every live stream to agree.
        deadline = time.monotonic() + 5
        while time.monotonic() < deadline:
            still_muted = unmute_live_streams(key, node_name)
            if still_muted == 0 and read_state().get(key, {}).get("mute") is False:
                return result(True, key, "unmuted")
            time.sleep(0.2)
        return result(False, key, "WirePlumber did not save the unmute")
    finally:
        proc.terminate()
        try:
            proc.wait(timeout=2)
        except subprocess.TimeoutExpired:
            proc.kill()


def main(argv):
    # A plain SIGTERM would otherwise end Python without running the
    # `finally` that stops the stand-in.
    signal.signal(signal.SIGTERM, lambda *_: sys.exit(1))
    if len(argv) == 2 and argv[1] == "list":
        return cmd_list()
    if len(argv) == 3 and argv[1] == "unmute":
        return cmd_unmute(argv[2])
    sys.stderr.write(__doc__ or "")
    return 2


if __name__ == "__main__":
    sys.exit(main(sys.argv))

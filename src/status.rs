//! Audio status menu (`modules/status/StatusMenu.qml`): default sink/source,
//! per-app playback groups with WirePlumber saved-mute resurrection, and an
//! audio-stack restart. Network is out of scope (CMST), as in the QML.
//!
//! AntiWebUI's `AudioStream` exposes fewer PipeWire props than Quickshell's
//! node objects (no `application.id`, `media.role`, binary, or ready flag),
//! so live streams are grouped by `application.name` — the WirePlumber key
//! field most apps land on. Saved-state rows from `scripts/app-mute.py` keep
//! their exact keys, so idle-muted unmute (including `application.id` and
//! Notification-role entries) works exactly as in the QML.

use crate::bar::assets_dir;
use crate::menus::menu_card;
use crate::state::AppState;
use crate::theme::*;
use anti_webui::prelude::*;
use anti_webui::shell::AudioStream;
use anti_webui::shell::prelude::*;
use serde_json::Value;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::{Duration, Instant, SystemTime};

const HOLD: Duration = Duration::from_millis(2500);
const NOTIFICATION_KEY: &str = "Output/Audio:media.role:Notification";

#[derive(Clone, Debug)]
struct SavedApp {
    key: String,
    name: String,
    mute: bool,
    can_unmute: bool,
    block: String,
}

#[derive(Clone)]
struct Recent {
    name: String,
    until: Instant,
    rank: usize,
    last_muted: bool,
    last_volume: f64,
    pending_volume: Option<f64>,
}

#[derive(Default)]
struct StatusLocal {
    saved_apps: Vec<SavedApp>,
    list_fresh: bool,
    idle_shown: bool,
    recent: HashMap<String, Recent>,
    pending_mutes: HashMap<String, (bool, Instant)>,
    unmute_key: Option<String>,
    unmute_note: Option<String>,
    unmute_error: bool,
    restart_running: bool,
    state_mtime: Option<SystemTime>,
    debounce_armed: bool,
}

fn stream_key(app_name: &str) -> String {
    format!("Output/Audio:application.name:{app_name}")
}

fn live_groups() -> Vec<(String, Vec<AudioStream>)> {
    let mut order: Vec<String> = Vec::new();
    let mut groups: HashMap<String, Vec<AudioStream>> = HashMap::new();
    for stream in audio().streams() {
        if stream.input || stream.app_name.starts_with("minka-unmute-") {
            continue;
        }
        let key = stream_key(&stream.app_name);
        if !groups.contains_key(&key) {
            order.push(key.clone());
        }
        groups.entry(key).or_default().push(stream);
    }
    order
        .into_iter()
        .map(|k| {
            let streams = groups.remove(&k).unwrap_or_default();
            (k, streams)
        })
        .collect()
}

fn group_title(key: &str, streams: &[AudioStream]) -> String {
    if key == NOTIFICATION_KEY {
        return "Notifications".to_owned();
    }
    streams
        .first()
        .map(|s| s.app_name.clone())
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| "(unnamed)".to_owned())
}

fn group_detail(streams: &[AudioStream], title: &str) -> String {
    let mut parts: Vec<String> = Vec::new();
    let mut medias: Vec<String> = Vec::new();
    for s in streams {
        if !s.media_name.is_empty() && s.media_name != title && !medias.contains(&s.media_name) {
            medias.push(s.media_name.clone());
        }
    }
    parts.extend(medias);
    if streams.len() > 1 {
        parts.push(format!("{} streams", streams.len()));
    }
    parts.join(" · ")
}

fn saved_mute(local: &StatusLocal, key: &str) -> bool {
    if let Some((value, until)) = local.pending_mutes.get(key)
        && *until > Instant::now()
    {
        return *value;
    }
    local
        .saved_apps
        .iter()
        .find(|a| a.key == key)
        .is_some_and(|a| a.mute)
}

fn app_mute_script() -> PathBuf {
    assets_dir().parent().map_or_else(
        || PathBuf::from("scripts/app-mute.py"),
        |root| root.join("scripts/app-mute.py"),
    )
}

fn stream_properties_path() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_STATE_HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME")
                .filter(|v| !v.is_empty())
                .map(|home| PathBuf::from(home).join(".local/state"))
        })?;
    Some(base.join("wireplumber/stream-properties"))
}

fn run_saved_list(local: Rc<RefCell<StatusLocal>>, frame: Signal<u64>) {
    let buf = Rc::new(RefCell::new(String::new()));
    let proc = Process::new("python3")
        .arg(app_mute_script().to_string_lossy().into_owned())
        .arg("list")
        .spawn();
    {
        let buf = Rc::clone(&buf);
        proc.on_line(move |line| {
            buf.borrow_mut().push_str(line);
        });
    }
    proc.on_exit(move |_| {
        if !frame.is_alive() {
            return;
        }
        let text = buf.borrow().clone();
        let apps: Vec<SavedApp> = serde_json::from_str::<Value>(&text)
            .ok()
            .and_then(|v| v.get("apps").and_then(Value::as_array).cloned())
            .map(|arr| {
                arr.iter()
                    .map(|a| SavedApp {
                        key: a.get("key").and_then(Value::as_str).unwrap_or("").into(),
                        name: a.get("name").and_then(Value::as_str).unwrap_or("").into(),
                        mute: a.get("mute").and_then(Value::as_bool).unwrap_or(false),
                        can_unmute: a.get("canUnmute").and_then(Value::as_bool).unwrap_or(false),
                        block: a.get("block").and_then(Value::as_str).unwrap_or("").into(),
                    })
                    .collect()
            })
            .unwrap_or_default();
        {
            let mut local = local.borrow_mut();
            // Prune optimistic mutes the file now confirms (or that expired).
            let now = Instant::now();
            local.pending_mutes.retain(|key, (value, until)| {
                *until > now && !apps.iter().any(|a| &a.key == key && a.mute == *value)
            });
            local.saved_apps = apps;
            local.list_fresh = true;
            if !local.idle_shown {
                local.idle_shown = true;
            }
        }
        frame.update(|n| *n += 1);
    });
}

fn note_live_apps(local: &Rc<RefCell<StatusLocal>>, hovered: bool) {
    let groups = live_groups();
    let now = Instant::now();
    let mut local = local.borrow_mut();
    // Cache last-known values and apply stashed volumes for live groups.
    for (key, streams) in &groups {
        let name = group_title(key, streams);
        let muted = streams.iter().any(|s| s.muted);
        let volume = streams.first().map_or(0.0, |s| s.volume);
        let entry = local.recent.entry(key.to_owned()).or_insert(Recent {
            name: name.clone(),
            until: now,
            rank: usize::MAX,
            last_muted: muted,
            last_volume: volume,
            pending_volume: None,
        });
        entry.name = name;
        entry.last_muted = muted;
        entry.last_volume = volume;
        entry.until = if hovered {
            now + Duration::from_secs(3600)
        } else {
            now + HOLD
        };
        if let Some(v) = entry.pending_volume.take() {
            for s in streams {
                let _ = s.set_volume(v);
            }
        }
    }
    let live_keys: Vec<String> = groups.iter().map(|(k, _)| k.clone()).collect();
    local
        .recent
        .retain(|k, r| live_keys.contains(k) || r.until > now);
    // Ranks: name order when settled, stable arrival order while hovered.
    if hovered {
        let mut next = local
            .recent
            .values()
            .map(|r| r.rank)
            .filter(|r| *r != usize::MAX)
            .max()
            .map_or(0, |m| m + 1);
        for (key, _) in &groups {
            if let Some(entry) = local.recent.get_mut(key)
                && entry.rank == usize::MAX
            {
                entry.rank = next;
                next += 1;
            }
        }
    } else {
        let mut named: Vec<(String, String)> = local
            .recent
            .iter()
            .map(|(k, r)| (r.name.to_lowercase(), k.clone()))
            .collect();
        named.sort();
        for (rank, (_, key)) in named.into_iter().enumerate() {
            if let Some(entry) = local.recent.get_mut(&key) {
                entry.rank = rank;
            }
        }
    }
}

// ---------------------------------------------------------------------------
// AudioControl: mute toggle + drag slider + percent (out / mic / each app)

fn mute_toggle(muted: bool, busy: bool, enabled: bool, on_click: impl Fn() + 'static) -> Div {
    div()
        .w(34.0)
        .h(22.0)
        .rounded(5.0_f32)
        .border(1.0_f32)
        .border_color(color(if muted && enabled { RED_DIM } else { LINE }))
        .flex()
        .items_center()
        .justify_center()
        .bg(color(if muted && enabled {
            RED_DIM
        } else {
            SURFACE_RAISED
        }))
        .dyn_style(move |s| s.opacity(if enabled { 1.0 } else { 0.45_f32 }))
        .on_click(move |e: &PointerEvent| {
            if enabled {
                e.stop_propagation();
                on_click();
            }
        })
        .child(mono_text(
            if busy {
                "…"
            } else if muted {
                "✕"
            } else {
                "on"
            },
            FONT_SIZE - 3.0,
            if muted && enabled { TEXT } else { TEXT_MUTED },
        ))
}

struct ControlProps {
    tag: Option<&'static str>,
    title: String,
    detail: String,
    muted: bool,
    volume: f64,
    present: bool,
    opacity: f32,
    wheel_adjusts: bool,
}

fn audio_control(
    props: ControlProps,
    on_mute: Rc<dyn Fn(bool)>,
    on_volume: Rc<dyn Fn(f64)>,
) -> Div {
    let muted = props.muted;
    let volume = props.volume;
    let track_w = Rc::new(Cell::new(120.0_f32));
    let dragging = Rc::new(Cell::new(false));
    let drag_volume: Signal<Option<f64>> = signal(None);
    let shown_volume: Rc<dyn Fn() -> f64> = Rc::new(move || drag_volume.get().unwrap_or(volume));

    let mute_click = Rc::clone(&on_mute);
    let mut header = div().flex().flex_row().items_center().gap(6.0_f32);
    if let Some(tag) = props.tag {
        header = header.child(mono_text(tag, FONT_SIZE - 4.0, TEXT_FAINT));
    }
    header = header
        .child(
            div()
                .max_w(if props.detail.is_empty() {
                    220.0
                } else {
                    130.0
                })
                .overflow_hidden()
                .child(ui_text(props.title.clone(), FONT_SIZE - 1.0, TEXT)),
        )
        .child(ui_text(props.detail.clone(), FONT_SIZE - 3.0, TEXT_FAINT));

    let apply_at: Rc<dyn Fn(f32)> = {
        let on_volume = Rc::clone(&on_volume);
        let track_w = Rc::clone(&track_w);
        Rc::new(move |x: f32| {
            let w = track_w.get().max(1.0);
            let v = (f64::from(x) / f64::from(w)).clamp(0.0, 1.0);
            drag_volume.set(Some(v));
            on_volume(v);
        })
    };
    let apply_down = Rc::clone(&apply_at);
    let apply_move = Rc::clone(&apply_at);
    let down_drag = Rc::clone(&dragging);
    let move_drag = Rc::clone(&dragging);
    let up_drag = Rc::clone(&dragging);
    let present = props.present;
    let wheel_on_volume = Rc::clone(&on_volume);
    let wheel_adjusts = props.wheel_adjusts;
    let wheel_shown = Rc::clone(&shown_volume);
    let fill_shown = Rc::clone(&shown_volume);
    let knob_shown = Rc::clone(&shown_volume);
    let pct_shown = Rc::clone(&shown_volume);
    let fill_track = Rc::clone(&track_w);
    let knob_track = Rc::clone(&track_w);
    let slider = div()
        .relative()
        .flex_1()
        .min_w(0.0)
        .h(16.0)
        .on_resized(move |w, _| track_w.set(w))
        .on_pointer_down(move |e: &PointerEvent| {
            if present {
                down_drag.set(true);
                apply_down(e.local.x);
            }
        })
        .on_pointer_move(move |e: &PointerEvent| {
            if move_drag.get() {
                apply_move(e.local.x);
            }
        })
        .on_pointer_up(move |_| {
            up_drag.set(false);
            drag_volume.set(None);
        })
        .on_scroll(move |e: &ScrollEvent| {
            if wheel_adjusts && present {
                e.stop_propagation();
                let step = if e.delta_y < 0.0 { 0.05 } else { -0.05 };
                wheel_on_volume((wheel_shown() + step).clamp(0.0, 1.0));
            }
        })
        .child(
            div()
                .absolute()
                .left(0.0)
                .right(0.0)
                .top(6.0)
                .h(4.0)
                .rounded(2.0_f32)
                .bg(color(LINE)),
        )
        .child(dyn_view(move || {
            let frac = fill_shown().clamp(0.0, 1.0) as f32;
            let w = fill_track.get();
            div().child(
                div()
                    .absolute()
                    .left(0.0)
                    .top(6.0)
                    .h(4.0)
                    .rounded(2.0_f32)
                    .bg(color(if muted { TEXT_FAINT } else { RED }))
                    .dyn_style(move |s| s.w((w * frac).max(0.0))),
            )
        }))
        .child(dyn_view(move || {
            let frac = knob_shown().clamp(0.0, 1.0) as f32;
            let w = knob_track.get();
            div().child(
                div()
                    .absolute()
                    .top(2.0)
                    .w(12.0)
                    .h(12.0)
                    .rounded(6.0_f32)
                    .bg(color(if muted { TEXT_FAINT } else { RED }))
                    .dyn_style(move |s| s.left((w * frac - 6.0).max(0.0))),
            )
        }));

    div()
        .flex()
        .flex_col()
        .gap(4.0_f32)
        .dyn_style(move |s| s.opacity(props.opacity))
        .child(header)
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap(8.0_f32)
                .child(mute_toggle(muted, false, present, move || {
                    mute_click(!muted);
                }))
                .child(slider)
                .child(mono_text(
                    move || format!("{:.0}%", pct_shown() * 100.0),
                    FONT_SIZE - 2.0,
                    TEXT_MUTED,
                )),
        )
}

fn device_control(tag: &'static str, device: Option<AudioDevice>) -> Option<Div> {
    let device = device?;
    let d_mute = device.clone();
    let d_vol = device.clone();
    Some(audio_control(
        ControlProps {
            tag: Some(tag),
            title: device.description.clone(),
            detail: String::new(),
            muted: device.muted,
            volume: device.volume,
            present: true,
            opacity: 1.0,
            wheel_adjusts: true,
        },
        Rc::new(move |v| {
            let _ = d_mute.set_muted(v);
        }),
        Rc::new(move |v| {
            let _ = d_vol.set_volume(v);
        }),
    ))
}

// ---------------------------------------------------------------------------

pub fn status_body(_state: AppState, monitor: &str) -> Div {
    let local: Rc<RefCell<StatusLocal>> = Rc::new(RefCell::new(StatusLocal::default()));
    let frame: Signal<u64> = signal(0);
    let hovered: Signal<bool> = signal(false);

    // Open lifecycle: fetch the saved list once, then tick at 500ms.
    run_saved_list(Rc::clone(&local), frame);
    {
        let local = Rc::clone(&local);
        let tick = Rc::new(Cell::new(0_u64));
        fn schedule(
            local: Rc<RefCell<StatusLocal>>,
            frame: Signal<u64>,
            hovered: Signal<bool>,
            tick: Rc<Cell<u64>>,
        ) {
            set_timeout(Duration::from_millis(500), move || {
                if !frame.is_alive() {
                    return;
                }
                note_live_apps(&local, hovered.get_untracked());
                // stream-properties watch, 300ms-debounced refetch.
                let mtime = stream_properties_path()
                    .and_then(|p| std::fs::metadata(p).and_then(|m| m.modified()).ok());
                let arm = {
                    let mut local = local.borrow_mut();
                    if mtime != local.state_mtime {
                        local.state_mtime = mtime;
                        if !local.debounce_armed {
                            local.debounce_armed = true;
                            true
                        } else {
                            false
                        }
                    } else {
                        false
                    }
                };
                if arm {
                    let local2 = Rc::clone(&local);
                    set_timeout(Duration::from_millis(300), move || {
                        local2.borrow_mut().debounce_armed = false;
                        if frame.is_alive() {
                            run_saved_list(Rc::clone(&local2), frame);
                        }
                    });
                }
                tick.set(tick.get() + 1);
                frame.update(|n| *n += 1);
                schedule(local, frame, hovered, tick);
            });
        }
        schedule(local, frame, hovered, tick);
    }

    let screen_h = outputs()
        .into_iter()
        .find(|o| o.name == monitor)
        .map_or(1080.0, |o| o.logical_size.height as f32);

    let body_local = Rc::clone(&local);
    let card = menu_card(
        300.0,
        div().child(dyn_view(move || {
            let _ = frame.get();
            let audio = audio();
            let sink = audio.default_sink();
            let source = audio.default_source();
            let devices_h = match (sink.is_some(), source.is_some()) {
                (true, true) => 124.0,
                (false, false) => 20.0,
                _ => 62.0,
            };
            let apps_max = (screen_h - 38.0 - 32.0 - 24.0 - devices_h - 56.0).max(120.0);

            let mut col = div().flex().flex_col().gap(14.0_f32);
            let mut any_device = false;
            if let Some(c) = device_control("out", sink) {
                col = col.child(c);
                any_device = true;
            }
            if let Some(c) = device_control("mic", source) {
                col = col.child(c);
                any_device = true;
            }
            if !any_device {
                col = col.child(ui_text("no audio devices", FONT_SIZE - 1.0, TEXT_FAINT));
            }

            // Restart audio (fixed position above the churning app list).
            {
                let restarting = body_local.borrow().restart_running;
                let restart_local = Rc::clone(&body_local);
                col = col.child(
                    div()
                        .h(28.0)
                        .w_full()
                        .rounded(5.0_f32)
                        .border(1.0_f32)
                        .border_color(color(LINE))
                        .flex()
                        .items_center()
                        .justify_center()
                        .cursor(CursorIcon::Pointer)
                        .on_click(move |_| {
                            if restart_local.borrow().restart_running {
                                return;
                            }
                            restart_local.borrow_mut().restart_running = true;
                            let proc = Process::new("systemctl")
                                .args([
                                    "--user",
                                    "restart",
                                    "wireplumber",
                                    "pipewire",
                                    "pipewire-pulse",
                                ])
                                .spawn();
                            let done_local = Rc::clone(&restart_local);
                            proc.on_exit(move |_| {
                                done_local.borrow_mut().restart_running = false;
                                if frame.is_alive() {
                                    frame.update(|n| *n += 1);
                                }
                            });
                        })
                        .child(ui_text(
                            if restarting {
                                "restarting audio…"
                            } else {
                                "restart audio"
                            },
                            FONT_SIZE - 1.0,
                            TEXT_MUTED,
                        )),
                );
            }

            // App rows.
            let groups = live_groups();
            let group_map: HashMap<String, Vec<AudioStream>> = groups.into_iter().collect();
            let local = body_local.borrow();
            let mut rows: Vec<(usize, String, Recent)> = local
                .recent
                .iter()
                .map(|(k, r)| (r.rank, k.clone(), r.clone()))
                .collect();
            rows.sort_by_key(|(rank, _, _)| *rank);
            let scrollable_hint = rows.len() > 4;
            let mut apps = div().flex().flex_col().gap(14.0_f32).child(mono_text(
                "apps",
                FONT_SIZE - 3.0,
                TEXT_FAINT,
            ));
            for (_, key, recent) in &rows {
                let streams = group_map.get(key).cloned().unwrap_or_default();
                let held = streams.is_empty();
                let title = if held {
                    recent.name.clone()
                } else {
                    group_title(key, &streams)
                };
                let detail = if held {
                    "between streams".to_owned()
                } else {
                    group_detail(&streams, &title)
                };
                let force_muted = saved_mute(&local, key);
                let muted = force_muted
                    || if held {
                        recent.last_muted
                    } else {
                        streams.iter().any(|s| s.muted)
                    };
                let volume = if held {
                    recent.last_volume
                } else {
                    streams.first().map_or(0.0, |s| s.volume)
                };
                let key_mute = key.clone();
                let local_mute = Rc::clone(&body_local);
                let streams_mute = streams.clone();
                let key_vol = key.clone();
                let local_vol = Rc::clone(&body_local);
                let streams_vol = streams.clone();
                apps = apps.child(audio_control(
                    ControlProps {
                        tag: None,
                        title,
                        detail,
                        muted,
                        volume,
                        present: true,
                        opacity: if held { 0.6 } else { 1.0 },
                        wheel_adjusts: !scrollable_hint,
                    },
                    Rc::new(move |v| {
                        let was: Vec<bool> = streams_mute.iter().map(|s| s.muted).collect();
                        for s in &streams_mute {
                            let _ = s.set_muted(v);
                        }
                        // Force WirePlumber to save an unmute when only the
                        // saved state was muted.
                        if !v
                            && was.iter().all(|m| !*m)
                            && let Some(first) = streams_mute.first()
                        {
                            let _ = first.set_muted(true);
                            let _ = first.set_muted(false);
                        }
                        let mut local = local_mute.borrow_mut();
                        local.pending_mutes.insert(
                            key_mute.clone(),
                            (v, Instant::now() + Duration::from_secs(3)),
                        );
                        if let Some(r) = local.recent.get_mut(&key_mute) {
                            r.last_muted = v;
                        }
                    }),
                    Rc::new(move |v| {
                        if streams_vol.is_empty() {
                            if let Some(r) = local_vol.borrow_mut().recent.get_mut(&key_vol) {
                                r.pending_volume = Some(v);
                                r.last_volume = v;
                            }
                            return;
                        }
                        let target_mute = streams_vol.iter().any(|s| s.muted);
                        for s in &streams_vol {
                            if s.muted != target_mute {
                                let _ = s.set_muted(target_mute);
                            }
                        }
                        for s in &streams_vol {
                            let _ = s.set_volume(v);
                        }
                    }),
                ));
            }

            // Idle-muted saved rows.
            if local.idle_shown {
                let idle: Vec<SavedApp> = local
                    .saved_apps
                    .iter()
                    .filter(|a| a.mute && a.block != "stale" && !local.recent.contains_key(&a.key))
                    .cloned()
                    .collect();
                for app in idle {
                    let busy = local.unmute_key.as_deref() == Some(app.key.as_str());
                    let status = if busy {
                        "unmuting…".to_owned()
                    } else if let Some(note) = &local.unmute_note {
                        note.clone()
                    } else if app.block == "while-playing" {
                        "muted · unmute it while it plays".to_owned()
                    } else {
                        "muted · not playing".to_owned()
                    };
                    let is_error = local.unmute_error && local.unmute_note.is_some() && !busy;
                    let key_unmute = app.key.clone();
                    let local_unmute = Rc::clone(&body_local);
                    let enabled = app.can_unmute && local.unmute_key.is_none();
                    apps = apps.child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(4.0_f32)
                            .child(div().flex().flex_row().items_center().gap(6.0_f32).child(
                                ui_text(
                                    if app.key == NOTIFICATION_KEY {
                                        "Notifications".to_owned()
                                    } else {
                                        app.name.clone()
                                    },
                                    FONT_SIZE - 1.0,
                                    TEXT,
                                ),
                            ))
                            .child(
                                div()
                                    .flex()
                                    .flex_row()
                                    .items_center()
                                    .gap(8.0_f32)
                                    .child(mute_toggle(true, busy, enabled, move || {
                                        {
                                            let mut local = local_unmute.borrow_mut();
                                            if local.unmute_key.is_some() {
                                                return;
                                            }
                                            local.unmute_key = Some(key_unmute.clone());
                                            local.unmute_note = None;
                                            local.unmute_error = false;
                                        }
                                        frame.update(|n| *n += 1);
                                        let buf = Rc::new(RefCell::new(String::new()));
                                        let proc = Process::new("python3")
                                            .arg(app_mute_script().to_string_lossy().into_owned())
                                            .arg("unmute")
                                            .arg(key_unmute.clone())
                                            .spawn();
                                        {
                                            let buf = Rc::clone(&buf);
                                            proc.on_line(move |line| {
                                                buf.borrow_mut().push_str(line);
                                            });
                                        }
                                        let done_local = Rc::clone(&local_unmute);
                                        proc.on_exit(move |_| {
                                            if !frame.is_alive() {
                                                return;
                                            }
                                            let text = buf.borrow().clone();
                                            let (ok, message) =
                                                serde_json::from_str::<Value>(&text)
                                                    .map(|v| {
                                                        (
                                                            v.get("ok")
                                                                .and_then(Value::as_bool)
                                                                .unwrap_or(false),
                                                            v.get("message")
                                                                .and_then(Value::as_str)
                                                                .unwrap_or("")
                                                                .to_owned(),
                                                        )
                                                    })
                                                    .unwrap_or((false, "unmute failed".into()));
                                            {
                                                let mut local = done_local.borrow_mut();
                                                local.unmute_key = None;
                                                if ok {
                                                    local.unmute_note = None;
                                                    local.unmute_error = false;
                                                } else {
                                                    local.unmute_note = Some(message);
                                                    local.unmute_error = true;
                                                }
                                            }
                                            run_saved_list(Rc::clone(&done_local), frame);
                                            frame.update(|n| *n += 1);
                                        });
                                    }))
                                    .child(ui_text(
                                        status,
                                        FONT_SIZE - 3.0,
                                        if is_error { RED } else { TEXT_FAINT },
                                    )),
                            ),
                    );
                }
            }
            drop(local);

            col.child(div().max_h(apps_max).overflow_y_scroll().child(apps))
        })),
    );
    crate::bar::track_hover(card, hovered)
}

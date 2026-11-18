//! The top bar (general layout) and its widgets, from `modules/bar/*.qml`.
//! The same widgets are reused by the Duo side panel.

use crate::ipc::Win;
use crate::settings;
use crate::state::{AppState, MenuKind, cleanup_write};
use crate::theme::*;
use anti_webui::prelude::*;
use anti_webui::shell::prelude::*;
use anti_webui_md3e::components::{
    ButtonSize, ButtonStyle, IconButtonStyle, Ink, icon_toggle_button, sized_icon, styled_button,
    styled_toggle_button,
};
use anti_webui_md3e::icons;
use std::cell::Cell;
use std::path::PathBuf;
use std::rc::Rc;

/// Directory holding `assets/` (window-control SVGs). Resolution order:
/// `$MINKA_SHELL_DIR`, the exe's ancestors (repo checkout under
/// `target/release`), then the installed tree.
pub fn assets_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os("MINKA_SHELL_DIR").filter(|v| !v.is_empty()) {
        return PathBuf::from(dir).join("assets");
    }
    if let Ok(exe) = std::env::current_exe() {
        let mut dir = exe.parent().map(PathBuf::from);
        for _ in 0..4 {
            let Some(d) = dir else { break };
            let candidate = d.join("assets");
            if candidate.join("mac-glass-minimize.svg").exists() {
                return candidate;
            }
            dir = d.parent().map(PathBuf::from);
        }
    }
    PathBuf::from("/usr/share/minka/MinkaShell/assets")
}

/// Heuristic desktop-entry icon for a compositor app_id (usually bare and
/// arbitrarily cased; `Apps::get` wants the exact id).
pub fn app_icon_path(app_id: &str, size: u32) -> Option<PathBuf> {
    let all = apps().all();
    let entry = all.iter().find(|a| {
        a.id.eq_ignore_ascii_case(app_id)
            || a.id
                .trim_end_matches(".desktop")
                .eq_ignore_ascii_case(app_id)
    });
    entry
        .and_then(|a| a.icon_path(size))
        .or_else(|| anti_webui::shell::icon_path("application-x-executable", size))
}

/// Hover tracking that survives owner disposal ordering (same pattern as
/// MinkaMon's `with_hover`).
pub fn track_hover(view: Div, hovered: Signal<bool>) -> Div {
    let here = Rc::new(Cell::new(false));
    let here_cleanup = Rc::clone(&here);
    on_cleanup(move || {
        if here_cleanup.get() {
            cleanup_write(move || {
                if hovered.is_alive() {
                    hovered.set(false);
                }
            });
        }
    });
    view.on_hover(move |on| {
        if here.get() != on {
            here.set(on);
            hovered.set_if_changed(on);
        }
    })
}

fn focused_now(state: &AppState, monitor: &str) -> Option<Win> {
    let duo = settings::duo_mode(&state.settings.get(), &outputs());
    state.view.with(|v| v.focused_window(duo, monitor))
}

/// The start button: an md3e icon button that is on (red, and squarer)
/// while the start menu is open.
pub fn start_button(state: AppState, monitor: String) -> Div {
    let click_state = state.clone();
    let monitor_click = monitor.clone();
    icon_toggle_button(
        icons::APPS,
        Binding::new(
            move || state.is_open(MenuKind::Start, &monitor),
            move |_| click_state.toggle_menu(MenuKind::Start, Some(&monitor_click), None),
        ),
        IconButtonStyle::filled().size(ButtonSize::ExtraSmall),
    )
}

/// What a monitor's workspace `index` is like now: (active, has windows).
fn workspace_now(state: &AppState, monitor: &str, index: i64) -> (bool, bool) {
    state.view.with(|v| {
        v.monitor_view(monitor)
            .and_then(|m| m.workspaces.iter().find(|w| w.index == index))
            .map_or((false, false), |w| (w.active, w.window_count > 0))
    })
}

/// One workspace: an md3e toggle button that is on (red, and squarer) while
/// its workspace is the active one. Off, it is outlined, brighter when the
/// workspace has windows. It reads what it shows itself, so it is not built
/// again when that changes and its change of shape can play.
fn workspace_pill(state: AppState, monitor: String, index: i64) -> Div {
    let (is_active, activate) = (state.clone(), state.clone());
    let (active_monitor, click_monitor) = (monitor.clone(), monitor.clone());
    let (dim_state, dim_monitor) = (state.clone(), monitor.clone());
    styled_toggle_button(
        index.to_string(),
        Binding::new(
            move || workspace_now(&is_active, &active_monitor, index).0,
            move |_| activate.ipc.activate_workspace(&click_monitor, index),
        ),
        ButtonStyle::filled().size(ButtonSize::ExtraSmall),
    )
    .dyn_style(move |s| {
        let (active, occupied) = workspace_now(&dim_state, &dim_monitor, index);
        // Always 1 px, so the pill is as wide on as off.
        let s = s.border(1.0_f32);
        if active {
            s.border_color(color(RED))
        } else {
            s.border_color(color(if occupied { TEXT_FAINT } else { LINE }))
                .text_color(color(if occupied { TEXT } else { TEXT_FAINT }))
        }
    })
    // As in the DOM, `on_click` is the primary button only: the middle and
    // right buttons arrive as aux clicks.
    .on_aux_click(move |e: &PointerEvent| {
        if e.button == Some(PointerButton::Middle) {
            state.ipc.toggle_tiling(&monitor);
        }
    })
}

pub fn workspaces(state: AppState, monitor: String) -> Div {
    // The pills are keyed by workspace, so only a workspace that comes or
    // goes builds or drops one.
    let shown = {
        let (state, monitor) = (state.clone(), monitor.clone());
        memo(move || {
            let enabled = state.settings.with(|s| s.workspaces_enabled);
            state.view.with(|v| {
                v.monitor_view(&monitor)
                    .filter(|m| enabled || m.workspaces.len() > 1)
                    .map(|m| m.workspaces.iter().map(|w| w.index).collect::<Vec<_>>())
                    .unwrap_or_default()
            })
        })
    };
    // (the monitor is known, its active workspace is tiled)
    let tiling = {
        let (state, monitor) = (state.clone(), monitor.clone());
        memo(move || {
            state.view.with(|v| {
                v.monitor_view(&monitor)
                    .map(|m| m.active_workspace().is_some_and(|w| w.is_tiled))
            })
        })
    };
    // None: the monitor is not known. Some(true): there are no pills, and
    // the tiling icon is the toggle.
    let as_button = memo(move || tiling.get().map(|_| shown.with(|pills| pills.is_empty())));
    let (pill_state, pill_monitor) = (state.clone(), monitor.clone());
    div()
        .flex()
        .flex_row()
        .items_center()
        .gap(4.0_f32)
        .child(for_each(
            move || shown.get(),
            |index| *index,
            move |index| workspace_pill(pill_state.clone(), pill_monitor.clone(), index),
        ))
        .child(dyn_view(move || {
            // Only which kind of thing stands here is read: the glyph below
            // follows the tiling itself, so a click's ripple is not cut off
            // by the answer to it.
            let Some(as_button) = as_button.get() else {
                return div();
            };
            if !as_button {
                return tiling_glyph(tiling);
            }
            // With desktops off the icon itself is the tiling toggle.
            let (state, monitor) = (state.clone(), monitor.clone());
            let ink = Ink::new(|c| c.on_surface, || false);
            let button = div()
                .size(28.0)
                .rounded(14.0_f32)
                .flex()
                .flex_none()
                .items_center()
                .justify_center()
                .cursor(CursorIcon::Pointer)
                .on_click(move |_| state.ipc.toggle_tiling(&monitor))
                .child(tiling_glyph(tiling));
            ink.paint(ink.listen(button))
        }))
}

/// The active workspace's layout as an icon: tiled or floating.
fn tiling_glyph(tiling: Memo<Option<bool>>) -> Div {
    div().flex().flex_none().child(dyn_view(move || {
        let symbol = if tiling.get() == Some(true) {
            icons::SPLITSCREEN_LEFT
        } else {
            icons::SELECT_WINDOW
        };
        sized_icon(symbol, 16.0).text_color(color(PURPLE))
    }))
}

pub fn focused_title(state: AppState, monitor: String) -> Div {
    let view_state = state.clone();
    div().child(dyn_view(move || {
        let row = div()
            .flex()
            .flex_row()
            .items_center()
            .gap(6.0_f32)
            .max_w(340.0)
            .overflow_hidden();
        let Some(win) = focused_now(&view_state, &monitor) else {
            return row;
        };
        let row = match app_icon_path(&win.app_id, 32) {
            Some(path) => row.child(img(path).w(16.0).h(16.0).object_fit(ObjectFit::Contain)),
            None => row,
        };
        row.child(div().max_w(320.0).overflow_hidden().child(ui_text(
            elide(&win.title, 320.0, FONT_SIZE - 1.0),
            FONT_SIZE - 1.0,
            TEXT_MUTED,
        )))
    }))
}

fn control_button(body: &'static str, glyph: &'static str, on_click: impl Fn() + 'static) -> Div {
    let hovered = signal(false);
    let assets = assets_dir();
    let button = div()
        .relative()
        .w(16.0)
        .h(16.0)
        .cursor(CursorIcon::Pointer)
        .on_click(move |_| on_click())
        .child(
            img(assets.join(body))
                .absolute()
                .left(0.0)
                .top(0.0)
                .w(16.0)
                .h(16.0),
        )
        .child(
            img(assets.join(glyph))
                .absolute()
                .left(0.0)
                .top(0.0)
                .w(16.0)
                .h(16.0)
                .transition(Transition::ms(120))
                .dyn_style(move |s| s.opacity(if hovered.get() { 1.0 } else { 0.0_f32 })),
        );
    track_hover(button, hovered)
}

pub fn window_controls(state: AppState, monitor: String) -> Div {
    let view_state = state.clone();
    div().child(dyn_view(move || {
        let row = div().flex().flex_row().items_center().gap(6.0_f32);
        let Some(win) = focused_now(&view_state, &monitor) else {
            return row;
        };
        let min_state = view_state.clone();
        let min_win = win.clone();
        let max_state = view_state.clone();
        let max_win = win.clone();
        row.child(control_button(
            "mac-glass-minimize.svg",
            "mac-glass-minus.svg",
            move || {
                if min_win.minimized {
                    min_state.ipc.activate_window(&min_win.id);
                } else {
                    min_state.ipc.minimize_window(&min_win.id);
                }
            },
        ))
        .child(control_button(
            "mac-glass-maximize.svg",
            if win.maximized {
                "mac-glass-shrink.svg"
            } else {
                "mac-glass-expand.svg"
            },
            move || {
                max_state
                    .ipc
                    .maximize_window(&max_win.id, !max_win.maximized);
            },
        ))
    }))
}

/// The date and time: an md3e toggle button that is on while the calendar is
/// open.
pub fn clock_pill(state: AppState, monitor: String) -> Div {
    let click_state = state.clone();
    let monitor_click = monitor.clone();
    let (open_state, open_monitor) = (state.clone(), monitor.clone());
    styled_toggle_button(
        move || {
            let t = local_time(Every::Second);
            format!(
                "{}/{}/{}  {:02}:{:02}",
                t.day, t.month, t.year, t.hour, t.minute
            )
        },
        Binding::new(
            move || state.is_open(MenuKind::Calendar, &monitor),
            move |_| click_state.toggle_menu(MenuKind::Calendar, Some(&monitor_click), None),
        ),
        // The label of the next size up (13 px, as the clock was) in the
        // smallest size's box, which also fits the 32 px bar.
        ButtonStyle::filled().size(ButtonSize::Small),
    )
    .h(28.0)
    .px(12.0_f32)
    // Off, a filled toggle's label is the muted text color; the clock is
    // read too often for that.
    .dyn_style(move |s| {
        if open_state.is_open(MenuKind::Calendar, &open_monitor) {
            s
        } else {
            s.text_color(color(TEXT))
        }
    })
}

pub fn sys_usage(state: AppState) -> Div {
    let usage = state.sys_usage;
    div()
        .flex()
        .flex_row()
        .items_center()
        .gap(8.0_f32)
        .child(
            mono_text(
                move || format!("cpu {:.0}%", usage.get().0),
                FONT_SIZE - 2.0,
                TEXT_FAINT,
            )
            .transition(Transition::ms(200))
            .dyn_style(move |s| {
                s.text_color(color(if usage.get().0 >= 90.0 {
                    RED
                } else {
                    TEXT_FAINT
                }))
            }),
        )
        .child(
            mono_text(
                move || format!("mem {:.0}%", usage.get().1),
                FONT_SIZE - 2.0,
                TEXT_FAINT,
            )
            .transition(Transition::ms(200))
            .dyn_style(move |s| {
                s.text_color(color(if usage.get().1 >= 90.0 {
                    RED
                } else {
                    TEXT_FAINT
                }))
            }),
        )
}

/// The volume: a Material speaker in a round box with md3e's hover and
/// ripple. A click opens the status menu, the middle button mutes, the wheel
/// is 5% a step. The box stays; only the icon in it is built again when the
/// sound changes.
pub fn volume_widget(state: AppState, monitor: String) -> Div {
    div().child(show(
        || audio().default_sink().is_some(),
        move || volume_button(state.clone(), monitor.clone()),
    ))
}

fn volume_button(state: AppState, monitor: String) -> Div {
    let ink = Ink::new(|c| c.on_surface, || false);
    let button = div()
        .size(28.0)
        .rounded(14.0_f32)
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .cursor(CursorIcon::Pointer)
        .on_click(move |_| state.toggle_menu(MenuKind::Status, Some(&monitor), None))
        .on_aux_click(|e: &PointerEvent| {
            if e.button == Some(PointerButton::Middle)
                && let Some(sink) = untrack(|| audio().default_sink())
            {
                let _ = sink.set_muted(!sink.muted);
            }
        })
        .on_scroll(|e: &ScrollEvent| {
            // One step a wheel notch and one a finger gesture. This box stays
            // while the sound changes, so a gesture's later frames, its lift
            // and its inertia all come here too, the last two with no motion.
            if e.delta_y == 0.0 || !matches!(e.phase, ScrollPhase::Single | ScrollPhase::Began) {
                return;
            }
            if let Some(sink) = untrack(|| audio().default_sink()) {
                let step = if e.delta_y < 0.0 { 0.05 } else { -0.05 };
                let _ = sink.set_volume((sink.volume + step).clamp(0.0, 1.0));
            }
        })
        .child(dyn_view(|| {
            let Some(sink) = audio().default_sink() else {
                return div();
            };
            let (symbol, hex) = if sink.muted {
                (icons::VOLUME_OFF, RED)
            } else if sink.volume <= 0.0 {
                (icons::VOLUME_MUTE, TEXT_MUTED)
            } else if sink.volume * 100.0 > 50.0 {
                (icons::VOLUME_UP, TEXT_MUTED)
            } else {
                (icons::VOLUME_DOWN, TEXT_MUTED)
            };
            sized_icon(symbol, 18.0).text_color(color(hex))
        }));
    ink.paint(ink.listen(button))
}

pub fn battery_widget(state: AppState, monitor: String) -> Div {
    let click_state = state.clone();
    div().child(dyn_view(move || {
        let bat = battery();
        if !bat.available() {
            return div();
        }
        let pct = bat.percentage().round();
        let charging = bat.charging();
        let low = !charging && pct <= 20.0;
        let fill_w = ((24.0 - 4.0) * pct / 100.0).max(1.0) as f32;
        let fill_hex = if low {
            RED_DIM
        } else if charging {
            PURPLE_DIM
        } else {
            LINE
        };
        let menu_state = click_state.clone();
        let monitor_menu = monitor.clone();
        let mut row = div()
            .flex()
            .flex_row()
            .items_center()
            .gap(3.0_f32)
            .cursor(CursorIcon::Pointer)
            .on_click(move |_| menu_state.toggle_menu(MenuKind::Battery, Some(&monitor_menu), None))
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .child(
                        div()
                            .relative()
                            .w(24.0)
                            .h(13.0)
                            .rounded(2.0_f32)
                            .border(1.0_f32)
                            .border_color(color(if low { RED } else { TEXT_FAINT }))
                            .child(
                                div()
                                    .absolute()
                                    .left(1.0)
                                    .top(1.0)
                                    .w(fill_w)
                                    .h(9.0)
                                    .bg(color(fill_hex)),
                            )
                            .child(
                                div()
                                    .absolute()
                                    .left(0.0)
                                    .right(0.0)
                                    .top(0.0)
                                    .bottom(0.0)
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .child(
                                        mono_text(format!("{pct:.0}"), 9.0, TEXT)
                                            .font_weight(FontWeight::BOLD),
                                    ),
                            ),
                    )
                    .child(
                        div()
                            .w(2.0)
                            .h(6.0)
                            .bg(color(if low { RED } else { TEXT_FAINT })),
                    ),
            );
        if charging {
            row = row.child(sized_icon(icons::BOLT, 14.0).text_color(color(PURPLE)));
        }
        row
    }))
}

fn tray_item_view(output: Output, item: anti_webui::shell::TrayItem) -> Div {
    let hovered = signal(false);
    let menu_open = signal(false);
    let click_item = item.clone();
    let ox = output.position.x;
    let oy = output.position.y;
    // The family is set here for the tray menu below, which is the toolkit's
    // own widget and would otherwise use the toolkit's default font.
    let ink = Ink::new(|c| c.on_surface, || false);
    let icon: Div = div()
        .relative()
        .font_family(FONT_FAMILY)
        .size(24.0)
        .rounded(12.0_f32)
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .cursor(CursorIcon::Pointer)
        .dyn_style(move |s| s.opacity(if hovered.get() { 1.0 } else { 0.85_f32 }))
        .on_click({
            let item = click_item.clone();
            move |e: &PointerEvent| {
                if item.item_is_menu && item.has_menu {
                    menu_open.set(!menu_open.get_untracked());
                } else {
                    let _ = item.activate(ox + e.position.x as i32, oy + e.position.y as i32);
                }
            }
        })
        // The middle and right buttons are aux clicks, not clicks.
        .on_aux_click(move |e: &PointerEvent| match e.button {
            Some(PointerButton::Middle) => {
                let _ = click_item
                    .secondary_activate(ox + e.position.x as i32, oy + e.position.y as i32);
            }
            Some(PointerButton::Secondary) if click_item.has_menu => {
                menu_open.set(!menu_open.get_untracked());
            }
            _ => {}
        });
    let icon = if item.icon.best(48).is_some() {
        let best = item.clone();
        icon.child(bitmap_view(move || best.icon.best(48)).w(18.0).h(18.0))
    } else if let Some(path) = item
        .icon
        .name
        .as_deref()
        .and_then(|n| anti_webui::shell::icon_path(n, 48))
    {
        icon.child(img(path).w(18.0).h(18.0).object_fit(ObjectFit::Contain))
    } else {
        icon
    };
    let icon = icon.child(tray_menu(&item, menu_open));
    track_hover(ink.paint(ink.listen(icon)), hovered)
}

pub fn tray_cluster(state: AppState, output: Output) -> Div {
    let monitor = output.name.clone();
    let state_items = state.clone();
    let output_items = output.clone();
    div()
        .flex()
        .flex_row()
        .items_center()
        .gap(6.0_f32)
        .child(dyn_view(move || {
            let mut row = div().flex().flex_row().items_center().gap(6.0_f32);
            // One icon an item, even if it is registered twice (by itself
            // under one name and on its behalf under another).
            let mut shown = std::collections::HashSet::new();
            for item in tray().items() {
                if shown.insert(item.key()) {
                    row = row.child(tray_item_view(output_items.clone(), item));
                }
            }
            row
        }))
        .child(volume_widget(state_items.clone(), monitor.clone()))
        .child(battery_widget(state, monitor))
}

/// Updates waiting: an outlined md3e button, in amber when one of them
/// would change repository. A click opens MinkaSyu.
pub fn updates_pill(state: AppState) -> Div {
    div().flex().flex_row().child(dyn_view(move || {
        let syu = state.syu.get();
        if syu.count == 0 {
            return div();
        }
        let attention = syu.repo_changes > 0;
        let label = if attention {
            format!("{} ⇄ {}", syu.count, syu.repo_changes)
        } else {
            syu.count.to_string()
        };
        styled_button(
            label,
            ButtonStyle::outlined()
                .size(ButtonSize::ExtraSmall)
                .icon(icons::ARROW_UPWARD),
        )
        .dyn_style(move |s| {
            if attention {
                s.border_color(color(WARN_AMBER))
                    .text_color(color(WARN_AMBER))
            } else {
                s
            }
        })
        .on_click(move |_| {
            // `minka-syu ui` picks the window: its AntiWebUI front end,
            // or the QML one under MINKA_SYU_DIR when that is missing.
            let _ = std::process::Command::new(settings::syu_bin())
                .arg("ui")
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn();
        })
    }))
}

pub fn health_dot(state: AppState) -> Div {
    div()
        .w(7.0)
        .h(7.0)
        .rounded(3.5_f32)
        .transition(Transition::ms(200))
        .dyn_style(move |s| s.bg(color(if state.ipc_ready.get() { RED_DIM } else { RED })))
}

pub fn bar_body(state: AppState, output: Output) -> Div {
    let monitor = output.name.clone();
    div()
        .relative()
        .w_full()
        .h_full()
        .bg(bar_bg())
        .flex()
        .flex_row()
        .items_center()
        .px(10.0_f32)
        .child(
            div()
                .flex_1()
                .min_w(0.0)
                .flex()
                .flex_row()
                .items_center()
                .gap(12.0_f32)
                .overflow_hidden()
                .child(start_button(state.clone(), monitor.clone()))
                .child(workspaces(state.clone(), monitor.clone()))
                .child(focused_title(state.clone(), monitor.clone()))
                .child(window_controls(state.clone(), monitor.clone())),
        )
        .child(clock_pill(state.clone(), monitor.clone()))
        .child(
            div()
                .flex_1()
                .min_w(0.0)
                .flex()
                .flex_row()
                .items_center()
                .justify_end()
                .gap(12.0_f32)
                .child(sys_usage(state.clone()))
                .child(tray_cluster(state.clone(), output))
                .child(updates_pill(state.clone()))
                .child(health_dot(state)),
        )
        .child(
            div()
                .absolute()
                .left(0.0)
                .right(0.0)
                .bottom(0.0)
                .h(1.0)
                .bg(color(LINE)),
        )
}

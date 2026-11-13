//! The top bar (general layout) and its widgets, from `modules/bar/*.qml`.
//! The same widgets are reused by the Duo side panel.

use crate::ipc::Win;
use crate::settings::{self, Settings};
use crate::state::{AppState, MenuKind, cleanup_write};
use crate::theme::*;
use anti_webui::prelude::svg::kurbo::{BezPath, Point};
use anti_webui::prelude::*;
use anti_webui::shell::prelude::*;
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

pub fn start_button(state: AppState, monitor: String) -> Div {
    let hovered = signal(false);
    let click_state = state.clone();
    let monitor_click = monitor.clone();
    let button = div()
        .w(24.0)
        .h(22.0)
        .rounded(5.0_f32)
        .flex()
        .items_center()
        .justify_center()
        .cursor(CursorIcon::Pointer)
        .dyn_style(move |s| {
            if state.is_open(MenuKind::Start, &monitor) {
                s.bg(color(RED_DIM))
            } else if hovered.get() {
                s.bg(color(SURFACE_RAISED))
            } else {
                s.bg(Color::TRANSPARENT)
            }
        })
        .on_click(move |_| click_state.toggle_menu(MenuKind::Start, Some(&monitor_click), None))
        .child(
            ui_text("❖", FONT_SIZE + 1.0, TEXT_MUTED).dyn_style(move |s| {
                s.text_color(if hovered.get() {
                    color(RED)
                } else {
                    color(TEXT_MUTED)
                })
            }),
        );
    track_hover(button, hovered)
}

fn workspace_pill(
    state: AppState,
    monitor: String,
    index: i64,
    active: bool,
    occupied: bool,
) -> Div {
    let hovered = signal(false);
    let click_state = state.clone();
    let monitor_click = monitor.clone();
    let pill = div()
        .h(20.0)
        .min_w(22.0)
        .px(6.0_f32)
        .rounded(4.0_f32)
        .border(1.0_f32)
        .flex()
        .items_center()
        .justify_center()
        .cursor(CursorIcon::Pointer)
        .dyn_style(move |s| {
            if active {
                s.bg(color(RED)).border_color(color(RED))
            } else if hovered.get() {
                s.bg(color(SURFACE_RAISED)).border_color(color(if occupied {
                    TEXT_FAINT
                } else {
                    LINE
                }))
            } else {
                s.bg(Color::TRANSPARENT).border_color(color(if occupied {
                    TEXT_FAINT
                } else {
                    LINE
                }))
            }
        })
        // As in the DOM, `on_click` is the primary button only: the middle and
        // right buttons arrive as aux clicks.
        .on_click({
            let state = click_state.clone();
            let monitor = monitor_click.clone();
            move |_| state.ipc.activate_workspace(&monitor, index)
        })
        .on_aux_click(move |e: &PointerEvent| {
            if e.button == Some(PointerButton::Middle) {
                click_state.ipc.toggle_tiling(&monitor_click);
            }
        })
        .child(ui_text(
            index.to_string(),
            FONT_SIZE - 1.0,
            if active {
                GROUND
            } else if occupied {
                TEXT
            } else {
                TEXT_FAINT
            },
        ));
    track_hover(pill, hovered)
}

pub fn workspaces(state: AppState, monitor: String) -> Div {
    let view_state = state.clone();
    let monitor_view = monitor.clone();
    div().child(dyn_view(move || {
        let settings_now: Settings = view_state.settings.get();
        let row = div().flex().flex_row().items_center().gap(4.0_f32);
        let monitor_data = view_state
            .view
            .with(|v| v.monitor_view(&monitor_view).cloned());
        let Some(monitor_data) = monitor_data else {
            return row;
        };
        let show_pills = settings_now.workspaces_enabled || monitor_data.workspaces.len() > 1;
        let mut row = row;
        if show_pills {
            for ws in &monitor_data.workspaces {
                row = row.child(workspace_pill(
                    view_state.clone(),
                    monitor_view.clone(),
                    ws.index,
                    ws.active,
                    ws.window_count > 0,
                ));
            }
        }
        let tiled = monitor_data.active_workspace().is_some_and(|w| w.is_tiled);
        let glyph = ui_text(if tiled { "◫" } else { "◰" }, FONT_SIZE, PURPLE);
        if show_pills {
            row.child(glyph)
        } else {
            // With desktops off the glyph itself is the tiling toggle.
            let click_state = view_state.clone();
            let monitor_click = monitor_view.clone();
            row.child(
                div()
                    .cursor(CursorIcon::Pointer)
                    .on_click(move |_| click_state.ipc.toggle_tiling(&monitor_click))
                    .child(glyph),
            )
        }
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

pub fn clock_pill(state: AppState, monitor: String) -> Div {
    let hovered = signal(false);
    let click_state = state.clone();
    let monitor_click = monitor.clone();
    let pill = div()
        .h(22.0)
        .px(8.0_f32)
        .rounded(5.0_f32)
        .flex()
        .items_center()
        .justify_center()
        .cursor(CursorIcon::Pointer)
        .dyn_style(move |s| {
            if state.is_open(MenuKind::Calendar, &monitor) {
                s.bg(color(RED_DIM))
            } else if hovered.get() {
                s.bg(color(SURFACE_RAISED))
            } else {
                s.bg(Color::TRANSPARENT)
            }
        })
        .on_click(move |_| click_state.toggle_menu(MenuKind::Calendar, Some(&monitor_click), None))
        .child(ui_text(
            move || {
                let t = local_time(Every::Second);
                format!(
                    "{}/{}/{}  {:02}:{:02}",
                    t.day, t.month, t.year, t.hour, t.minute
                )
            },
            FONT_SIZE,
            TEXT,
        ));
    track_hover(pill, hovered)
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
            .dyn_style(move |s| {
                s.text_color(color(if usage.get().1 >= 90.0 {
                    RED
                } else {
                    TEXT_FAINT
                }))
            }),
        )
}

fn speaker_icon(volume: f64, muted: bool, hex: u32) -> Svg {
    let mut chart = svg()
        .view_box(0.0, 0.0, 20.0, 18.0)
        .aspect(svg::SvgAspect {
            align: None,
            slice: false,
        })
        .w(20.0)
        .h(18.0);
    let mut body = BezPath::new();
    body.move_to(Point::new(2.0, 7.0));
    body.line_to(Point::new(6.0, 7.0));
    body.line_to(Point::new(10.0, 3.5));
    body.line_to(Point::new(10.0, 14.5));
    body.line_to(Point::new(6.0, 11.0));
    body.line_to(Point::new(2.0, 11.0));
    body.close_path();
    chart = chart.child(svg::path(body).fill(color(hex)));
    let arc = |x0: f64, y0: f64, cx: f64, cy: f64, x1: f64, y1: f64| {
        let mut p = BezPath::new();
        p.move_to(Point::new(x0, y0));
        p.quad_to(Point::new(cx, cy), Point::new(x1, y1));
        svg::path(p)
            .fill_none()
            .stroke(color(hex))
            .stroke_width(1.4_f32)
    };
    if muted {
        let mut cross = BezPath::new();
        cross.move_to(Point::new(13.0, 6.5));
        cross.line_to(Point::new(17.5, 11.5));
        cross.move_to(Point::new(17.5, 6.5));
        cross.line_to(Point::new(13.0, 11.5));
        chart = chart.child(
            svg::path(cross)
                .fill_none()
                .stroke(color(RED))
                .stroke_width(1.4_f32),
        );
    } else {
        chart = chart.child(arc(12.5, 6.8, 14.5, 9.0, 12.5, 11.2));
        if volume * 100.0 > 50.0 {
            chart = chart.child(arc(14.5, 5.0, 17.5, 9.0, 14.5, 13.0));
        }
    }
    chart
}

pub fn volume_widget(state: AppState, monitor: String) -> Div {
    let click_state = state.clone();
    div().child(dyn_view(move || {
        let sink = audio().default_sink();
        let Some(sink) = sink else {
            return div();
        };
        let mute_sink = sink.clone();
        let scroll_sink = sink.clone();
        let menu_state = click_state.clone();
        let monitor_menu = monitor.clone();
        div()
            .flex()
            .items_center()
            .cursor(CursorIcon::Pointer)
            .on_click(move |_| menu_state.toggle_menu(MenuKind::Status, Some(&monitor_menu), None))
            .on_aux_click(move |e: &PointerEvent| {
                if e.button == Some(PointerButton::Middle) {
                    let _ = mute_sink.set_muted(!mute_sink.muted);
                }
            })
            .on_scroll(move |e: &ScrollEvent| {
                let step = if e.delta_y < 0.0 { 0.05 } else { -0.05 };
                let _ = scroll_sink.set_volume((scroll_sink.volume + step).clamp(0.0, 1.0));
            })
            .child(speaker_icon(sink.volume, sink.muted, TEXT_MUTED))
    }))
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
            row = row.child(ui_text("⚡", FONT_SIZE - 2.0, PURPLE));
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
    let icon: Div = div()
        .relative()
        .w(18.0)
        .h(18.0)
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
            Some(PointerButton::Secondary) => {
                if click_item.has_menu {
                    menu_open.set(!menu_open.get_untracked());
                }
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
    track_hover(icon, hovered)
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
            for item in tray().items() {
                row = row.child(tray_item_view(output_items.clone(), item));
            }
            row
        }))
        .child(volume_widget(state_items.clone(), monitor.clone()))
        .child(battery_widget(state, monitor))
}

pub fn updates_pill(state: AppState) -> Div {
    div().child(dyn_view(move || {
        let syu = state.syu.get();
        if syu.count == 0 {
            return div();
        }
        let attention = syu.repo_changes > 0;
        let label = if attention {
            format!("↑ {} ⇄ {}", syu.count, syu.repo_changes)
        } else {
            format!("↑ {}", syu.count)
        };
        div()
            .h(20.0)
            .px(8.0_f32)
            .rounded(10.0_f32)
            .border(1.0_f32)
            .border_color(color(if attention { WARN_AMBER } else { LINE }))
            .flex()
            .items_center()
            .justify_center()
            .cursor(CursorIcon::Pointer)
            .on_click(move |_| {
                let _ = std::process::Command::new("qs")
                    .args(["-p", &settings::syu_ui_dir()])
                    .stdin(std::process::Stdio::null())
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .spawn();
            })
            .child(mono_text(
                label,
                FONT_SIZE - 2.0,
                if attention { WARN_AMBER } else { TEXT_MUTED },
            ))
    }))
}

pub fn health_dot(state: AppState) -> Div {
    div()
        .w(7.0)
        .h(7.0)
        .rounded(3.5_f32)
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

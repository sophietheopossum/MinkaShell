//! Dropdown menus, start menu, and the click-away backdrop.
//! QML used layer surfaces for all of these (no xdg popups), Overlay for
//! menus over a Top-layer backdrop; same arrangement here.

use crate::bar::track_hover;
use crate::state::{AppState, MenuKind};
use crate::status;
use crate::theme::*;
use anti_webui::prelude::*;
use anti_webui::shell::prelude::*;
use std::cell::Cell;
use std::rc::Rc;

const MENU_TOP_MARGIN: i32 = 38;

pub fn menu_card(width: f32, content: Div) -> Div {
    div()
        .w(width)
        .rounded(10.0_f32)
        .bg(bar_bg())
        .border(1.0_f32)
        .border_color(color(LINE))
        .p(12.0_f32)
        .child(content)
}

// ---------------------------------------------------------------------------
// Calendar (pure display, Monday-first month grid)

const MONTHS: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];

fn days_from_civil(y: i32, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y } as i64;
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = ((m + 9) % 12) as i64;
    let doy = (153 * mp + 2) / 5 + d as i64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// Weekday with Monday = 0.
fn weekday_mon0(y: i32, m: u32, d: u32) -> u32 {
    let z = days_from_civil(y, m, d);
    let sun0 = (z + 4).rem_euclid(7) as u32;
    (sun0 + 6) % 7
}

fn days_in_month(y: i32, m: u32) -> u32 {
    let (ny, nm) = if m == 12 { (y + 1, 1) } else { (y, m + 1) };
    (days_from_civil(ny, nm, 1) - days_from_civil(y, m, 1)) as u32
}

fn shift_month(y: i32, m: u32, delta: i32) -> (i32, u32) {
    let idx = (y * 12 + (m as i32 - 1) + delta).div_euclid(12);
    let rem = (y * 12 + (m as i32 - 1) + delta).rem_euclid(12);
    (idx, rem as u32 + 1)
}

fn month_button(label: &'static str, on_click: impl Fn() + 'static) -> Div {
    let hovered = signal(false);
    let button = div()
        .w(22.0)
        .h(22.0)
        .rounded(5.0_f32)
        .flex()
        .items_center()
        .justify_center()
        .cursor(CursorIcon::Pointer)
        .dyn_style(move |s| {
            if hovered.get() {
                s.bg(color(SURFACE_RAISED))
            } else {
                s.bg(Color::TRANSPARENT)
            }
        })
        .on_click(move |_| on_click())
        .child(ui_text(label, FONT_SIZE, TEXT_MUTED));
    track_hover(button, hovered)
}

pub fn calendar_body(state: AppState, monitor: String) -> Div {
    let now = LocalTime::now();
    let shown: Signal<(i32, u32)> = signal((now.year, u32::from(now.month)));
    // Reopening always lands on the current month.
    {
        let state = state.clone();
        let monitor = monitor.clone();
        let was_open = Rc::new(Cell::new(false));
        effect(move || {
            let open = state.is_open(MenuKind::Calendar, &monitor);
            if open && !was_open.get() {
                let now = LocalTime::now();
                shown.set_if_changed((now.year, u32::from(now.month)));
            }
            was_open.set(open);
        });
    }

    let header = div()
        .h(24.0)
        .flex()
        .flex_row()
        .items_center()
        .child(month_button("‹", move || {
            let (y, m) = shown.get_untracked();
            shown.set(shift_month(y, m, -1));
        }))
        .child(
            div()
                .flex_1()
                .flex()
                .justify_center()
                .cursor(CursorIcon::Pointer)
                .on_click(move |_| {
                    let now = LocalTime::now();
                    shown.set((now.year, u32::from(now.month)));
                })
                .child(ui_text(
                    move || {
                        let (y, m) = shown.get();
                        format!("{} {}", MONTHS[(m - 1) as usize], y)
                    },
                    FONT_SIZE,
                    TEXT,
                )),
        )
        .child(month_button("›", move || {
            let (y, m) = shown.get_untracked();
            shown.set(shift_month(y, m, 1));
        }));

    let weekdays = ["mo", "tu", "we", "th", "fr", "sa", "su"].into_iter().fold(
        div().flex().flex_row(),
        |row, d| {
            row.child(div().flex_1().flex().justify_center().child(mono_text(
                d,
                FONT_SIZE - 3.0,
                TEXT_FAINT,
            )))
        },
    );

    let grid = dyn_view(move || {
        let (y, m) = shown.get();
        let today = LocalTime::now();
        let lead = weekday_mon0(y, m, 1) as i32;
        let dim = days_in_month(y, m) as i32;
        let prev_dim = {
            let (py, pm) = shift_month(y, m, -1);
            days_in_month(py, pm) as i32
        };
        let mut col = div().flex().flex_col();
        for week in 0..6 {
            let mut row = div().flex().flex_row();
            for dow in 0..7 {
                let i = week * 7 + dow;
                let day_num = i - lead + 1;
                let (label, in_month) = if day_num < 1 {
                    (prev_dim + day_num, false)
                } else if day_num > dim {
                    (day_num - dim, false)
                } else {
                    (day_num, true)
                };
                let is_today = in_month
                    && y == today.year
                    && m == u32::from(today.month)
                    && day_num == i32::from(today.day);
                let cell_text = ui_text(
                    label.to_string(),
                    FONT_SIZE - 1.0,
                    if is_today {
                        GROUND
                    } else if in_month {
                        TEXT
                    } else {
                        TEXT_FAINT
                    },
                );
                let cell = div()
                    .flex_1()
                    .h(28.0)
                    .flex()
                    .items_center()
                    .justify_center();
                let cell = if is_today {
                    cell.child(
                        div()
                            .w(24.0)
                            .h(24.0)
                            .rounded(12.0_f32)
                            .bg(color(RED))
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(cell_text),
                    )
                } else {
                    cell.child(cell_text)
                };
                row = row.child(cell);
            }
            col = col.child(row);
        }
        col
    });

    menu_card(
        300.0,
        div()
            .flex()
            .flex_col()
            .gap(8.0_f32)
            .child(header)
            .child(weekdays)
            .child(grid),
    )
}

// ---------------------------------------------------------------------------
// Battery popover (display-only)

fn fmt_secs(secs: u64) -> String {
    if secs == 0 {
        return String::new();
    }
    let h = secs / 3600;
    let m = (secs % 3600) / 60;
    if h > 0 {
        format!("{h}h {m}m")
    } else {
        format!("{m}m")
    }
}

pub fn battery_body() -> Div {
    menu_card(
        240.0,
        div().child(dyn_view(move || {
            let bat = battery();
            let present = bat.available();
            let pct = bat.percentage().round();
            let charging = bat.charging();
            let low = present && !charging && pct <= 20.0;
            let state_text = if !present {
                "no battery".to_owned()
            } else if bat.state() == BatteryState::Full {
                "full".to_owned()
            } else if charging {
                match bat.time_to_full() {
                    Some(t) if t.as_secs() > 0 => {
                        format!("charging · {} to full", fmt_secs(t.as_secs()))
                    }
                    _ => "charging".to_owned(),
                }
            } else {
                match bat.time_to_empty() {
                    Some(t) if t.as_secs() > 0 => format!("{} left", fmt_secs(t.as_secs())),
                    _ => "discharging".to_owned(),
                }
            };
            let fill_w = ((240.0 - 24.0 - 4.0) * pct / 100.0).max(2.0) as f32;
            let mut head = div()
                .flex()
                .flex_row()
                .items_center()
                .gap(8.0_f32)
                .child(mono_text("bat", FONT_SIZE - 3.0, TEXT_FAINT))
                .child(ui_text(
                    format!("{pct:.0}%"),
                    FONT_SIZE + 2.0,
                    if low { RED } else { TEXT },
                ));
            if charging {
                head = head.child(ui_text("⚡", FONT_SIZE, PURPLE));
            }
            div()
                .flex()
                .flex_col()
                .gap(10.0_f32)
                .child(head)
                .child(
                    div()
                        .relative()
                        .w_full()
                        .h(8.0)
                        .rounded(2.0_f32)
                        .border(1.0_f32)
                        .border_color(color(LINE))
                        .child(
                            div()
                                .absolute()
                                .left(1.0)
                                .top(1.0)
                                .w(fill_w)
                                .h(4.0)
                                .bg(color(if low {
                                    RED
                                } else if charging {
                                    PURPLE
                                } else {
                                    TEXT_MUTED
                                })),
                        ),
                )
                .child(ui_text(
                    state_text,
                    FONT_SIZE - 1.0,
                    if charging { PURPLE } else { TEXT_MUTED },
                ))
        })),
    )
}

// ---------------------------------------------------------------------------
// Start menu: fullscreen search + app grid + session row

fn session_button(label: &'static str, on_click: impl Fn() + 'static) -> Div {
    let hovered = signal(false);
    let button = div()
        .w(72.0)
        .h(30.0)
        .rounded(6.0_f32)
        .border(1.0_f32)
        .border_color(color(LINE))
        .flex()
        .items_center()
        .justify_center()
        .cursor(CursorIcon::Pointer)
        .dyn_style(move |s| {
            if hovered.get() {
                s.bg(color(SURFACE_RAISED))
            } else {
                s.bg(bar_bg())
            }
        })
        .on_click(move |e: &PointerEvent| {
            e.stop_propagation();
            on_click();
        })
        .child(ui_text(label, FONT_SIZE - 1.0, TEXT_MUTED));
    track_hover(button, hovered)
}

fn app_cell(state: AppState, app: Application, first_hit: bool) -> Div {
    let hovered = signal(false);
    let launch_app = app.clone();
    let cell = div()
        .w(112.0)
        .h(104.0)
        .rounded(8.0_f32)
        .border(1.0_f32)
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .gap(6.0_f32)
        .cursor(CursorIcon::Pointer)
        .dyn_style(move |s| {
            let s = s.border_color(color(if first_hit { RED_DIM } else { LINE }));
            if hovered.get() {
                s.bg(color(SURFACE_RAISED))
            } else {
                s.bg(Color::TRANSPARENT)
            }
        })
        .on_click(move |e: &PointerEvent| {
            e.stop_propagation();
            let close = state.clone();
            launch_app.launch().then(move |_| close.close_all_menus());
        });
    let cell = match app.icon_path(96) {
        Some(path) => cell.child(img(path).w(44.0).h(44.0).object_fit(ObjectFit::Contain)),
        None => cell.child(div().w(44.0).h(44.0)),
    };
    let cell = cell.child(
        div()
            .max_w(104.0)
            .overflow_hidden()
            .flex()
            .justify_center()
            .child(ui_text(
                elide(&app.name, 200.0, FONT_SIZE - 1.0),
                FONT_SIZE - 1.0,
                TEXT_MUTED,
            )),
    );
    track_hover(cell, hovered)
}

pub fn start_body(state: AppState, output: Output) -> Div {
    let query: Signal<String> = signal(String::new());
    let input_ref = node_ref();
    request_frame(move |_| input_ref.focus());
    let out_w = output.logical_size.width as f32;
    let columns = (((out_w - 96.0).min(1400.0)) / 112.0).floor().max(1.0) as usize;

    let submit_state = state.clone();
    let escape_state = state.clone();
    let search = text_field(query)
        .font_family(FONT_FAMILY)
        .placeholder("search…")
        .node_ref(input_ref)
        .on_submit(move |text| {
            let results = apps().search(text);
            if let Some(first) = results.into_iter().next() {
                let close = submit_state.clone();
                first.launch().then(move |_| close.close_all_menus());
            }
        })
        .on_key_down(move |e: &KeyboardEvent| {
            if e.key == Key::Named(NamedKey::Escape) {
                escape_state.close_all_menus();
            }
        })
        .on_click(|e: &PointerEvent| e.stop_propagation());

    let grid_state = state.clone();
    let grid = div()
        .flex_1()
        .min_h(0.0)
        .w_full()
        .overflow_y_scroll()
        .child(dyn_view(move || {
            let q = query.get();
            let results = if q.trim().is_empty() {
                apps().all()
            } else {
                apps().search(&q)
            };
            let searching = !q.trim().is_empty();
            let mut col = div().flex().flex_col().items_center();
            for (row_idx, chunk) in results.chunks(columns).enumerate() {
                let mut row = div().flex().flex_row();
                for (i, app) in chunk.iter().enumerate() {
                    let first_hit = searching && row_idx == 0 && i == 0;
                    row = row.child(app_cell(grid_state.clone(), app.clone(), first_hit));
                }
                col = col.child(row);
            }
            col
        }));

    let session_row = div()
        .flex()
        .flex_row()
        .justify_center()
        .gap(8.0_f32)
        .pb(20.0_f32)
        .child(session_button("logout", || {
            let _ = std::process::Command::new("sh")
                .args(["-c", "loginctl terminate-session \"$XDG_SESSION_ID\""])
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn();
        }))
        .child(session_button("sleep", || {
            let _ = session().suspend();
        }))
        .child(session_button("reboot", || {
            let _ = session().reboot();
        }))
        .child(session_button("off", || {
            let _ = session().power_off();
        }));

    let close_state = state.clone();
    div()
        .w_full()
        .h_full()
        .bg(color_alpha(GROUND, 0.88))
        .selection_color(selection_bg())
        .scrollbar_color(scrollbar_fg())
        .on_click(move |_| close_state.close_all_menus())
        .flex()
        .flex_col()
        .items_center()
        .child(div().h(36.0))
        .child(div().w(520.0).max_w(out_w - 96.0).child(search))
        .child(div().h(16.0))
        .child(grid)
        .child(session_row)
}

// ---------------------------------------------------------------------------
// Surface installation (per output, effect-driven like the QML bindings)

fn dropdown(
    state: AppState,
    output: Output,
    kind: MenuKind,
    options: LayerOptions,
    body: impl Fn(AppState, String) -> Div + 'static,
) {
    let body: Rc<dyn Fn(AppState, String) -> Div> = Rc::new(body);
    let monitor = output.name.clone();
    effect(move || {
        if !state.is_open(kind, &monitor) {
            return;
        }
        let body_state = state.clone();
        let body_monitor = monitor.clone();
        let body = Rc::clone(&body);
        let handle = open_layer(options.clone(), move || body(body_state, body_monitor));
        on_cleanup(move || handle.close());
    });
}

pub fn install(state: AppState) {
    for_each_output(move |output| {
        // Click-away backdrop for dropdowns (Top layer, below the Overlay
        // menus; not used for the start menu).
        {
            let state = state.clone();
            let output = output.clone();
            effect(move || {
                if !state.dropdown_open() {
                    return;
                }
                let close_state = state.clone();
                let handle = open_layer(
                    LayerOptions::new()
                        .layer(Layer::Top)
                        .anchor(Anchor::ALL)
                        .exclusive(Exclusive::None)
                        .output(&output)
                        .namespace("minkashell-backdrop"),
                    move || {
                        div()
                            .w_full()
                            .h_full()
                            .on_pointer_down(move |_| close_state.close_all_menus())
                    },
                );
                on_cleanup(move || handle.close());
            });
        }

        let base = LayerOptions::new()
            .layer(Layer::Overlay)
            .exclusive(Exclusive::None)
            .output(&output);

        dropdown(
            state.clone(),
            output.clone(),
            MenuKind::Calendar,
            base.clone()
                .anchor(Anchor::TOP)
                .margin(Margin {
                    top: MENU_TOP_MARGIN,
                    ..Default::default()
                })
                .namespace("minkashell-calendar"),
            calendar_body,
        );
        dropdown(
            state.clone(),
            output.clone(),
            MenuKind::Battery,
            base.clone()
                .anchor(Anchor::TOP | Anchor::RIGHT)
                .margin(Margin {
                    top: MENU_TOP_MARGIN,
                    right: 8,
                    ..Default::default()
                })
                .namespace("minkashell-battery"),
            |_state, _monitor| battery_body(),
        );
        dropdown(
            state.clone(),
            output.clone(),
            MenuKind::Status,
            base.clone()
                .anchor(Anchor::TOP | Anchor::RIGHT)
                .margin(Margin {
                    top: MENU_TOP_MARGIN,
                    right: 8,
                    ..Default::default()
                })
                .namespace("minkashell-status"),
            |state, monitor| status::status_body(state, &monitor),
        );

        // Start menu: fullscreen Overlay with exclusive keyboard.
        {
            let state = state.clone();
            let output = output.clone();
            effect(move || {
                if !state.is_open(MenuKind::Start, &output.name) {
                    return;
                }
                let body_state = state.clone();
                let body_output = output.clone();
                let handle = open_layer(
                    LayerOptions::new()
                        .layer(Layer::Overlay)
                        .anchor(Anchor::ALL)
                        .exclusive(Exclusive::None)
                        .keyboard(Keyboard::Exclusive)
                        .output(&output)
                        .namespace("minkashell-start"),
                    move || start_body(body_state, body_output),
                );
                on_cleanup(move || handle.close());
            });
        }
    });
}

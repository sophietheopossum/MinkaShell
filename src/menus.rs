//! Dropdown menus, start menu, and the click-away backdrop.
//! QML used layer surfaces for all of these (no xdg popups), Overlay for
//! menus over a Top-layer backdrop; same arrangement here.

use crate::state::{AppState, MenuKind};
use crate::status;
use crate::theme::*;
use anti_webui::prelude::*;
use anti_webui::shell::prelude::*;
use anti_webui_md3e::ColorScheme;
use anti_webui_md3e::components::{
    ButtonSize, ButtonStyle, CardKind, IconButtonStyle, MaterialShape, ShapeStyle,
    TextFieldStyle, button_group, clickable_card, linear_progress_colored, outlined_text_field,
    sized_icon, styled_button, styled_icon_button, styled_shape, wavy_progress_colored,
};
use anti_webui_md3e::icons::{self, Symbol};
use std::cell::Cell;
use std::rc::Rc;

const MENU_TOP_MARGIN: i32 = 38;

/// The emphasized easing of Material's motion: quick to start, a long settle.
fn emphasized(ms: u64) -> Transition {
    Transition::ms(ms).cubic_bezier(0.05, 0.7, 0.1, 1.0)
}

/// What a popover sits on. It comes in as a Material menu does, from a
/// little smaller and faded, each time it is opened.
pub fn menu_card(width: f32, content: Div) -> Div {
    let card = div()
        .w(width)
        .rounded(10.0_f32)
        .bg(bar_bg())
        .border(1.0_f32)
        .border_color(color(LINE))
        .p(12.0_f32)
        .child(content);
    // Built once: the condition never changes, so the view is asked for once.
    let card = Cell::new(Some(card));
    div().child(
        show(|| true, move || card.take().unwrap_or_else(div))
            .appear()
            .enter(|s| s.opacity(0.0_f32).scale(0.94_f32).translate_y(-6.0_f32))
            .transition(emphasized(220)),
    )
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

/// Previous or next month: a Material icon button.
fn month_button(symbol: &'static Symbol, on_click: impl Fn() + 'static) -> Div {
    styled_icon_button(symbol, IconButtonStyle::standard().size(ButtonSize::ExtraSmall))
        .on_click(move |_| on_click())
}

const CELL_H: f32 = 28.0;

/// One month as six weeks of seven days, Monday first, in local time.
fn month_grid(y: i32, m: u32) -> Div {
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
                .h(CELL_H)
                .flex()
                .items_center()
                .justify_center();
            let cell = if is_today {
                // Today on a Material shape, dark on red.
                cell.child(
                    div()
                        .relative()
                        .size(26.0)
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(
                            styled_shape(
                                || MaterialShape::Cookie9Sided,
                                ShapeStyle::new().color(|c| c.primary),
                            )
                            .absolute()
                            .left(0.0)
                            .top(0.0)
                            .size(26.0),
                        )
                        .child(cell_text.font_weight(FontWeight(600))),
                )
            } else {
                cell.child(cell_text)
            };
            row = row.child(cell);
        }
        col = col.child(row);
    }
    col
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
        .flex()
        .flex_row()
        .items_center()
        .child(month_button(icons::CHEVRON_LEFT, move || {
            let (y, m) = shown.get_untracked();
            shown.set(shift_month(y, m, -1));
        }))
        .child(
            div().flex_1().flex().justify_center().child(
                // The month: a click goes back to this one.
                styled_button(
                    move || {
                        let (y, m) = shown.get();
                        format!("{} {}", MONTHS[(m - 1) as usize], y)
                    },
                    ButtonStyle::text().size(ButtonSize::Small),
                )
                .h(28.0)
                .dyn_style(|s| s.text_color(color(TEXT)))
                .on_click(move |_| {
                    let now = LocalTime::now();
                    shown.set((now.year, u32::from(now.month)));
                }),
            ),
        )
        .child(month_button(icons::CHEVRON_RIGHT, move || {
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

    // The month fades in when it changes. Both stand in the same place while
    // one goes and the other comes.
    let grid = div().relative().w_full().h(CELL_H * 6.0).child(
        for_each(
            move || vec![shown.get()],
            |month| *month,
            |(y, m)| month_grid(y, m).absolute().left(0.0).top(0.0).right(0.0),
        )
        .enter(|s| s.opacity(0.0_f32).translate_y(8.0_f32))
        .exit(|s| s.opacity(0.0_f32))
        .transition(emphasized(200)),
    );

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

/// The battery's (percent, charging, low) now.
fn battery_now() -> (f64, bool, bool) {
    let bat = battery();
    let pct = bat.percentage().round();
    let charging = bat.charging();
    (pct, charging, bat.available() && !charging && pct <= 20.0)
}

/// The bar's line and track: red when low, purple while charging, muted
/// otherwise.
fn battery_colors(c: &ColorScheme) -> (Color, Color) {
    match battery_now() {
        (_, _, true) => (c.primary, c.primary_container),
        (_, true, _) => (c.secondary, c.secondary_container),
        _ => (c.on_surface_variant, c.outline_variant),
    }
}

pub fn battery_body() -> Div {
    let head = div().child(dyn_view(|| {
        let (pct, charging, low) = battery_now();
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
            head = head.child(sized_icon(icons::BOLT, 16.0).text_color(color(PURPLE)));
        }
        head
    }));
    // The bar is built again only when charging starts or stops: then it
    // turns wavy, or straight. Its value and colors follow by themselves.
    let charging = memo(|| battery().charging());
    let bar = div().w_full().child(dyn_view(move || {
        let fill = || (battery().percentage() / 100.0) as f32;
        if charging.get() {
            wavy_progress_colored(fill, Transition::ms(500).linear(), battery_colors).w_full()
        } else {
            linear_progress_colored(fill, battery_colors).w_full()
        }
    }));
    let line = div().child(dyn_view(|| {
        let bat = battery();
        let present = bat.available();
        let charging = bat.charging();
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
        ui_text(
            state_text,
            FONT_SIZE - 1.0,
            if charging { PURPLE } else { TEXT_MUTED },
        )
    }));
    menu_card(
        240.0,
        div()
            .flex()
            .flex_col()
            .gap(10.0_f32)
            .child(head)
            .child(bar)
            .child(line),
    )
}

// ---------------------------------------------------------------------------
// Start menu: fullscreen search + app grid + session row

/// One of the session buttons: outlined, with its icon.
fn session_button(
    label: &'static str,
    symbol: &'static Symbol,
    on_click: impl Fn() + 'static,
) -> Div {
    styled_button(
        label,
        ButtonStyle::outlined().size(ButtonSize::Small).icon(symbol),
    )
    .on_click(move |e: &PointerEvent| {
        e.stop_propagation();
        on_click();
    })
}

const CELL_W: f32 = 112.0;

/// An application: a Material card with its icon and name. The first match
/// of a search, which Enter launches, has a red outline.
fn app_cell(state: AppState, app: Application, first_hit: Memo<Option<String>>) -> Div {
    let launch_app = app.clone();
    let id = app.id.clone();
    let card = clickable_card(CardKind::Outlined)
        .w(CELL_W - 8.0)
        .h(104.0)
        .m(4.0)
        .items_center()
        .justify_center()
        .gap(6.0_f32)
        .dyn_style(move |s| {
            if first_hit.with(|first| first.as_deref() == Some(id.as_str())) {
                s.border_color(color(RED_DIM))
            } else {
                s
            }
        })
        .on_click(move |e: &PointerEvent| {
            e.stop_propagation();
            let close = state.clone();
            launch_app.launch().then(move |_| close.close_all_menus());
        });
    let card = match app.icon_path(96) {
        Some(path) => card.child(img(path).w(44.0).h(44.0).object_fit(ObjectFit::Contain)),
        None => card.child(div().w(44.0).h(44.0)),
    };
    card.child(
        div()
            .max_w(96.0)
            .overflow_hidden()
            .flex()
            .justify_center()
            .child(ui_text(
                elide(&app.name, 192.0, FONT_SIZE - 1.0),
                FONT_SIZE - 1.0,
                TEXT_MUTED,
            )),
    )
}

pub fn start_body(state: AppState, output: Output) -> Div {
    let query: Signal<String> = signal(String::new());
    let input_ref = node_ref();
    request_frame(move |_| input_ref.focus());
    let out_w = output.logical_size.width as f32;
    let columns = (((out_w - 96.0).min(1400.0)) / CELL_W).floor().max(1.0) as usize;

    let submit_state = state.clone();
    let search = outlined_text_field(
        query,
        TextFieldStyle::new()
            .placeholder("search…")
            .leading(sized_icon(icons::SEARCH, 20.0).text_color(color(TEXT_MUTED)))
            .input_ref(input_ref)
            .on_submit(move |text| {
                let results = apps().search(text);
                if let Some(first) = results.into_iter().next() {
                    let close = submit_state.clone();
                    first.launch().then(move |_| close.close_all_menus());
                }
            }),
    )
    .w_full()
    .on_click(|e: &PointerEvent| e.stop_propagation());

    let searching = move || !query.with(|q| q.trim().is_empty());
    let found = move || {
        if searching() {
            query.with(|q| apps().search(q))
        } else {
            apps().all()
        }
    };
    // The application Enter launches: the first match while searching.
    let first_hit = memo(move || {
        if searching() {
            found().into_iter().next().map(|app| app.id)
        } else {
            None
        }
    });
    // Keyed by application, so a search keeps the cards that still match:
    // they move to their new places, and the others fade away.
    let grid_state = state.clone();
    let grid = div()
        .flex_1()
        .min_h(0.0)
        .w_full()
        .overflow_y_scroll()
        .child(
            // The scroller measures its direct child, so the centring row is
            // that child and the wrapping grid sits in it.
            div().w_full().flex().flex_row().justify_center().child(
                div()
                    .w(CELL_W * columns as f32)
                    .flex()
                    .flex_row()
                    .flex_wrap()
                    .child(
                        for_each(found, |app| app.id.clone(), move |app| {
                            app_cell(grid_state.clone(), app, first_hit)
                        })
                        .enter(|s| s.opacity(0.0_f32).scale(0.9_f32))
                        .exit(|s| s.opacity(0.0_f32).scale(0.9_f32))
                        .transition(emphasized(180))
                        .moves(emphasized(220)),
                    ),
            ),
        );

    let session_row = div().flex().flex_row().justify_center().pb(20.0_f32).child(button_group([
        session_button("logout", icons::LOGOUT, || {
            let _ = std::process::Command::new("sh")
                .args(["-c", "loginctl terminate-session \"$XDG_SESSION_ID\""])
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn();
        }),
        session_button("sleep", icons::BEDTIME, || {
            let _ = session().suspend();
        }),
        session_button("reboot", icons::RESTART_ALT, || {
            let _ = session().reboot();
        }),
        session_button("off", icons::POWER_SETTINGS_NEW, || {
            let _ = session().power_off();
        }),
    ]));

    let close_state = state.clone();
    let escape_state = state.clone();
    let panel = div()
        .w_full()
        .h_full()
        .bg(color_alpha(GROUND, 0.88))
        .selection_color(selection_bg())
        .scrollbar_color(scrollbar_fg())
        .on_click(move |_| close_state.close_all_menus())
        // Escape from anywhere in it, the search field included.
        .on_key_down(move |e: &KeyboardEvent| {
            if e.key == Key::Named(NamedKey::Escape) {
                escape_state.close_all_menus();
            }
        })
        .flex()
        .flex_col()
        .items_center()
        .child(div().h(36.0))
        .child(div().w(520.0).max_w(out_w - 96.0).child(search))
        .child(div().h(16.0))
        .child(grid)
        .child(session_row);
    // It fades in, a little larger first, each time it is opened.
    let panel = Cell::new(Some(panel));
    div().w_full().h_full().child(
        show(|| true, move || panel.take().unwrap_or_else(div))
            .appear()
            .enter(|s| s.opacity(0.0_f32).scale(1.02_f32))
            .transition(emphasized(240)),
    )
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

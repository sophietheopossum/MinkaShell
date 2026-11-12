//! Notification popups over AntiWebUI's `org.freedesktop.Notifications`
//! daemon (the QML `NotificationServer` + `NotifPopup.qml`).
//!
//! The daemon owns expiry itself (sender timeout; critical never), so the
//! QML's per-card 6s timer and hover-pause have no equivalent here; cards
//! simply render `server.popups()` while the daemon keeps them up.

use crate::bar::track_hover;
use crate::settings;
use crate::state::AppState;
use crate::theme::*;
use anti_webui::prelude::*;
use anti_webui::shell::prelude::*;

fn action_button(notification: Notification, key: String, label: String) -> Div {
    let hovered = signal(false);
    let button = div()
        .px(9.0_f32)
        .py(5.0_f32)
        .rounded(6.0_f32)
        .border(1.0_f32)
        .flex()
        .items_center()
        .cursor(CursorIcon::Pointer)
        .dyn_style(move |s| {
            let s = s.border_color(color(if hovered.get() { RED } else { LINE }));
            if hovered.get() {
                s.bg(color(RED_DIM))
            } else {
                s.bg(color(SURFACE_RAISED))
            }
        })
        .on_click(move |e: &PointerEvent| {
            e.stop_propagation();
            notification.invoke(&key);
        })
        .child(ui_text(label, FONT_SIZE - 1.0, TEXT));
    track_hover(button, hovered)
}

fn card(notification: Notification) -> Div {
    let hovered = signal(false);
    let dismiss = notification.clone();
    let mut col = div()
        .flex()
        .flex_col()
        .gap(3.0_f32)
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap(6.0_f32)
                .child(div().w(6.0).h(6.0).rounded(3.0_f32).bg(color(RED)))
                .child(
                    mono_text(
                        if notification.app_name.is_empty() {
                            "notification".to_owned()
                        } else {
                            notification.app_name.clone()
                        },
                        FONT_SIZE - 3.0,
                        TEXT_FAINT,
                    )
                    .letter_spacing(1.0_f32),
                ),
        )
        .child(
            ui_text(notification.summary.clone(), FONT_SIZE, TEXT).font_weight(FontWeight::BOLD),
        );
    let body = notification.body_text();
    if !body.is_empty() {
        col = col.child(ui_text(body, FONT_SIZE - 1.0, TEXT_MUTED));
    }
    if !notification.actions.is_empty() {
        let mut row = div().flex().flex_row().gap(6.0_f32);
        for (key, label) in &notification.actions {
            if key == "default" {
                continue;
            }
            row = row.child(action_button(
                notification.clone(),
                key.clone(),
                label.clone(),
            ));
        }
        col = col.child(row);
    }
    let card = div()
        .w_full()
        .rounded(10.0_f32)
        .bg(bar_bg())
        .border(1.0_f32)
        .p(10.0_f32)
        .cursor(CursorIcon::Pointer)
        .dyn_style(move |s| s.border_color(color(if hovered.get() { RED } else { LINE })))
        .on_click(move |_| dismiss.dismiss())
        .child(col);
    track_hover(card, hovered)
}

pub fn install(state: AppState) {
    for_each_output(move |output| {
        let state = state.clone();
        let output = output.clone();
        let any_popups = {
            let state = state.clone();
            let output = output.clone();
            memo(move || {
                let settings_now = state.settings.get();
                if !settings::show_bar_on(&settings_now, &outputs(), &output) {
                    return false;
                }
                state
                    .notifs()
                    .is_some_and(|server| !server.popups().is_empty())
            })
        };
        effect(move || {
            if !any_popups.get() {
                return;
            }
            let Some(server) = state.notifs() else {
                return;
            };
            let max_h = (output.logical_size.height as f32 - BAR_HEIGHT - 24.0).max(80.0);
            let handle = open_layer(
                LayerOptions::new()
                    .layer(Layer::Top)
                    .anchor(Anchor::TOP | Anchor::RIGHT)
                    .margin(Margin {
                        top: BAR_HEIGHT as i32 + 6,
                        right: 8,
                        ..Default::default()
                    })
                    .width(340)
                    .exclusive(Exclusive::None)
                    .output(&output)
                    .namespace("minkashell-notifications"),
                move || {
                    let server = server.clone();
                    div()
                        .w(340.0)
                        .max_h(max_h)
                        .overflow_hidden()
                        .child(dyn_view(move || {
                            let mut col = div().flex().flex_col().gap(8.0_f32);
                            for notification in server.popups() {
                                col = col.child(card(notification));
                            }
                            col
                        }))
                },
            );
            on_cleanup(move || handle.close());
        });
    });
}

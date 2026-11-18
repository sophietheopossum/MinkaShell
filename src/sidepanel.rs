//! Duo-mode ScreenPad side column (`modules/screenpad/SidePanel.qml`):
//! everything the bar holds in general layout, stacked in a fixed 400px
//! column whose exclusive zone leaves MinkaMon the rest of the pad.

use crate::bar;
use crate::state::AppState;
use crate::theme::*;
use anti_webui::prelude::*;
use anti_webui::shell::prelude::*;
use anti_webui_md3e::components::divider;

pub const SIDE_WIDTH: i32 = 400;

pub fn side_body(state: AppState, output: Output) -> Div {
    let monitor = output.name.clone();
    div()
        .relative()
        .w_full()
        .h_full()
        .bg(bar_bg())
        .child(
            div()
                .absolute()
                .left(0.0)
                .top(0.0)
                .bottom(0.0)
                .w(1.0)
                .bg(color(LINE)),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .px(12.0_f32)
                .pt(10.0_f32)
                .gap(10.0_f32)
                .child(bar::focused_title(state.clone(), monitor.clone()))
                .child(bar::window_controls(state.clone(), monitor.clone()))
                .child(divider())
                .child(bar::tray_cluster(state.clone(), output.clone()))
                .child(divider())
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .child(bar::clock_pill(state.clone(), monitor.clone())),
                )
                .child(bar::workspaces(state.clone(), monitor.clone()).flex_wrap())
                .child(bar::sys_usage(state.clone()))
                .child(bar::updates_pill(state.clone())),
        )
        .child(
            div()
                .absolute()
                .left(12.0)
                .bottom(10.0)
                .flex()
                .flex_row()
                .items_center()
                .gap(12.0_f32)
                .child(bar::start_button(state.clone(), monitor))
                .child(bar::health_dot(state)),
        )
}

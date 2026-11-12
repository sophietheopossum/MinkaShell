//! MinkaShell — the Minka session shell (AntiWebUI port of the Quickshell
//! shell in `shell.qml` + `modules/`). One surface set per output;
//! ShellLayout rules decide which outputs carry persistent surfaces.

#[allow(dead_code)]
mod bar;
mod dock;
mod instance;
#[allow(dead_code)]
mod ipc;
mod menus;
mod notifications;
mod settings;
mod sidepanel;
#[allow(dead_code)]
mod state;
mod status;
#[allow(dead_code)]
mod theme;

use anti_webui::prelude::*;
use anti_webui::shell::prelude::*;
use state::AppState;
use theme::*;

fn wallpaper_body(path: String) -> Div {
    div()
        .w_full()
        .h_full()
        .bg(Color::from_rgb(0x000000))
        .child(img(path).w_full().h_full().object_fit(ObjectFit::Cover))
}

fn install_output_surfaces(state: AppState) {
    for_each_output(move |output| {
        // Wallpaper: every output, unmapped entirely when no path is set.
        {
            let state = state.clone();
            let output = output.clone();
            effect(move || {
                let path = state.settings.get().wallpaper;
                if path.is_empty() {
                    return;
                }
                let handle = open_layer(
                    LayerOptions::background()
                        .output(&output)
                        .namespace("minkashell-wallpaper"),
                    move || wallpaper_body(path.clone()),
                );
                on_cleanup(move || handle.close());
            });
        }

        // Bar: general layout only (Duo carries everything in the dock +
        // side panel instead).
        {
            let state = state.clone();
            let output = output.clone();
            effect(move || {
                let settings_now = state.settings.get();
                let all = outputs();
                if settings::duo_mode(&settings_now, &all)
                    || !settings::show_bar_on(&settings_now, &all, &output)
                {
                    return;
                }
                let body_state = state.clone();
                let body_output = output.clone();
                let handle = open_layer(
                    LayerOptions::bar(Anchor::TOP, BAR_HEIGHT as i32)
                        .output(&output)
                        .namespace("minkashell-bar"),
                    move || bar::bar_body(body_state, body_output),
                );
                on_cleanup(move || handle.close());
            });
        }
    });
}

fn install_dock(state: AppState) {
    for_each_output(move |output| {
        let state = state.clone();
        let output = output.clone();
        // (visible, duo) as one memo key: the layer reopens with the right
        // anchors when either flips, and not on every workspace broadcast.
        let shape = {
            let state = state.clone();
            let output = output.clone();
            memo(move || {
                let settings_now = state.settings.get();
                let all = outputs();
                let duo = settings::duo_mode(&settings_now, &all);
                let visible = settings::show_bar_on(&settings_now, &all, &output)
                    && !dock::dock_model(&state, &output.name).ids.is_empty();
                (visible, duo)
            })
        };
        effect(move || {
            let (visible, duo) = shape.get();
            if !visible {
                return;
            }
            let options = if duo {
                LayerOptions::new()
                    .anchor(Anchor::TOP | Anchor::LEFT | Anchor::RIGHT)
                    .height(DOCK_HEIGHT as i32)
                    .exclusive(Exclusive::Fixed(DOCK_HEIGHT as i32))
            } else {
                LayerOptions::new()
                    .anchor(Anchor::BOTTOM)
                    .height(DOCK_HEIGHT as i32)
                    .exclusive(Exclusive::Fixed(DOCK_HEIGHT as i32))
            };
            let body_state = state.clone();
            let body_output = output.clone();
            let handle = open_layer(
                options.output(&output).namespace("minkashell-dock"),
                move || dock::dock_body(body_state, body_output),
            );
            on_cleanup(move || handle.close());
        });
    });
}

fn install_side_panel(state: AppState) {
    for_each_output(move |output| {
        let state = state.clone();
        let output = output.clone();
        let visible = {
            let state = state.clone();
            let output = output.clone();
            memo(move || {
                let settings_now = state.settings.get();
                let all = outputs();
                settings::duo_mode(&settings_now, &all)
                    && settings::show_bar_on(&settings_now, &all, &output)
            })
        };
        effect(move || {
            if !visible.get() {
                return;
            }
            let body_state = state.clone();
            let body_output = output.clone();
            let handle = open_layer(
                LayerOptions::new()
                    .anchor(Anchor::TOP | Anchor::BOTTOM | Anchor::RIGHT)
                    .width(sidepanel::SIDE_WIDTH)
                    .exclusive(Exclusive::Auto)
                    .output(&output)
                    .namespace("minkashell-sidepanel"),
                move || sidepanel::side_body(body_state, body_output),
            );
            on_cleanup(move || handle.close());
        });
    });
}

fn shell_root(guard: instance::InstanceGuard) {
    let state = AppState::new(guard);
    state.start_services();
    install_output_surfaces(state.clone());
    install_dock(state.clone());
    install_side_panel(state.clone());
    menus::install(state.clone());
    notifications::install(state);
}

fn main() {
    let guard = match instance::claim() {
        instance::InstanceClaim::Primary(guard) => guard,
        instance::InstanceClaim::Duplicate => return,
    };

    if let Err(error) = Shell::new()
        .namespace("minkashell")
        .app_id("minkashell")
        .run(move || shell_root(guard))
    {
        eprintln!("minkashell: {error}");
        std::process::exit(1);
    }
}

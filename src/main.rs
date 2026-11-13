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
mod tray_rescue;

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

/// The dock and, in Duo, the side column: one effect for both, because the
/// order they are opened in decides the ScreenPad's layout.
///
/// The compositor gives a screen edge to exclusive layers in the order they
/// were first mapped. Duo is the dock across the whole top of the pad with
/// the column below it (SidePanel.qml, 2/8/2026), so the dock has to come
/// first. Opened on their own they came in whichever order events arrived:
/// the column first after a login, because the dock waits for the window
/// list, which left the dock 400 px short, and the dock first after the pad
/// was plugged back in. Here the column is reopened behind the dock whenever
/// the dock appears or goes, and at a start it waits until the window list
/// has said whether there is a dock, so it is not drawn and at once redrawn
/// 44 px lower.
fn install_dock_and_side_panel(state: AppState) {
    for_each_output(move |output| {
        let state = state.clone();
        let output = output.clone();
        // (dock, column, duo) as one memo key: the layers reopen when one of
        // them flips, and not on every workspace broadcast.
        let shape = {
            let state = state.clone();
            let output = output.clone();
            memo(move || {
                let settings_now = state.settings.get();
                let all = outputs();
                let duo = settings::duo_mode(&settings_now, &all);
                let here = settings::show_bar_on(&settings_now, &all, &output);
                let dock = here && !dock::dock_model(&state, &output.name).ids.is_empty();
                (dock, duo && here && state.windows_known.get(), duo)
            })
        };
        effect(move || {
            let (dock, column, duo) = shape.get();
            if dock {
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
            }
            if column {
                let body_state = state.clone();
                let body_output = output.clone();
                let handle = open_layer(
                    LayerOptions::new()
                        .anchor(Anchor::TOP | Anchor::BOTTOM | Anchor::RIGHT)
                        .width(sidepanel::SIDE_WIDTH)
                        // Fixed, not Auto: Auto reserves the surface's own
                        // width, which is still the toolkit's 640 px default
                        // at the first commit, and the column is now
                        // reopened more often than once a session.
                        .exclusive(Exclusive::Fixed(sidepanel::SIDE_WIDTH))
                        .output(&output)
                        .namespace("minkashell-sidepanel"),
                    move || sidepanel::side_body(body_state, body_output),
                );
                on_cleanup(move || handle.close());
            }
        });
    });
}

fn shell_root(guard: instance::InstanceGuard) {
    let state = AppState::new(guard);
    state.start_services();
    install_output_surfaces(state.clone());
    install_dock_and_side_panel(state.clone());
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

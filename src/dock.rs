//! The dock: window chips with drag-to-reorder, from `modules/dock/Dock.qml`
//! plus `DockReorder.js` (ported 1:1 at the bottom of this file) and
//! `DockMenu.qml` (a view popup here instead of a QML PopupWindow).
//!
//! QML laid chips out in a Row and dragged them with Translate offsets so
//! layout never moved. We do the algebraic equivalent: chips are
//! absolutely positioned at Row-equivalent slots plus the plan's shifts.

use crate::bar::{app_icon_path, track_hover};
use crate::ipc::Win;
use crate::settings;
use crate::state::AppState;
use crate::theme::*;
use anti_webui::core::geometry::RectI;
use anti_webui::prelude::*;
use anti_webui::shell::prelude::*;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::time::{Duration, Instant};

const SPACING: f32 = 6.0;
const CHIP_H: f32 = 32.0;
const STRIP_H: f32 = 40.0;
const CANCEL_BAND: f32 = 48.0;
const EDGE_ZONE: f32 = 40.0;
const MAX_SCROLL_SPEED: f32 = 900.0;
const DRAG_START: f32 = 10.0;

// ---------------------------------------------------------------------------
// DockReorder.js, ported. `ids` are window ids in chip order, `group_of`
// maps id -> workspace group key, `slots[i]` is chip i's row x/width.

#[derive(Clone, Copy)]
pub struct Slot {
    pub x: f32,
    pub w: f32,
}

#[derive(Clone, Copy, PartialEq)]
pub struct Plan {
    pub from: usize,
    pub to: usize,
    pub left: f32,
    pub width: f32,
    pub landing_x: f32,
    pub group_lo: f32,
    pub group_end: f32,
}

// The reorder math below never indexes on trust. The window list changes
// under a held pointer (a window opens or closes mid-drag), so an index or id
// from one list can meet another list, and a panic here ends the whole shell.
// A stale input gives no plan, no shift or no move instead.

/// The run of neighbours in `i`'s group. `i` must be inside `ids`.
fn group_span(ids: &[String], group_of: &HashMap<String, String>, i: usize) -> (usize, usize) {
    let key = group_of.get(&ids[i]);
    let mut start = i;
    let mut end = i;
    while start > 0 && group_of.get(&ids[start - 1]) == key {
        start -= 1;
    }
    while end + 1 < ids.len() && group_of.get(&ids[end + 1]) == key {
        end += 1;
    }
    (start, end)
}

fn landing_x(slots: &[Slot], from: usize, to: usize, w: f32) -> f32 {
    if to > from {
        slots[to].x + slots[to].w - w
    } else {
        slots[to].x
    }
}

#[allow(clippy::too_many_arguments)]
pub fn live_plan(
    ids: &[String],
    group_of: &HashMap<String, String>,
    slots: &[Slot],
    drag_id: &str,
    pointer_x: f32,
    grab_offset: f32,
    view_lo: f32,
    view_hi: f32,
    cancelled: bool,
) -> Option<Plan> {
    if slots.len() != ids.len() {
        return None;
    }
    let from = ids.iter().position(|i| i == drag_id)?;
    let w = slots.get(from)?.w;
    let (start, end) = group_span(ids, group_of, from);
    let group_lo = slots[start].x;
    let group_hi = (slots[end].x + slots[end].w - w).max(group_lo);
    let (mut lo, mut hi) = (group_lo.max(view_lo), group_hi.min(view_hi - w));
    if lo > hi {
        lo = group_lo;
        hi = group_hi;
    }
    let left = (pointer_x - grab_offset).clamp(lo, hi);
    let mut to = from;
    if !cancelled {
        let centre = left + w / 2.0;
        let half = (w + SPACING) / 2.0;
        let mut i = from + 1;
        while i <= end && centre > slots[i].x + slots[i].w / 2.0 - half {
            to = i;
            i += 1;
        }
        if to == from {
            let mut i = from as i64 - 1;
            while i >= start as i64
                && centre < slots[i as usize].x + slots[i as usize].w / 2.0 + half
            {
                to = i as usize;
                i -= 1;
            }
        }
    }
    Some(Plan {
        from,
        to,
        left,
        width: w,
        landing_x: landing_x(slots, from, to, w),
        group_lo,
        group_end: slots[end].x + slots[end].w,
    })
}

#[derive(Clone, Debug, PartialEq)]
pub struct Move {
    pub id: String,
    pub before_id: Option<String>,
}

pub fn pending_plan(
    ids: &[String],
    group_of: &HashMap<String, String>,
    slots: &[Slot],
    mv: &Move,
) -> Option<Plan> {
    if slots.len() != ids.len() {
        return None;
    }
    let from = ids.iter().position(|i| i == &mv.id)?;
    let w = slots.get(from)?.w;
    let (start, end) = group_span(ids, group_of, from);
    let to = match &mv.before_id {
        None => end,
        Some(before) => {
            let b = ids.iter().position(|i| i == before)?;
            if !(start..=end).contains(&b) {
                return None;
            }
            if b > from { b - 1 } else { b }
        }
    };
    let x = landing_x(slots, from, to, w);
    Some(Plan {
        from,
        to,
        left: x,
        width: w,
        landing_x: x,
        group_lo: slots[start].x,
        group_end: slots[end].x + slots[end].w,
    })
}

pub fn shifts_for(ids: &[String], slots: &[Slot], plan: Option<&Plan>) -> HashMap<String, f32> {
    let mut shifts = HashMap::new();
    let Some(plan) = plan else { return shifts };
    // A plan made from a longer list than this one moves nothing.
    if plan.from >= ids.len() || plan.to >= ids.len() || slots.len() != ids.len() {
        return shifts;
    }
    let gap = plan.width + SPACING;
    shifts.insert(ids[plan.from].clone(), plan.left - slots[plan.from].x);
    if plan.to > plan.from {
        for id in &ids[plan.from + 1..=plan.to] {
            shifts.insert(id.clone(), -gap);
        }
    }
    if plan.to < plan.from {
        for id in &ids[plan.to..plan.from] {
            shifts.insert(id.clone(), gap);
        }
    }
    shifts
}

/// The id to pass as `beforeId` for a move from `from` to `to`; `None` puts
/// the window at the end of its group. Both must be inside `ids`.
pub fn before_id_for(
    ids: &[String],
    group_of: &HashMap<String, String>,
    from: usize,
    to: usize,
) -> Option<String> {
    if from >= ids.len() {
        return None;
    }
    if to < from {
        return ids.get(to).cloned();
    }
    let (_, end) = group_span(ids, group_of, from);
    if to < end {
        ids.get(to + 1).cloned()
    } else {
        None
    }
}

pub fn move_satisfied(ids: &[String], group_of: &HashMap<String, String>, mv: &Move) -> bool {
    let Some(i) = ids.iter().position(|x| x == &mv.id) else {
        return true;
    };
    let group = group_of.get(&mv.id);
    match &mv.before_id {
        None => ids
            .get(i + 1)
            .is_none_or(|next| group_of.get(next) != group),
        Some(before) => {
            let Some(b) = ids.iter().position(|x| x == before) else {
                return true;
            };
            if group_of.get(before) != group {
                return true;
            }
            b == i + 1
        }
    }
}

pub fn same_members(
    held_ids: &[String],
    held_groups: &[String],
    live_ids: &[String],
    group_of: &HashMap<String, String>,
) -> bool {
    if held_ids.len() != live_ids.len() {
        return false;
    }
    held_ids
        .iter()
        .zip(held_groups)
        .all(|(id, g)| group_of.get(id) == Some(g))
}

// ---------------------------------------------------------------------------
// Dock model, rebuilt from the IPC view on every change (as in QML).

#[derive(Default)]
pub struct DockModel {
    pub ids: Vec<String>,
    pub by_id: HashMap<String, Win>,
    pub group_of: HashMap<String, String>,
}

fn estimate_width(win: &Win) -> f32 {
    16.0 + 20.0 + 7.0 + (win.label().chars().count() as f32 * 6.4).min(150.0)
}

#[derive(Clone)]
struct Drag {
    id: String,
    grab_offset: f32,
    /// Where the pointer is across the viewport, not along the row: the row
    /// can scroll under a pointer that is held still, and the chip has to
    /// stay under the pointer (QML's pointerViewX).
    view_x: f32,
    pointer_y: f32,
    /// Where the last motion had the pointer in the window, as the event
    /// gave it. A button event comes with the position of the motion before
    /// it, so the drag's own release is at exactly this point, and one that
    /// is not belongs to another press (see `on_pointer_up`).
    window: (f32, f32),
    plan: Option<Plan>,
}

struct Held {
    ids: Vec<String>,
    groups: Vec<String>,
    mv: Move,
}

struct Press {
    id: Option<String>,
    button: Option<PointerButton>,
    /// Where the pointer was in the window. Travel is measured in this: a row
    /// that scrolls or is centred anew under a still pointer does not change
    /// it, where a position on the strip would jump.
    window_x: f32,
    content_x: f32,
    /// How wide the row was. A press that sees it change (a window opened or
    /// closed, a title grew) stays a click and cannot become a drag: the
    /// chip under the pointer is no longer the one pressed (the QML's
    /// layoutShifted).
    row_w: f32,
}

#[derive(Default)]
struct DockLocal {
    widths: HashMap<String, f32>,
    slots: Vec<Slot>,
    shown: Vec<String>,
    press: Option<Press>,
    drag: Option<Drag>,
    held: Option<Held>,
    pending_gen: u64,
    suppress_click: bool,
    /// Width of the viewport the row scrolls in, and of the row itself.
    flick_w: f32,
    total_w: f32,
    /// The part of a wheel step that was less than a pixel, kept for the
    /// next one.
    wheel_rest: f32,
    /// Edge scrolling during a drag: a frame tick is pending, and where it
    /// has got to (scroll offsets are whole pixels, a slow step is not).
    edge_running: bool,
    edge_pos: Option<f32>,
    menu_win: Option<String>,
    /// In window pixels, which is what a popup's anchor is in.
    menu_anchor_x: f32,
    last_reorder_seq: u64,
    was_duo: bool,
}

fn slots_for(ids: &[String], widths: &HashMap<String, f32>, model: &DockModel) -> Vec<Slot> {
    let mut slots = Vec::with_capacity(ids.len());
    let mut x = 0.0;
    for id in ids {
        let w = widths
            .get(id)
            .copied()
            .unwrap_or_else(|| model.by_id.get(id).map_or(80.0, estimate_width));
        slots.push(Slot { x, w });
        x += w + SPACING;
    }
    slots
}

#[derive(Clone, Copy)]
struct ChipStyle {
    lifted: bool,
    cancel_armed: bool,
    dimmed: bool,
}

fn chip_view(
    local: Rc<RefCell<DockLocal>>,
    frame: Signal<u64>,
    id: String,
    win: Win,
    left: f32,
    style: ChipStyle,
) -> Div {
    let ChipStyle {
        lifted,
        cancel_armed,
        dimmed,
    } = style;
    let hovered = signal(false);
    let focused = win.focused;
    let measure_id = id.clone();
    let chip = div()
        .absolute()
        .left(left)
        .top(if lifted { 2.0 } else { 4.0 })
        .h(CHIP_H)
        .rounded(8.0_f32)
        .border(1.0_f32)
        .flex()
        .flex_row()
        .items_center()
        .gap(7.0_f32)
        .px(8.0_f32)
        .on_resized(move |w, _h| {
            if w > 1.0 {
                let mut local = local.borrow_mut();
                let old = local.widths.get(&measure_id).copied();
                if old.is_none_or(|o| (o - w).abs() > 0.5) {
                    local.widths.insert(measure_id.clone(), w);
                    drop(local);
                    frame.update(|n| *n += 1);
                }
            }
        })
        .dyn_style(move |s| {
            let s = if lifted || focused {
                s.bg(color(SURFACE_RAISED))
            } else if hovered.get() {
                s.bg(color(SURFACE))
            } else {
                s.bg(Color::TRANSPARENT)
            };
            let s = s.border_color(color(if lifted {
                if cancel_armed { TEXT_FAINT } else { RED }
            } else if focused {
                RED_DIM
            } else {
                LINE
            }));
            s.opacity(if dimmed {
                0.35_f32
            } else if lifted && cancel_armed {
                0.5
            } else {
                1.0
            })
        });
    let chip = match app_icon_path(&win.app_id, 40) {
        Some(path) => chip.child(img(path).w(20.0).h(20.0).object_fit(ObjectFit::Contain)),
        None => chip,
    };
    // Not to wrap: a chip is laid out inside the row, whose width is the sum
    // of the chips', so a lone chip with wrapping text could never be
    // measured wider than it first came out, and broke its title in two.
    let chip = chip.child(
        div()
            .max_w(150.0)
            .overflow_hidden()
            .whitespace_nowrap()
            .child(ui_text(
                elide(win.label(), 150.0, FONT_SIZE - 1.0),
                FONT_SIZE - 1.0,
                if lifted || focused { TEXT } else { TEXT_MUTED },
            )),
    );
    track_hover(chip, hovered)
}

pub fn dock_model(state: &AppState, monitor: &str) -> DockModel {
    let duo = settings::duo_mode(&state.settings.get_untracked(), &outputs());
    state.view.with(|view| {
        let mut model = DockModel::default();
        for mon in &view.monitors {
            if !duo && mon.name != monitor {
                continue;
            }
            for ws in &mon.workspaces {
                for win in &ws.windows {
                    model.ids.push(win.id.clone());
                    model
                        .group_of
                        .insert(win.id.clone(), format!("{}:{}", mon.name, ws.index));
                    model.by_id.insert(win.id.clone(), win.clone());
                }
            }
        }
        model
    })
}

fn release_held(local: &Rc<RefCell<DockLocal>>, frame: Signal<u64>) {
    local.borrow_mut().held = None;
    frame.update(|n| *n += 1);
}

fn schedule_release(
    local: Rc<RefCell<DockLocal>>,
    frame: Signal<u64>,
    generation: u64,
    after_ms: u64,
) {
    set_timeout(Duration::from_millis(after_ms), move || {
        if local.borrow().pending_gen == generation && local.borrow().held.is_some() {
            release_held(&local, frame);
        }
    });
}

fn hit_chip(local: &DockLocal, x: f32) -> Option<String> {
    local
        .shown
        .iter()
        .zip(&local.slots)
        .find(|(_, s)| x >= s.x && x <= s.x + s.w)
        .map(|(id, _)| id.clone())
}

/// Where a pointer event on the strip's overlay is, in the logical pixels the
/// slots are in: x across the viewport, y down the strip, and x along the
/// chip row. Events carry physical pixels, which only equal logical ones at
/// scale 1 (the ScreenPad; the main panel is at 1.25). The overlay covers the
/// viewport and does not scroll, so the row position is the viewport position
/// plus the scroll offset.
fn strip_point(e: &PointerEvent, scroller: NodeRef) -> (f32, f32, f32) {
    let scale = untrack(window_scale).as_f32();
    let view_x = e.local.x / scale;
    (
        view_x,
        e.local.y / scale,
        view_x + scroller.scroll_offset().x,
    )
}

/// The pointer's x in the window, in logical pixels.
fn window_x(e: &PointerEvent) -> f32 {
    e.position.x / untrack(window_scale).as_f32()
}

/// Drops a drag in progress and leaves the chip where it was. True if there
/// was one.
fn cancel_drag(local: &Rc<RefCell<DockLocal>>, frame: Signal<u64>) -> bool {
    {
        let mut local = local.borrow_mut();
        if local.drag.is_none() {
            return false;
        }
        local.drag = None;
        local.press = None;
        local.suppress_click = true;
    }
    frame.update(|n| *n += 1);
    true
}

/// A drag's plan for where its pointer is now, against the list on screen.
fn replan(local: &DockLocal, model: &DockModel, drag: &Drag, scroller: NodeRef) -> Option<Plan> {
    let offset = scroller.scroll_offset().x;
    live_plan(
        &local.shown,
        &model.group_of,
        &slots_for(&local.shown, &local.widths, model),
        &drag.id,
        drag.view_x + offset,
        drag.grab_offset,
        offset,
        offset + local.flick_w,
        cancel_armed(drag.pointer_y),
    )
}

/// The dragged chip is outside the band where a release still moves it.
fn cancel_armed(pointer_y: f32) -> bool {
    !(-CANCEL_BAND..=STRIP_H + CANCEL_BAND).contains(&pointer_y)
}

/// Scroll speed, in pixels a second, for a chip dragged to `view_x` in a
/// viewport `view_w` wide: none in the middle, rising to the maximum over the
/// last `EDGE_ZONE` pixels of either side.
fn edge_velocity(view_x: f32, view_w: f32) -> f32 {
    let far = view_w - EDGE_ZONE;
    if view_x < EDGE_ZONE {
        -MAX_SCROLL_SPEED * ((EDGE_ZONE - view_x) / EDGE_ZONE).clamp(0.0, 1.0).powi(2)
    } else if view_x > far {
        MAX_SCROLL_SPEED * ((view_x - far) / EDGE_ZONE).clamp(0.0, 1.0).powi(2)
    } else {
        0.0
    }
}

/// Scrolls the strip while a dragged chip is held near an edge, a step each
/// frame, so it keeps going under a pointer that is not moving (the QML's
/// FrameAnimation). It stops by itself when the pointer leaves the edge, the
/// drag ends or is about to be cancelled, or the chip's own workspace group
/// is fully in view: a chip cannot leave its group, so there is nothing to
/// scroll further for. Only this function clears `edge_running`.
fn edge_tick(
    local: Rc<RefCell<DockLocal>>,
    scroller: NodeRef,
    frame: Signal<u64>,
    last: Option<Instant>,
) {
    request_frame(move |now| {
        if !frame.is_alive() {
            return;
        }
        // The first frame only starts the clock. A frame time can also
        // repeat or precede the last one, so no progress is not an ending.
        let dt = last.map_or(0.0, |last| {
            now.saturating_duration_since(last).as_secs_f32().min(0.05)
        });
        let target = {
            let mut local = local.borrow_mut();
            let velocity = match &local.drag {
                Some(drag) if !cancel_armed(drag.pointer_y) => {
                    edge_velocity(drag.view_x, local.flick_w)
                }
                _ => 0.0,
            };
            let actual = scroller.scroll_offset().x;
            let from = local
                .edge_pos
                .filter(|pos| (pos - actual).abs() < 1.0)
                .unwrap_or(actual);
            let limit = (local.total_w - local.flick_w).max(0.0);
            let plan = local.drag.as_ref().and_then(|drag| drag.plan);
            let bound = if velocity > 0.0 {
                plan.map_or(limit, |plan| from.max(plan.group_end - local.flick_w))
                    .min(limit)
            } else {
                plan.map_or(0.0, |plan| from.min(plan.group_lo)).max(0.0)
            };
            let done = velocity == 0.0
                || (velocity > 0.0 && from >= bound - 0.5)
                || (velocity < 0.0 && from <= bound + 0.5);
            if done {
                local.edge_running = false;
                local.edge_pos = None;
                return;
            }
            let to = if velocity > 0.0 {
                (from + velocity * dt).min(bound)
            } else {
                (from + velocity * dt).max(bound)
            };
            local.edge_pos = Some(to);
            to
        };
        // The scroller reports the new offset before the next frame, and
        // that remakes the drag's plan for it.
        scroller.scroll_to(target, 0.0);
        edge_tick(local, scroller, frame, Some(now));
    });
}

#[allow(clippy::too_many_lines)]
pub fn dock_body(state: AppState, output: Output) -> Div {
    let monitor = output.name.clone();
    let local: Rc<RefCell<DockLocal>> = Rc::new(RefCell::new(DockLocal {
        was_duo: settings::duo_mode(&state.settings.get_untracked(), &outputs()),
        ..DockLocal::default()
    }));
    let frame: Signal<u64> = signal(0);
    let scroller = node_ref();
    let duo = settings::duo_mode(&state.settings.get_untracked(), &outputs());
    let output_w = output.logical_size.width as f32;

    // Geometry the layout follows. These are signals of their own because
    // `frame` rebuilds every chip, which a scroll step must not do.
    // The chip row's width (QML chipRow.width).
    let row_w: Signal<f32> = signal(0.0);
    // The viewport the row scrolls in.
    let view_w: Signal<f32> = signal(0.0);
    // The Duo strip's own width, once it has been laid out.
    let body_w: Signal<f32> = signal(0.0);
    // The scroll offset (QML chipFlick.contentX).
    let scrolled: Signal<f32> = signal(0.0);
    // The row is wider than its track, so the arrows show and it scrolls.
    // Measured against the body, as the QML did against its track: the
    // viewport's own width depends on the answer, and comparing with that
    // made the arrows linger until the row was 52 px narrower again.
    let overflowing = memo(move || {
        if duo {
            let body = body_w.get();
            body > 0.0 && row_w.get() > body - 16.0
        } else {
            row_w.get() + 16.0 > output_w - 40.0
        }
    });

    // Held-order release, reorder results, drag/menu invalidation.
    {
        let state = state.clone();
        let local = Rc::clone(&local);
        let monitor = monitor.clone();
        effect(move || {
            let _view = state.view.get();
            let seq = state.reorder_seq.get();
            let duo = settings::duo_mode(&state.settings.get_untracked(), &outputs());
            let model = dock_model(&state, &monitor);
            let mut bump = false;
            {
                let mut local = local.borrow_mut();
                if duo != local.was_duo {
                    local.was_duo = duo;
                    local.drag = None;
                    local.press = None;
                    bump = true;
                }
                if let Some(drag) = &local.drag
                    && !model.by_id.contains_key(&drag.id)
                {
                    local.drag = None;
                    local.press = None;
                    bump = true;
                }
                if let Some(menu_id) = &local.menu_win
                    && !model.by_id.contains_key(menu_id)
                {
                    local.menu_win = None;
                    bump = true;
                }
                if let Some(held) = &local.held
                    && (move_satisfied(&model.ids, &model.group_of, &held.mv)
                        || !same_members(&held.ids, &held.groups, &model.ids, &model.group_of))
                {
                    local.held = None;
                    bump = true;
                }
            }
            if seq != local.borrow().last_reorder_seq {
                local.borrow_mut().last_reorder_seq = seq;
                if let Some(outcome) = state.last_reorder() {
                    let pending_id = local.borrow().held.as_ref().map(|h| h.mv.id.clone());
                    if pending_id.as_deref() == Some(outcome.window_id.as_str()) {
                        if outcome.ok {
                            let generation = local.borrow().pending_gen;
                            schedule_release(Rc::clone(&local), frame, generation, 400);
                        } else {
                            if let Some(err) = &outcome.error
                                && err.starts_with("unknown method")
                                && !state.reorder_hint_shown.get()
                            {
                                state.reorder_hint_shown.set(true);
                                let _ = std::process::Command::new("notify-send")
                                    .args([
                                        "--app-name=MinkaShell",
                                        "Dock order needs a newer ShojiWM",
                                        "Install the current build and start a new session to get windows.reorder.",
                                    ])
                                    .stdin(std::process::Stdio::null())
                                    .stdout(std::process::Stdio::null())
                                    .stderr(std::process::Stdio::null())
                                    .spawn();
                            }
                            release_held(&local, frame);
                        }
                    }
                }
            }
            if bump {
                frame.update(|n| *n += 1);
            }
        });
    }

    let strip_state = state.clone();
    let strip_local = Rc::clone(&local);
    let strip_monitor = monitor.clone();
    // The chip row. A scroller takes its extent from its direct children
    // only, and a dyn_view is a fragment, so `inner` below, which carries the
    // row's width, counts as the scroller's child. (Wrapped in a box as wide
    // as the viewport, the row had nothing to scroll.)
    let chips = dyn_view(move || {
        let _ = frame.get();
        let model = dock_model(&strip_state, &strip_monitor);
        let offset = scroller.scroll_offset().x;
        let (shown, slots, shifts, plan, drag_id, cancel_armed, pending_id, total) = {
            let mut local = strip_local.borrow_mut();
            let shown = match &local.held {
                Some(held)
                    if same_members(&held.ids, &held.groups, &model.ids, &model.group_of)
                        && !move_satisfied(&model.ids, &model.group_of, &held.mv) =>
                {
                    held.ids.clone()
                }
                _ => model.ids.clone(),
            };
            let slots = slots_for(&shown, &local.widths, &model);
            let total = slots.last().map_or(0.0, |s| s.x + s.w);
            // A drag's plan holds indices into the list it was made
            // from, and this render also runs when the window list
            // changes with the pointer held still. Remake the plan for
            // the list being drawn, as the QML's viewChanged() did
            // through refreshShifts(), and keep it for the release. It
            // also runs when the row scrolls under a held chip, which
            // changes where along the row the pointer is.
            let (view_lo, view_hi) = (offset, offset + local.flick_w);
            if let Some(drag) = &mut local.drag {
                drag.plan = live_plan(
                    &shown,
                    &model.group_of,
                    &slots,
                    &drag.id,
                    drag.view_x + offset,
                    drag.grab_offset,
                    view_lo,
                    view_hi,
                    cancel_armed(drag.pointer_y),
                );
            }
            let plan = match (&local.drag, &local.held) {
                (Some(drag), _) => drag.plan,
                (None, Some(held)) => {
                    pending_plan(
                        &model.ids,
                        &model.group_of,
                        &slots_for(&model.ids, &local.widths, &model),
                        &held.mv,
                    )
                    .and_then(|p| {
                        // Pending plan is in live-order slots; chips
                        // are drawn in held order, where it is home.
                        let _ = p;
                        None
                    })
                }
                (None, None) => None,
            };
            let shifts = shifts_for(&shown, &slots, plan.as_ref());
            let drag_id = local.drag.as_ref().map(|d| d.id.clone());
            let cancel = local
                .drag
                .as_ref()
                .is_some_and(|d| cancel_armed(d.pointer_y));
            let pending_id = local.held.as_ref().map(|h| h.mv.id.clone());
            local.slots = slots.clone();
            local.shown = shown.clone();
            local.total_w = total;
            (
                shown, slots, shifts, plan, drag_id, cancel, pending_id, total,
            )
        };
        // After the borrow: this re-runs the layout that follows the row.
        row_w.set_if_changed(total);
        let drag_group = drag_id
            .as_ref()
            .and_then(|id| model.group_of.get(id))
            .cloned();
        let mut inner = div().relative().h(STRIP_H).dyn_style(move |s| s.w(total));
        if let Some(plan) = &plan {
            inner = inner.child(
                div()
                    .absolute()
                    .left(plan.landing_x)
                    .top(4.0)
                    .w(plan.width)
                    .h(CHIP_H)
                    .rounded(8.0_f32)
                    .border(1.0_f32)
                    .border_color(color(RED_DIM)),
            );
        }
        for (i, id) in shown.iter().enumerate() {
            let Some(win) = model.by_id.get(id) else {
                continue;
            };
            let slot = slots[i];
            let shift = shifts.get(id).copied().unwrap_or(0.0);
            let group_start = i > 0 && model.group_of.get(id) != model.group_of.get(&shown[i - 1]);
            if group_start && drag_id.is_none() && pending_id.is_none() {
                inner = inner.child(
                    div()
                        .absolute()
                        .left(slot.x + shift - 4.0)
                        .top(12.0)
                        .w(1.0)
                        .h(16.0)
                        .bg(color(LINE)),
                );
            }
            let lifted = drag_id.as_deref() == Some(id.as_str());
            let dimmed = drag_group.is_some() && model.group_of.get(id) != drag_group.as_ref();
            inner = inner.child(chip_view(
                Rc::clone(&strip_local),
                frame,
                id.clone(),
                win.clone(),
                slot.x + shift,
                ChipStyle {
                    lifted,
                    cancel_armed,
                    dimmed,
                },
            ));
        }
        inner
    });

    // Strip-wide pointer handling (QML's stripArea). Handlers live on a
    // transparent overlay above the chips so chips can rebuild freely.
    //
    // The overlay lies over the viewport, beside the scroller and not inside
    // it. Inside, every press would also be the scroller's: the toolkit
    // takes the bottom 12 px of an overflowing scroller as its scrollbar,
    // which is the lower quarter of every chip, and scrolls it by itself
    // while a press is dragged past its edge. The price is that the toolkit
    // no longer scrolls the strip for the wheel either: `on_scroll` below
    // does.
    let input_state = state.clone();
    let input_local = Rc::clone(&local);
    let overlay = div()
        .absolute()
        .left(0.0)
        .right(0.0)
        .top(0.0)
        .bottom(0.0)
        .on_pointer_down({
            let local = Rc::clone(&input_local);
            move |e: &PointerEvent| {
                // Another button mid-drag cancels it.
                if cancel_drag(&local, frame) {
                    return;
                }
                let mut local = local.borrow_mut();
                let (_, _, x) = strip_point(e, scroller);
                let id = hit_chip(&local, x);
                local.press = Some(Press {
                    id,
                    button: e.button,
                    window_x: window_x(e),
                    content_x: x,
                    row_w: local.total_w,
                });
                local.suppress_click = false;
            }
        })
        // The backstop for a press whose release never arrives here: a
        // second button pressed off the dock takes the press away without a
        // word, and the drag would stay lifted and follow the bare pointer
        // the next time it crossed. This is told when the buttons come up,
        // wherever that is, so a drag or press still here then has lost its
        // release. (A release that does arrive may still be another
        // press's: `on_pointer_up` tells those apart.)
        .on_active({
            let local = Rc::clone(&input_local);
            move |active| {
                if !active && !cancel_drag(&local, frame) {
                    local.borrow_mut().press = None;
                }
            }
        })
        .on_pointer_move({
            let local = Rc::clone(&input_local);
            let state = input_state.clone();
            let monitor = monitor.clone();
            move |e: &PointerEvent| {
                let mut bump = false;
                let mut start_edge = false;
                {
                    let mut local = local.borrow_mut();
                    let (view_x, view_y, _) = strip_point(e, scroller);
                    if local.drag.is_none()
                        && let Some(press) = &local.press
                        && press.button == Some(PointerButton::Primary)
                        && press.id.is_some()
                        && (window_x(e) - press.window_x).abs() >= DRAG_START
                        && (local.total_w - press.row_w).abs() < 0.5
                        && state.ipc_ready.get_untracked()
                        && local.held.is_none()
                    {
                        let id = press.id.clone().unwrap_or_default();
                        let slot_x = local
                            .shown
                            .iter()
                            .position(|s| s == &id)
                            .and_then(|i| local.slots.get(i).map(|s| s.x))
                            .unwrap_or(0.0);
                        local.drag = Some(Drag {
                            id,
                            grab_offset: press.content_x - slot_x,
                            view_x,
                            pointer_y: view_y,
                            window: (e.position.x, e.position.y),
                            plan: None,
                        });
                    }
                    if let Some(mut drag) = local.drag.take() {
                        drag.view_x = view_x;
                        drag.pointer_y = view_y;
                        drag.window = (e.position.x, e.position.y);
                        let model = dock_model(&state, &monitor);
                        drag.plan = replan(&local, &model, &drag, scroller);
                        // Held near an edge of an overflowing row, the
                        // strip scrolls under the chip, from a frame
                        // tick of its own.
                        if !local.edge_running
                            && local.total_w > local.flick_w
                            && !cancel_armed(view_y)
                            && edge_velocity(view_x, local.flick_w) != 0.0
                        {
                            local.edge_running = true;
                            start_edge = true;
                        }
                        local.drag = Some(drag);
                        bump = true;
                    }
                }
                if start_edge {
                    edge_tick(Rc::clone(&local), scroller, frame, None);
                }
                if bump {
                    frame.update(|n| *n += 1);
                }
            }
        })
        .on_pointer_up({
            let local = Rc::clone(&input_local);
            let state = input_state.clone();
            let monitor = monitor.clone();
            move |e: &PointerEvent| {
                let mut commit: Option<(String, Option<String>)> = None;
                let mut click: Option<(String, PointerButton, f32)> = None;
                let mut bump = false;
                {
                    let mut local = local.borrow_mut();
                    let (view_x, view_y, x) = strip_point(e, scroller);
                    if let Some(mut drag) = local.drag.take() {
                        local.press = None;
                        local.suppress_click = true;
                        // Plan against where the button came up, as the
                        // QML's finishDrag did: the row can have moved
                        // under the pointer since its last motion.
                        drag.view_x = view_x;
                        drag.pointer_y = view_y;
                        let model = dock_model(&state, &monitor);
                        drag.plan = replan(&local, &model, &drag, scroller);
                        // Is this the drag's own release? After a second
                        // button pressed off the dock, the first button up
                        // over the strip is handed to this handler too,
                        // whichever button it is, and that press was meant
                        // to cancel. The drag's own release is the primary
                        // button, at the point of the last motion.
                        let own = e.button == Some(PointerButton::Primary)
                            && (e.position.x - drag.window.0).abs() <= 1.0
                            && (e.position.y - drag.window.1).abs() <= 1.0;
                        // Only a plan for the list on screen is carried
                        // out: its `from` must be the dragged window.
                        if own
                            && !cancel_armed(view_y)
                            && let Some(plan) = drag.plan
                            && plan.to != plan.from
                            && local.shown.get(plan.from) == Some(&drag.id)
                            && plan.to < local.shown.len()
                        {
                            let before = before_id_for(
                                &local.shown.clone(),
                                &model.group_of,
                                plan.from,
                                plan.to,
                            );
                            let mut held_ids = local.shown.clone();
                            let moved = held_ids.remove(plan.from);
                            held_ids.insert(plan.to, moved);
                            let held_groups: Vec<String> = held_ids
                                .iter()
                                .map(|id| model.group_of.get(id).cloned().unwrap_or_default())
                                .collect();
                            local.held = Some(Held {
                                ids: held_ids,
                                groups: held_groups,
                                mv: Move {
                                    id: drag.id.clone(),
                                    before_id: before.clone(),
                                },
                            });
                            local.pending_gen += 1;
                            commit = Some((drag.id.clone(), before));
                        }
                        bump = true;
                    } else if let Some(press) = local.press.take() {
                        if !local.suppress_click
                            && let (Some(press_id), Some(button)) = (press.id.clone(), press.button)
                            // The button that was pressed, let go on the
                            // strip, over the chip pressed: a
                            // pointer pulled away first means not to.
                            && e.button == Some(button)
                            && (0.0..=STRIP_H).contains(&view_y)
                            && (0.0..=local.flick_w).contains(&view_x)
                            && hit_chip(&local, x).as_deref() == Some(press_id.as_str())
                        {
                            // Anchor the menu at the chip's centre. A
                            // popup hangs from a rectangle in window
                            // pixels, and `e.rect` is where the overlay,
                            // so the viewport wherever the body put it,
                            // is in the window.
                            let centre = local
                                .shown
                                .iter()
                                .position(|s| s == &press_id)
                                .and_then(|i| local.slots.get(i).map(|s| s.x + s.w / 2.0))
                                .unwrap_or(x)
                                - scroller.scroll_offset().x;
                            let scale = untrack(window_scale).as_f32();
                            click = Some((press_id, button, e.rect.x as f32 + centre * scale));
                        }
                        local.suppress_click = false;
                    }
                }
                if let Some((id, before)) = commit {
                    state.ipc.reorder_window(&id, before.as_deref());
                    let generation = local.borrow().pending_gen;
                    schedule_release(Rc::clone(&local), frame, generation, 2000);
                }
                if let Some((id, button, anchor)) = click {
                    match button {
                        PointerButton::Middle => state.ipc.close_window(&id),
                        PointerButton::Secondary => {
                            let mut local = local.borrow_mut();
                            if local.menu_win.as_deref() == Some(id.as_str()) {
                                local.menu_win = None;
                            } else {
                                local.menu_win = Some(id);
                                local.menu_anchor_x = anchor;
                            }
                            bump = true;
                        }
                        PointerButton::Primary => state.ipc.activate_window(&id),
                        // A mouse's side buttons do nothing here.
                        _ => {}
                    }
                }
                if bump {
                    frame.update(|n| *n += 1);
                }
            }
        })
        .on_scroll({
            let local = Rc::clone(&input_local);
            move |e: &ScrollEvent| {
                // A wheel turns the strip whichever way it is mounted: the
                // strip only goes one way. (Under the QML a plain vertical
                // wheel did nothing here.) Fingers have a sideways axis of
                // their own, and only that one moves it: taking the larger
                // axis of each event let a slanted swipe pull both ways.
                let sideways = matches!(e.source, ScrollSource::Touchpad)
                    || e.delta_x.abs() >= e.delta_y.abs();
                let delta = if sideways { e.delta_x } else { e.delta_y };
                // The end of a gesture carries no movement.
                if delta == 0.0 {
                    return;
                }
                let (limit, rest) = {
                    let local = local.borrow();
                    ((local.total_w - local.flick_w).max(0.0), local.wheel_rest)
                };
                let before = scroller.scroll_offset().x;
                let want = before + rest + delta / untrack(window_scale).as_f32();
                scroller.scroll_to(want, 0.0);
                let after = scroller.scroll_offset().x;
                // Offsets are whole pixels. What a step lost goes to the
                // next one, unless it was lost against an end.
                let inside = want > 0.0 && want < limit;
                local.borrow_mut().wheel_rest = if inside { want - after } else { 0.0 };
                // Left unclaimed at an end, the event also tells the
                // toolkit to stop its inertia there.
                if after != before || inside {
                    e.stop_propagation();
                }
            }
        });

    // The scroller, holding only the row.
    let flick = div()
        .w_full()
        .h(STRIP_H)
        .overflow_x_scroll()
        // The toolkit's scrollbar would be drawn across the chips' lower
        // edge. The arrows say where the row can go.
        .scrollbar_color(Color::TRANSPARENT)
        .node_ref(scroller)
        .on_scrolled({
            let local = Rc::clone(&local);
            move |at| {
                scrolled.set_if_changed(at.x);
                // The row moved under a held chip: plan it again (the QML's
                // onContentXChanged).
                let dragging = local.borrow().drag.is_some();
                if dragging {
                    frame.update(|n| *n += 1);
                }
            }
        })
        .child(chips);

    // The viewport: as wide as the row while that fits, so the body can
    // centre it, and squeezed to the track by the body's flex row once it
    // does not. `min_w(0)` is what allows the squeeze below its content.
    let viewport = div()
        .relative()
        .h(STRIP_H)
        .min_w(0.0)
        .dyn_style(move |s| s.w(row_w.get()))
        .on_resized({
            let local = Rc::clone(&local);
            move |w, _h| {
                local.borrow_mut().flick_w = w;
                view_w.set_if_changed(w);
            }
        })
        .child(flick)
        // After the scroller, so that it is what a press lands on.
        .child(overlay);

    let arrow = move |dir: i32| -> Div {
        div()
            .w(22.0)
            .h(32.0)
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .cursor(CursorIcon::Pointer)
            .dyn_style(move |s| {
                let at = scrolled.get();
                let can = if dir < 0 {
                    at > 0.5
                } else {
                    at < row_w.get() - view_w.get() - 0.5
                };
                s.opacity(if can { 1.0 } else { 0.3_f32 })
            })
            .on_click(move |_| {
                // Most of a viewport at a time; scroll_to stops at the ends.
                let step = dir as f32 * view_w.get_untracked() * 0.8;
                scroller.scroll_to(scroller.scroll_offset().x + step, 0.0);
            })
            .child(ui_text(
                if dir < 0 { "‹" } else { "›" },
                FONT_SIZE + 4.0,
                TEXT_MUTED,
            ))
    };

    let body = div()
        .h(DOCK_HEIGHT)
        .flex()
        .flex_row()
        .items_center()
        // Centres the viewport while the row fits. An overflowing row fills
        // the track between its arrows instead.
        .justify_center()
        .gap(4.0_f32)
        .on_resized(move |w, _h| {
            if duo {
                body_w.set_if_changed(w);
            }
        })
        .dyn_style(move |s| {
            let s = s.bg(bar_bg());
            if duo {
                s.w_full().px(8.0_f32)
            } else {
                // A pill as wide as its chips, up to the screen.
                let s = s
                    .w((row_w.get() + 16.0).min(output_w - 40.0).max(48.0))
                    .rounded(10.0_f32)
                    .border(1.0_f32)
                    .border_color(color(LINE));
                // The arrows stand 8 px in from the pill's edge, border
                // included. Only then: a row that fits is centred, and
                // padding would squeeze it at scales where 7 px rounds up.
                if overflowing.get() { s.px(7.0_f32) } else { s }
            }
        })
        // An absent arrow takes no room, gap included, so the row is centred
        // on the strip and not on what is left beside two placeholders.
        .child(dyn_view(move || {
            if overflowing.get() {
                arrow(-1)
            } else {
                div().hidden()
            }
        }))
        .child(viewport)
        .child(dyn_view(move || {
            if overflowing.get() {
                arrow(1)
            } else {
                div().hidden()
            }
        }));

    let root = div()
        .relative()
        .w_full()
        .h_full()
        // Text is selectable by default, and a press on the strip would
        // start selecting chip titles (and put them on the clipboard).
        .select_none()
        // A second button cancels a drag wherever on the dock it lands: on
        // an arrow or the padding as well as on the strip.
        .on_pointer_down({
            let local = Rc::clone(&local);
            move |_| {
                cancel_drag(&local, frame);
            }
        })
        .flex()
        .flex_col()
        .justify_end()
        .items_center()
        .child(body);
    let root = if duo {
        root.child(
            div()
                .absolute()
                .left(0.0)
                .right(0.0)
                .bottom(0.0)
                .h(1.0)
                .bg(color(LINE)),
        )
    } else {
        root
    };

    // DockMenu: a view popup anchored at the chip centre.
    let menu_state = state.clone();
    let menu_local = Rc::clone(&local);
    root.child(dyn_view(move || {
        let _ = frame.get();
        let (menu_id, anchor_x) = {
            let local = menu_local.borrow();
            (local.menu_win.clone(), local.menu_anchor_x)
        };
        let Some(menu_id) = menu_id else {
            return div();
        };
        let model = dock_model(&menu_state, &monitor);
        let Some(win) = model.by_id.get(&menu_id) else {
            return div();
        };
        let title = win.label().to_owned();
        let close_state = menu_state.clone();
        let close_id = menu_id.clone();
        let dismiss_local = Rc::clone(&menu_local);
        let card = div()
            .w(200.0)
            .rounded(8.0_f32)
            .bg(bar_bg())
            .border(1.0_f32)
            .border_color(color(LINE))
            .flex()
            .flex_col()
            .p(8.0_f32)
            .gap(6.0_f32)
            .child(div().max_w(184.0).overflow_hidden().child(ui_text(
                elide(&title, 184.0, FONT_SIZE - 1.0),
                FONT_SIZE - 1.0,
                TEXT_MUTED,
            )))
            .child(div().h(1.0).w_full().bg(color(LINE)))
            .child(
                div()
                    .h(24.0)
                    .rounded(5.0_f32)
                    .flex()
                    .items_center()
                    .px(8.0_f32)
                    .cursor(CursorIcon::Pointer)
                    .on_click(move |_| {
                        close_state.ipc.close_window(&close_id);
                        dismiss_local.borrow_mut().menu_win = None;
                        frame.update(|n| *n += 1);
                    })
                    .child(ui_text("close window", FONT_SIZE - 1.0, RED)),
            );
        let dismiss_local = Rc::clone(&menu_local);
        div().child(
            popup()
                .kind(PopupKind::Menu)
                .placement(if duo {
                    Placement::BelowCenter
                } else {
                    Placement::AboveCenter
                })
                .anchor_rect(RectI::new(
                    anchor_x as i32,
                    0,
                    1,
                    untrack(window_scale).px(DOCK_HEIGHT),
                ))
                .gap(6)
                .on_dismiss(move || {
                    dismiss_local.borrow_mut().menu_win = None;
                    frame.update(|n| *n += 1);
                })
                .child(card),
        )
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() -> (Vec<String>, HashMap<String, String>, Vec<Slot>) {
        let ids: Vec<String> = ["a", "b", "c", "d"].iter().map(|s| s.to_string()).collect();
        let mut group_of = HashMap::new();
        for id in &ids {
            group_of.insert(id.clone(), "m:1".to_owned());
        }
        group_of.insert("d".to_owned(), "m:2".to_owned());
        let slots = vec![
            Slot { x: 0.0, w: 100.0 },
            Slot { x: 106.0, w: 80.0 },
            Slot { x: 192.0, w: 100.0 },
            Slot { x: 298.0, w: 90.0 },
        ];
        (ids, group_of, slots)
    }

    #[test]
    fn live_plan_clamps_to_group() {
        let (ids, groups, slots) = setup();
        // Drag "a" far right: stops at end of its group (index 2).
        let plan = live_plan(&ids, &groups, &slots, "a", 500.0, 10.0, 0.0, 1000.0, false).unwrap();
        assert_eq!(plan.to, 2);
        assert_eq!(before_id_for(&ids, &groups, plan.from, plan.to), None);
        // Cancelled drag stays home.
        let plan = live_plan(&ids, &groups, &slots, "a", 500.0, 10.0, 0.0, 1000.0, true).unwrap();
        assert_eq!(plan.to, 0);
    }

    #[test]
    fn move_satisfied_cases() {
        let (ids, groups, _) = setup();
        let mv = Move {
            id: "a".into(),
            before_id: Some("c".into()),
        };
        assert!(!move_satisfied(&ids, &groups, &mv));
        let moved: Vec<String> = ["b", "a", "c", "d"].iter().map(|s| s.to_string()).collect();
        assert!(move_satisfied(&moved, &groups, &mv));
        let mv_end = Move {
            id: "a".into(),
            before_id: None,
        };
        let moved_end: Vec<String> = ["b", "c", "a", "d"].iter().map(|s| s.to_string()).collect();
        assert!(move_satisfied(&moved_end, &groups, &mv_end));
    }

    /// A window closing mid-drag shortens the list under a plan made from the
    /// longer one. That used to index past the end and end the shell.
    #[test]
    fn stale_plan_does_not_panic() {
        let (ids, groups, slots) = setup();
        // Drag "c" (index 2) back to index 0, then lose "a" and "b".
        let plan = live_plan(&ids, &groups, &slots, "c", 0.0, 10.0, 0.0, 1000.0, false).unwrap();
        assert_eq!((plan.from, plan.to), (2, 0));
        let short_ids = ids[2..].to_vec();
        let short_slots = slots[..2].to_vec();
        assert!(shifts_for(&short_ids, &short_slots, Some(&plan)).is_empty());
        assert_eq!(before_id_for(&short_ids, &groups, plan.from, plan.to), None);
        // Slots from the old list give no plan for the new one.
        assert!(
            live_plan(
                &short_ids, &groups, &slots, "c", 0.0, 10.0, 0.0, 1000.0, false
            )
            .is_none()
        );
        // The plan remade for the short list is in range for it.
        let remade = live_plan(
            &short_ids,
            &groups,
            &short_slots,
            "c",
            0.0,
            10.0,
            0.0,
            1000.0,
            false,
        )
        .unwrap();
        assert_eq!((remade.from, remade.to), (0, 0));
        assert!(!shifts_for(&short_ids, &short_slots, Some(&remade)).is_empty());
    }

    #[test]
    fn edge_scroll_speed() {
        // Nothing in the middle of a 1000 px viewport.
        assert_eq!(edge_velocity(500.0, 1000.0), 0.0);
        assert_eq!(edge_velocity(EDGE_ZONE, 1000.0), 0.0);
        assert_eq!(edge_velocity(1000.0 - EDGE_ZONE, 1000.0), 0.0);
        // Towards the start on the left, towards the end on the right, and
        // quadratic: half way into the zone is a quarter of the speed.
        assert_eq!(
            edge_velocity(EDGE_ZONE / 2.0, 1000.0),
            -MAX_SCROLL_SPEED / 4.0
        );
        assert_eq!(
            edge_velocity(1000.0 - EDGE_ZONE / 2.0, 1000.0),
            MAX_SCROLL_SPEED / 4.0
        );
        // Full speed at the edge and beyond it: the pointer is captured, so
        // it can be outside the strip.
        assert_eq!(edge_velocity(0.0, 1000.0), -MAX_SCROLL_SPEED);
        assert_eq!(edge_velocity(-300.0, 1000.0), -MAX_SCROLL_SPEED);
        assert_eq!(edge_velocity(1300.0, 1000.0), MAX_SCROLL_SPEED);
    }

    #[test]
    fn cancel_band() {
        assert!(!cancel_armed(0.0));
        assert!(!cancel_armed(STRIP_H));
        assert!(!cancel_armed(-CANCEL_BAND));
        assert!(!cancel_armed(STRIP_H + CANCEL_BAND));
        assert!(cancel_armed(-CANCEL_BAND - 1.0));
        assert!(cancel_armed(STRIP_H + CANCEL_BAND + 1.0));
    }

    /// Ids the group map has never heard of are tolerated, not indexed.
    #[test]
    fn unknown_ids_do_not_panic() {
        let (ids, _, slots) = setup();
        let empty = HashMap::new();
        assert!(live_plan(&ids, &empty, &slots, "a", 500.0, 10.0, 0.0, 1000.0, false).is_some());
        let mv = Move {
            id: "a".into(),
            before_id: Some("c".into()),
        };
        assert!(!move_satisfied(&ids, &empty, &mv));
        assert!(pending_plan(&ids, &empty, &slots, &mv).is_some());
    }
}

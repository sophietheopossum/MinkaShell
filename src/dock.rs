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
use std::time::Duration;

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

fn group_span(ids: &[String], group_of: &HashMap<String, String>, i: usize) -> (usize, usize) {
    let key = &group_of[&ids[i]];
    let mut start = i;
    let mut end = i;
    while start > 0 && group_of[&ids[start - 1]] == *key {
        start -= 1;
    }
    while end < ids.len() - 1 && group_of[&ids[end + 1]] == *key {
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

pub fn before_id_for(
    ids: &[String],
    group_of: &HashMap<String, String>,
    from: usize,
    to: usize,
) -> Option<String> {
    if to < from {
        return Some(ids[to].clone());
    }
    let (_, end) = group_span(ids, group_of, from);
    if to < end {
        Some(ids[to + 1].clone())
    } else {
        None
    }
}

pub fn move_satisfied(ids: &[String], group_of: &HashMap<String, String>, mv: &Move) -> bool {
    let Some(i) = ids.iter().position(|x| x == &mv.id) else {
        return true;
    };
    match &mv.before_id {
        None => i == ids.len() - 1 || group_of[&ids[i + 1]] != group_of[&mv.id],
        Some(before) => {
            let Some(b) = ids.iter().position(|x| x == before) else {
                return true;
            };
            if group_of[before] != group_of[&mv.id] {
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
    pointer_x: f32,
    pointer_y: f32,
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
    view_x: f32,
    content_x: f32,
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
    flick_w: f32,
    scroll_x: f32,
    total_w: f32,
    menu_win: Option<String>,
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
    let chip = chip.child(div().max_w(150.0).overflow_hidden().child(ui_text(
        elide(win.label(), 150.0, FONT_SIZE - 1.0),
        FONT_SIZE - 1.0,
        if lifted || focused { TEXT } else { TEXT_MUTED },
    )));
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

#[allow(clippy::too_many_lines)]
pub fn dock_body(state: AppState, output: Output) -> Div {
    let monitor = output.name.clone();
    let local: Rc<RefCell<DockLocal>> = Rc::new(RefCell::new(DockLocal {
        was_duo: settings::duo_mode(&state.settings.get_untracked(), &outputs()),
        ..DockLocal::default()
    }));
    let frame: Signal<u64> = signal(0);
    let scroller = node_ref();

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

    let duo = settings::duo_mode(&state.settings.get_untracked(), &outputs());
    let output_w = output.logical_size.width as f32;

    let strip_state = state.clone();
    let strip_local = Rc::clone(&local);
    let strip_monitor = monitor.clone();
    let strip = div()
        .relative()
        .h(STRIP_H)
        .w_full()
        .child(dyn_view(move || {
            let _ = frame.get();
            let model = dock_model(&strip_state, &strip_monitor);
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
                let cancel = local.drag.as_ref().is_some_and(|d| {
                    d.pointer_y < -CANCEL_BAND || d.pointer_y > STRIP_H + CANCEL_BAND
                });
                let pending_id = local.held.as_ref().map(|h| h.mv.id.clone());
                local.slots = slots.clone();
                local.shown = shown.clone();
                local.total_w = total;
                (
                    shown, slots, shifts, plan, drag_id, cancel, pending_id, total,
                )
            };
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
                let group_start =
                    i > 0 && model.group_of.get(id) != model.group_of.get(&shown[i - 1]);
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
        }));

    // Strip-wide pointer handling (QML's stripArea). Handlers live on a
    // transparent overlay above the chips so chips can rebuild freely.
    let input_state = state.clone();
    let input_local = Rc::clone(&local);
    let scroller_move = scroller;
    let strip = strip.child(
        div()
            .absolute()
            .left(0.0)
            .right(0.0)
            .top(0.0)
            .bottom(0.0)
            .on_pointer_down({
                let local = Rc::clone(&input_local);
                move |e: &PointerEvent| {
                    let mut local = local.borrow_mut();
                    if local.drag.is_some() {
                        // Another button mid-drag cancels it.
                        local.drag = None;
                        local.press = None;
                        local.suppress_click = true;
                        drop(local);
                        frame.update(|n| *n += 1);
                        return;
                    }
                    let x = e.local.x + local.scroll_x;
                    let id = hit_chip(&local, x);
                    local.press = Some(Press {
                        id,
                        button: e.button,
                        view_x: e.position.x,
                        content_x: x,
                    });
                    local.suppress_click = false;
                }
            })
            .on_pointer_move({
                let local = Rc::clone(&input_local);
                let state = input_state.clone();
                let monitor = monitor.clone();
                move |e: &PointerEvent| {
                    let mut bump = false;
                    let mut scroll_to: Option<f32> = None;
                    {
                        let mut local = local.borrow_mut();
                        let x = e.local.x + local.scroll_x;
                        if local.drag.is_none()
                            && let Some(press) = &local.press
                            && press.button == Some(PointerButton::Primary)
                            && press.id.is_some()
                            && (e.position.x - press.view_x).abs() >= DRAG_START
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
                                pointer_x: x,
                                pointer_y: e.local.y,
                                plan: None,
                            });
                        }
                        if let Some(mut drag) = local.drag.take() {
                            drag.pointer_x = x;
                            drag.pointer_y = e.local.y;
                            let cancelled =
                                e.local.y < -CANCEL_BAND || e.local.y > STRIP_H + CANCEL_BAND;
                            let model = dock_model(&state, &monitor);
                            let shown = local.shown.clone();
                            let slots = slots_for(&shown, &local.widths, &model);
                            drag.plan = live_plan(
                                &shown,
                                &model.group_of,
                                &slots,
                                &drag.id.clone(),
                                x,
                                drag.grab_offset,
                                local.scroll_x,
                                local.scroll_x + local.flick_w,
                                cancelled,
                            );
                            // Edge auto-scroll (step per move event).
                            let view_x = x - local.scroll_x;
                            let velocity = if view_x < EDGE_ZONE {
                                -MAX_SCROLL_SPEED
                                    * ((EDGE_ZONE - view_x) / EDGE_ZONE).clamp(0.0, 1.0).powi(2)
                            } else if view_x > local.flick_w - EDGE_ZONE {
                                MAX_SCROLL_SPEED
                                    * ((view_x - (local.flick_w - EDGE_ZONE)) / EDGE_ZONE)
                                        .clamp(0.0, 1.0)
                                        .powi(2)
                            } else {
                                0.0
                            };
                            if velocity != 0.0 {
                                let max_scroll = (local.total_w - local.flick_w).max(0.0);
                                scroll_to = Some(
                                    (local.scroll_x + velocity * 0.032).clamp(0.0, max_scroll),
                                );
                            }
                            local.drag = Some(drag);
                            bump = true;
                        }
                    }
                    if let Some(target) = scroll_to {
                        scroller_move.scroll_to(target, 0.0);
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
                        let x = e.local.x + local.scroll_x;
                        if let Some(drag) = local.drag.take() {
                            local.press = None;
                            local.suppress_click = true;
                            let cancelled = drag.pointer_y < -CANCEL_BAND
                                || drag.pointer_y > STRIP_H + CANCEL_BAND;
                            if !cancelled
                                && let Some(plan) = drag.plan
                                && plan.to != plan.from
                            {
                                let model = dock_model(&state, &monitor);
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
                                && let (Some(press_id), Some(button)) =
                                    (press.id.clone(), press.button)
                                && hit_chip(&local, x).as_deref() == Some(press_id.as_str())
                            {
                                // Anchor the menu at the chip centre in
                                // surface coordinates.
                                let center = local
                                    .shown
                                    .iter()
                                    .position(|s| s == &press_id)
                                    .and_then(|i| local.slots.get(i).map(|s| s.x + s.w / 2.0))
                                    .unwrap_or(x)
                                    - local.scroll_x;
                                click = Some((press_id, button, center));
                            }
                            local.suppress_click = false;
                        }
                    }
                    if let Some((id, before)) = commit {
                        state.ipc.reorder_window(&id, before.as_deref());
                        let generation = local.borrow().pending_gen;
                        schedule_release(Rc::clone(&local), frame, generation, 2000);
                    }
                    if let Some((id, button, center)) = click {
                        match button {
                            PointerButton::Middle => state.ipc.close_window(&id),
                            PointerButton::Secondary => {
                                let mut local = local.borrow_mut();
                                if local.menu_win.as_deref() == Some(id.as_str()) {
                                    local.menu_win = None;
                                } else {
                                    local.menu_win = Some(id);
                                    local.menu_anchor_x = center;
                                }
                                bump = true;
                            }
                            _ => state.ipc.activate_window(&id),
                        }
                    }
                    if bump {
                        frame.update(|n| *n += 1);
                    }
                }
            }),
    );

    let scroller_ref = scroller;
    let scroll_local = Rc::clone(&local);
    let flick = div()
        .flex_1()
        .min_w(0.0)
        .h(STRIP_H)
        .overflow_x_scroll()
        .node_ref(scroller_ref)
        .on_resized({
            let local = Rc::clone(&local);
            move |w, _h| {
                local.borrow_mut().flick_w = w;
            }
        })
        .on_scrolled(move |at| {
            scroll_local.borrow_mut().scroll_x = at.x;
        })
        .child(strip);

    fn arrow(
        dir: i32,
        local: Rc<RefCell<DockLocal>>,
        frame: Signal<u64>,
        scroller: NodeRef,
    ) -> Div {
        let click_local = Rc::clone(&local);
        div()
            .w(22.0)
            .h(32.0)
            .flex()
            .items_center()
            .justify_center()
            .cursor(CursorIcon::Pointer)
            .dyn_style(move |s| {
                let local = local.borrow();
                let can = if dir < 0 {
                    local.scroll_x > 0.5
                } else {
                    local.scroll_x < local.total_w - local.flick_w - 0.5
                };
                let _ = frame.get();
                s.opacity(if can { 1.0 } else { 0.3_f32 })
            })
            .on_click(move |_| {
                let local = click_local.borrow();
                let max_scroll = (local.total_w - local.flick_w).max(0.0);
                let target =
                    (local.scroll_x + dir as f32 * local.flick_w * 0.8).clamp(0.0, max_scroll);
                scroller.scroll_to(target, 0.0);
            })
            .child(ui_text(
                if dir < 0 { "‹" } else { "›" },
                FONT_SIZE + 4.0,
                TEXT_MUTED,
            ))
    }

    let body_local = Rc::clone(&local);
    let body = div()
        .h(DOCK_HEIGHT)
        .flex()
        .flex_row()
        .items_center()
        .gap(4.0_f32)
        .dyn_style(move |s| {
            let local = body_local.borrow();
            let _ = frame.get();
            let s = s.bg(bar_bg());
            if duo {
                s.w_full()
            } else {
                let overflowing = local.total_w > local.flick_w && local.flick_w > 0.0;
                let arrows = if overflowing { 52.0 } else { 0.0 };
                s.w((local.total_w + 16.0 + arrows)
                    .min(output_w - 40.0)
                    .max(48.0))
                    .rounded(10.0_f32)
                    .border(1.0_f32)
                    .border_color(color(LINE))
            }
        })
        .child(dyn_view({
            let local = Rc::clone(&local);
            move || {
                let _ = frame.get();
                let local_ref = local.borrow();
                if local_ref.flick_w > 0.0 && local_ref.total_w > local_ref.flick_w {
                    drop(local_ref);
                    arrow(-1, Rc::clone(&local), frame, scroller_ref)
                } else {
                    div()
                }
            }
        }))
        .child(flick)
        .child(dyn_view({
            let local = Rc::clone(&local);
            move || {
                let _ = frame.get();
                let local_ref = local.borrow();
                if local_ref.flick_w > 0.0 && local_ref.total_w > local_ref.flick_w {
                    drop(local_ref);
                    arrow(1, Rc::clone(&local), frame, scroller_ref)
                } else {
                    div()
                }
            }
        }));

    let root = div()
        .relative()
        .w_full()
        .h_full()
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
                .anchor_rect(RectI::new(anchor_x as i32, 0, 1, 44))
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
}

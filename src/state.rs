//! Shared shell state: IPC view, menu routing, settings, update counts and
//! sys usage, driven by one 250ms service ticker. Mirrors the QML singletons
//! (`ShojiIpc`, `MenuState`, `ShellLayout`, `SyuState`) plus `SysUsage`'s
//! /proc sampler (read directly here instead of spawning `sh`).

use crate::instance::InstanceGuard;
use crate::ipc::{IpcController, IpcNews, WorkspaceView};
use crate::settings::{self, Settings, SyuState};
use crate::tray_rescue;
use anti_webui::prelude::*;
use anti_webui::shell::prelude::*;
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::{Duration, Instant, SystemTime};

/// AntiWebUI disposes signal owners before running cleanups, so writes from
/// cleanup hooks can hit dead nodes. Swallow that specific panic there.
pub fn cleanup_write(f: impl FnOnce()) {
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(f));
}

pub fn set_if_alive<T: 'static>(sig: Signal<T>, value: T) {
    if sig.is_alive() {
        sig.set(value);
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuKind {
    Start,
    Calendar,
    Status,
    Battery,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ReorderOutcome {
    pub window_id: String,
    pub ok: bool,
    pub changed: bool,
    pub error: Option<String>,
}

#[derive(Clone)]
pub struct AppState {
    pub ipc: Rc<IpcController>,
    pub view: Signal<WorkspaceView>,
    pub ipc_ready: Signal<bool>,
    pub ipc_connected: Signal<bool>,
    /// The window list has arrived once, or has been waited for long enough.
    /// Until then it is not known whether there is a dock to put the Duo
    /// side column under, and the column waits.
    pub windows_known: Signal<bool>,
    pub settings: Signal<Settings>,
    pub syu: Signal<SyuState>,
    /// At most one menu open shell-wide: (screen name, kind).
    pub open_menu: Signal<Option<(String, MenuKind)>>,
    /// (cpu %, mem %) sampled from /proc every 3s.
    pub sys_usage: Signal<(f64, f64)>,
    pub reorder_seq: Signal<u64>,
    /// The "needs a newer ShojiWM" notify-send fires once per shell run.
    pub reorder_hint_shown: Rc<Cell<bool>>,
    notifs: Rc<RefCell<Option<NotificationServer>>>,
    reorder_last: Rc<RefCell<Option<ReorderOutcome>>>,
    tick: Rc<Cell<u64>>,
    settings_mtime: Rc<Cell<Option<SystemTime>>>,
    syu_mtime: Rc<Cell<Option<SystemTime>>>,
    prev_cpu: Rc<Cell<Option<(u64, u64)>>>,
    /// The tray items written down for the next shell (see `tray_rescue`).
    tray_saved: Rc<RefCell<tray_rescue::Saved>>,
    /// Until when, after the start menu closed, the last focused window
    /// stays marked (see `IpcNews::View`).
    focus_hold_until: Rc<Cell<Option<Instant>>>,
    _guard: Rc<InstanceGuard>,
}

impl AppState {
    pub fn new(guard: InstanceGuard) -> Self {
        Self {
            ipc: IpcController::new(),
            view: signal(WorkspaceView::default()),
            ipc_ready: signal(false),
            ipc_connected: signal(false),
            windows_known: signal(false),
            settings: signal(Settings::load()),
            syu: signal(SyuState::load()),
            open_menu: signal(None),
            sys_usage: signal((0.0, 0.0)),
            reorder_seq: signal(0),
            reorder_hint_shown: Rc::new(Cell::new(false)),
            notifs: Rc::new(RefCell::new(None)),
            reorder_last: Rc::new(RefCell::new(None)),
            tick: Rc::new(Cell::new(0)),
            settings_mtime: Rc::new(Cell::new(None)),
            syu_mtime: Rc::new(Cell::new(None)),
            prev_cpu: Rc::new(Cell::new(None)),
            tray_saved: Rc::new(RefCell::new(Vec::new())),
            focus_hold_until: Rc::new(Cell::new(None)),
            _guard: Rc::new(guard),
        }
    }

    // ---- MenuState.qml routing ----

    pub fn is_open(&self, kind: MenuKind, screen: &str) -> bool {
        self.open_menu
            .with(|open| open.as_ref() == Some(&(screen.to_owned(), kind)))
    }

    pub fn dropdown_open(&self) -> bool {
        self.open_menu
            .with(|open| matches!(open, Some((_, kind)) if *kind != MenuKind::Start))
    }

    fn resolve_target(&self, kind: MenuKind, connector: Option<&str>) -> String {
        let all = outputs();
        let settings = self.settings.get_untracked();
        if kind != MenuKind::Start
            && settings::duo_mode(&settings, &all)
            && let Some(pad) = all.iter().find(|o| settings::is_screenpad(o))
        {
            return pad.name.clone();
        }
        if let Some(name) = connector
            && all.iter().any(|o| o.name == name)
        {
            return name.to_owned();
        }
        all.first().map_or_else(String::new, |o| o.name.clone())
    }

    /// `action`: `"open"`, `"toggle"`/None, anything else closes.
    pub fn toggle_menu(&self, kind: MenuKind, connector: Option<&str>, action: Option<&str>) {
        let target = self.resolve_target(kind, connector);
        let was_open = self.is_open(kind, &target);
        let open = match action {
            Some("open") => true,
            None | Some("toggle") => !was_open,
            Some(_) => false,
        };
        self.set_open_menu(open.then_some((target, kind)));
    }

    pub fn close_all_menus(&self) {
        self.set_open_menu(None);
    }

    pub fn start_menu_open(&self) -> bool {
        self.open_menu
            .with_untracked(|open| matches!(open, Some((_, MenuKind::Start))))
    }

    fn set_open_menu(&self, next: Option<(String, MenuKind)>) {
        let was_start = self.start_menu_open();
        self.open_menu.set_if_changed(next);
        if was_start && !self.start_menu_open() {
            // While the start menu was open the view kept its last focused
            // window (see `IpcNews::View` below). An answer the compositor
            // gave before the keyboard is back can still arrive, so the mark
            // is held a moment longer: until a view says who has the keyboard,
            // or a second has gone. Then ask, in case no window got it back.
            self.focus_hold_until
                .set(Some(Instant::now() + Duration::from_secs(1)));
            let ipc = Rc::clone(&self.ipc);
            set_timeout(Duration::from_millis(1100), move || ipc.refresh());
        }
    }

    /// Whether a view with no focused window should keep the last one marked.
    fn holding_focus(&self) -> bool {
        self.start_menu_open()
            || self
                .focus_hold_until
                .get()
                .is_some_and(|until| Instant::now() < until)
    }

    pub fn last_reorder(&self) -> Option<ReorderOutcome> {
        self.reorder_last.borrow().clone()
    }

    // ---- Service ticker ----

    /// The notification daemon handle, once claimed in `start_services`.
    /// `None` while another daemon owns the bus name (e.g. the QML shell
    /// during side-by-side testing).
    pub fn notifs(&self) -> Option<NotificationServer> {
        self.notifs.borrow().clone()
    }

    pub fn start_services(&self) {
        match notification_server() {
            Ok(server) => *self.notifs.borrow_mut() = Some(server),
            Err(e) => eprintln!("minkashell: notifications unavailable: {e}"),
        }
        self.ipc.ensure_started();
        // Before the first tick can write the file: what the last shell
        // showed, for the watcher that inherited its name to be told of.
        let saved = tray_rescue::load();
        *self.tray_saved.borrow_mut() = saved.clone();
        tray_rescue::rescue_in_background(saved);
        self.poll_settings(true);
        self.poll_syu(true);
        self.sample_sys_usage();
        schedule(self.clone());
    }

    fn service_tick(&self) {
        let tick = self.tick.get() + 1;
        self.tick.set(tick);

        for news in self.ipc.take_news() {
            match news {
                IpcNews::Connected => {
                    self.ipc_connected.set_if_changed(true);
                    self.ipc.refresh();
                }
                IpcNews::Disconnected => {
                    self.ipc_connected.set_if_changed(false);
                    self.ipc_ready.set_if_changed(false);
                }
                IpcNews::View(mut view) => {
                    // The start menu takes the keyboard, so no window has
                    // it while it is open: the one that had it stays marked.
                    if view.has_focused_window() {
                        self.focus_hold_until.set(None);
                    } else if self.holding_focus() {
                        view.keep_focus_of(&self.view.get_untracked());
                    }
                    self.view.set_if_changed(view);
                    self.ipc_ready.set_if_changed(true);
                    self.windows_known.set_if_changed(true);
                }
                IpcNews::UiStartMenu { connector, action } => {
                    self.toggle_menu(MenuKind::Start, connector.as_deref(), Some(&action));
                }
                IpcNews::ReorderResult {
                    window_id,
                    ok,
                    changed,
                    error,
                } => {
                    *self.reorder_last.borrow_mut() = Some(ReorderOutcome {
                        window_id,
                        ok,
                        changed,
                        error,
                    });
                    self.reorder_seq.update(|n| *n += 1);
                }
            }
        }

        if tick.is_multiple_of(8) {
            self.ipc.probe_if_idle();
            // Two seconds without a window list: the compositor's socket is
            // not answering, and the shell does without knowing.
            self.windows_known.set_if_changed(true);
        }
        if tick.is_multiple_of(4) {
            self.poll_settings(false);
            self.poll_syu(false);
            self.save_tray_items();
        }
        if tick.is_multiple_of(12) {
            self.sample_sys_usage();
        }
    }

    /// Writes down the tray's items when there is one the file lacks.
    fn save_tray_items(&self) {
        let now = tray_rescue::snapshot(&tray().items());
        let mut saved = self.tray_saved.borrow_mut();
        let merged = tray_rescue::merge(&saved, now);
        // Only once it is in the file: a write that failed is tried again
        // at the next tick, where `merged` comes out the same.
        if merged != *saved && tray_rescue::store(&merged) {
            *saved = merged;
        }
    }

    fn poll_settings(&self, force: bool) {
        let Some(path) = settings::settings_path() else {
            return;
        };
        let mtime = std::fs::metadata(&path).and_then(|m| m.modified()).ok();
        if !force && mtime == self.settings_mtime.get() {
            return;
        }
        self.settings_mtime.set(mtime);
        self.settings.set_if_changed(Settings::load());
    }

    fn poll_syu(&self, force: bool) {
        let Some(path) = settings::syu_state_path() else {
            return;
        };
        let mtime = std::fs::metadata(&path).and_then(|m| m.modified()).ok();
        if !force && mtime == self.syu_mtime.get() {
            return;
        }
        self.syu_mtime.set(mtime);
        self.syu.set_if_changed(SyuState::load());
    }

    fn sample_sys_usage(&self) {
        let mut cpu_pct = self.sys_usage.get_untracked().0;
        if let Ok(stat) = std::fs::read_to_string("/proc/stat")
            && let Some(line) = stat.lines().next()
        {
            let nums: Vec<u64> = line
                .split_whitespace()
                .skip(1)
                .filter_map(|t| t.parse().ok())
                .collect();
            if nums.len() >= 5 {
                let idle = nums[3] + nums[4];
                let total: u64 = nums.iter().sum();
                if let Some((prev_idle, prev_total)) = self.prev_cpu.get() {
                    let d_total = total.saturating_sub(prev_total);
                    let d_idle = idle.saturating_sub(prev_idle);
                    if d_total > 0 {
                        cpu_pct = 100.0 * (1.0 - d_idle as f64 / d_total as f64);
                    }
                }
                self.prev_cpu.set(Some((idle, total)));
            }
        }
        let mut mem_pct = self.sys_usage.get_untracked().1;
        if let Ok(meminfo) = std::fs::read_to_string("/proc/meminfo") {
            let mut total: Option<f64> = None;
            let mut available: Option<f64> = None;
            for line in meminfo.lines() {
                if let Some(rest) = line.strip_prefix("MemTotal:") {
                    total = rest.split_whitespace().next().and_then(|v| v.parse().ok());
                }
                if let Some(rest) = line.strip_prefix("MemAvailable:") {
                    available = rest.split_whitespace().next().and_then(|v| v.parse().ok());
                }
            }
            if let (Some(t), Some(a)) = (total, available)
                && t > 0.0
            {
                mem_pct = 100.0 * (1.0 - a / t);
            }
        }
        self.sys_usage.set_if_changed((cpu_pct, mem_pct));
    }
}

fn schedule(state: AppState) {
    set_timeout(Duration::from_millis(250), move || {
        if !state.view.is_alive() {
            return;
        }
        state.service_tick();
        schedule(state);
    });
}

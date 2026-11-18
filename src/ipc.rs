//! ShojiWM IPC: the workspace view model and command helpers from
//! `services/ShojiIpc.qml`, over the shared MinkaIPC client.
//!
//! MinkaIPC's I/O thread reconnects forever (the socket is recreated on
//! compositor config reload); this controller is drained by the service
//! ticker. Commands are id-correlated requests whose replies we usually
//! ignore, which is wire-compatible with the QML's fire-and-forget sends.

use calloop::channel::Channel;
use minka_ipc::{IpcClient, IpcEvent};
use serde_json::{Value, json};
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Instant;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Win {
    pub id: String,
    pub title: String,
    pub app_id: String,
    pub focused: bool,
    pub minimized: bool,
    pub maximized: bool,
}

impl Win {
    fn parse(v: &Value) -> Self {
        let s = |k: &str| v.get(k).and_then(Value::as_str).unwrap_or("").to_owned();
        let b = |k: &str| v.get(k).and_then(Value::as_bool).unwrap_or(false);
        Self {
            id: s("id"),
            title: s("title"),
            app_id: s("appId"),
            focused: b("focused"),
            minimized: b("minimized"),
            maximized: b("maximized"),
        }
    }

    pub fn label(&self) -> &str {
        if !self.title.is_empty() {
            &self.title
        } else if !self.app_id.is_empty() {
            &self.app_id
        } else {
            "?"
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Workspace {
    pub index: i64,
    pub active: bool,
    pub is_tiled: bool,
    pub window_count: i64,
    pub windows: Vec<Win>,
}

impl Workspace {
    fn parse(v: &Value) -> Self {
        Self {
            index: v.get("index").and_then(Value::as_i64).unwrap_or(0),
            active: v.get("active").and_then(Value::as_bool).unwrap_or(false),
            is_tiled: v.get("isTiled").and_then(Value::as_bool).unwrap_or(false),
            window_count: v.get("windowCount").and_then(Value::as_i64).unwrap_or(0),
            windows: v
                .get("windows")
                .and_then(Value::as_array)
                .map(|a| a.iter().map(Win::parse).collect())
                .unwrap_or_default(),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Monitor {
    pub name: String,
    pub workspaces: Vec<Workspace>,
}

impl Monitor {
    fn parse(v: &Value) -> Self {
        Self {
            name: v
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_owned(),
            workspaces: v
                .get("workspaces")
                .and_then(Value::as_array)
                .map(|a| a.iter().map(Workspace::parse).collect())
                .unwrap_or_default(),
        }
    }

    pub fn active_workspace(&self) -> Option<&Workspace> {
        self.workspaces.iter().find(|w| w.active)
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct WorkspaceView {
    pub current_monitor: String,
    pub monitors: Vec<Monitor>,
}

impl WorkspaceView {
    pub fn parse(v: &Value) -> Option<Self> {
        let monitors = v.get("monitors")?.as_array()?;
        Some(Self {
            current_monitor: v
                .get("currentMonitor")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_owned(),
            monitors: monitors.iter().map(Monitor::parse).collect(),
        })
    }

    pub fn monitor_view(&self, connector: &str) -> Option<&Monitor> {
        self.monitors
            .iter()
            .find(|m| m.name == connector)
            .or_else(|| {
                self.monitors
                    .iter()
                    .find(|m| m.name == self.current_monitor)
            })
            .or_else(|| self.monitors.first())
    }

    /// The id of the window that has the keyboard, on any monitor.
    fn focused_id(&self) -> Option<&str> {
        self.monitors
            .iter()
            .flat_map(|m| m.workspaces.iter())
            .flat_map(|w| w.windows.iter())
            .find(|w| w.focused)
            .map(|w| w.id.as_str())
    }

    /// Whether some window here has the keyboard.
    pub fn has_focused_window(&self) -> bool {
        self.focused_id().is_some()
    }

    /// When no window here has the keyboard, the one that had it in `before`
    /// is marked as having it, if it is still there. For while the shell's
    /// own start menu holds the keyboard: the compositor then reports no
    /// focused window, and the title in the bar and the dock's highlight
    /// would go for as long as the menu is open.
    pub fn keep_focus_of(&mut self, before: &WorkspaceView) {
        if self.focused_id().is_some() {
            return;
        }
        let Some(id) = before.focused_id() else {
            return;
        };
        if let Some(window) = self
            .monitors
            .iter_mut()
            .flat_map(|m| m.workspaces.iter_mut())
            .flat_map(|w| w.windows.iter_mut())
            .find(|w| w.id == id)
        {
            window.focused = true;
        }
    }

    pub fn focused_window(&self, duo: bool, monitor_name: &str) -> Option<Win> {
        let scan = |m: &Monitor| {
            m.workspaces
                .iter()
                .flat_map(|w| w.windows.iter())
                .find(|w| w.focused)
                .cloned()
        };
        if duo {
            self.monitors.iter().find_map(scan)
        } else {
            self.monitor_view(monitor_name).and_then(scan)
        }
    }

    pub fn window_exists(&self, id: &str) -> bool {
        self.monitors
            .iter()
            .flat_map(|m| m.workspaces.iter())
            .flat_map(|w| w.windows.iter())
            .any(|w| w.id == id)
    }
}

/// Decoded news for the service ticker to fold into signals.
pub enum IpcNews {
    Connected,
    Disconnected,
    View(WorkspaceView),
    UiStartMenu {
        connector: Option<String>,
        action: String,
    },
    ReorderResult {
        window_id: String,
        ok: bool,
        changed: bool,
        error: Option<String>,
    },
}

enum Pending {
    Get,
    Reorder(String),
}

pub struct IpcController {
    client: RefCell<Option<Arc<IpcClient>>>,
    events: RefCell<Option<Channel<IpcEvent>>>,
    started: Cell<bool>,
    pending: RefCell<HashMap<u64, Pending>>,
    last_rx: Cell<Option<Instant>>,
}

impl IpcController {
    pub fn new() -> Rc<Self> {
        Rc::new(Self {
            client: RefCell::new(None),
            events: RefCell::new(None),
            started: Cell::new(false),
            pending: RefCell::new(HashMap::new()),
            last_rx: Cell::new(None),
        })
    }

    pub fn ensure_started(&self) {
        if self.started.get() {
            return;
        }
        self.started.set(true);
        if let Ok((client, events)) = IpcClient::spawn() {
            *self.client.borrow_mut() = Some(client);
            *self.events.borrow_mut() = Some(events);
        }
    }

    fn request(&self, method: &str, params: Value, pending: Option<Pending>) {
        self.ensure_started();
        let id = self
            .client
            .borrow()
            .as_ref()
            .and_then(|c| c.request(method, params));
        if let (Some(id), Some(p)) = (id, pending) {
            self.pending.borrow_mut().insert(id, p);
        }
    }

    pub fn refresh(&self) {
        self.request("workspaces.get", json!(null), Some(Pending::Get));
    }

    pub fn activate_workspace(&self, monitor: &str, index: i64) {
        self.request(
            "workspaces.activate",
            json!({ "monitor": monitor, "index": index }),
            None,
        );
    }

    pub fn toggle_tiling(&self, monitor: &str) {
        self.request(
            "workspaces.toggleTiling",
            json!({ "monitor": monitor }),
            None,
        );
    }

    pub fn activate_window(&self, id: &str) {
        self.request("windows.activate", json!({ "windowId": id }), None);
    }

    pub fn close_window(&self, id: &str) {
        self.request("windows.close", json!({ "windowId": id }), None);
    }

    pub fn minimize_window(&self, id: &str) {
        self.request("windows.minimize", json!({ "windowId": id }), None);
    }

    pub fn maximize_window(&self, id: &str, maximized: bool) {
        self.request(
            "windows.maximize",
            json!({ "windowId": id, "maximized": maximized }),
            None,
        );
    }

    pub fn reorder_window(&self, id: &str, before_id: Option<&str>) {
        self.request(
            "windows.reorder",
            json!({ "windowId": id, "beforeId": before_id }),
            Some(Pending::Reorder(id.to_owned())),
        );
    }

    /// QML liveness probe, simplified: after 15s without any inbound line,
    /// re-request the view. (The QML also force-drops a wedged socket after
    /// a 5s unanswered probe; MinkaIPC exposes no drop, and its reader
    /// thread already reconnects on EOF.)
    pub fn probe_if_idle(&self) {
        if !self.started.get() {
            return;
        }
        let idle = self
            .last_rx
            .get()
            .is_none_or(|t| t.elapsed().as_secs() > 15);
        if idle {
            self.refresh();
            self.last_rx.set(Some(Instant::now()));
        }
    }

    pub fn take_news(&self) -> Vec<IpcNews> {
        let mut out = Vec::new();
        let events = self.events.borrow();
        let Some(channel) = events.as_ref() else {
            return out;
        };
        while let Ok(event) = channel.try_recv() {
            self.last_rx.set(Some(Instant::now()));
            match event {
                IpcEvent::Connected => out.push(IpcNews::Connected),
                IpcEvent::Disconnected => out.push(IpcNews::Disconnected),
                IpcEvent::Broadcast { event, payload } => match event.as_str() {
                    "workspaces.changed" => {
                        if let Some(view) = WorkspaceView::parse(&payload) {
                            out.push(IpcNews::View(view));
                        }
                    }
                    "ui.startMenu" => {
                        out.push(IpcNews::UiStartMenu {
                            connector: payload
                                .get("connector")
                                .and_then(Value::as_str)
                                .map(str::to_owned),
                            action: payload
                                .get("action")
                                .and_then(Value::as_str)
                                .unwrap_or("toggle")
                                .to_owned(),
                        });
                    }
                    _ => {}
                },
                IpcEvent::Response { id, result } => match self.pending.borrow_mut().remove(&id) {
                    Some(Pending::Get) => {
                        if let Ok(value) = &result
                            && let Some(view) = WorkspaceView::parse(value)
                        {
                            out.push(IpcNews::View(view));
                        }
                    }
                    Some(Pending::Reorder(window_id)) => match result {
                        Ok(value) => out.push(IpcNews::ReorderResult {
                            window_id,
                            ok: value.get("ok").and_then(Value::as_bool).unwrap_or(false),
                            changed: value
                                .get("changed")
                                .and_then(Value::as_bool)
                                .unwrap_or(false),
                            error: None,
                        }),
                        Err(err) => out.push(IpcNews::ReorderResult {
                            window_id,
                            ok: false,
                            changed: false,
                            error: Some(match &err {
                                Value::String(s) => s.clone(),
                                other => other.to_string(),
                            }),
                        }),
                    },
                    None => {}
                },
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One monitor with one workspace holding `windows`: (id, focused).
    fn view(windows: &[(&str, bool)]) -> WorkspaceView {
        WorkspaceView {
            current_monitor: "eDP-1".into(),
            monitors: vec![Monitor {
                name: "eDP-1".into(),
                workspaces: vec![Workspace {
                    index: 1,
                    active: true,
                    is_tiled: false,
                    window_count: windows.len() as i64,
                    windows: windows
                        .iter()
                        .map(|(id, focused)| Win {
                            id: (*id).into(),
                            focused: *focused,
                            ..Win::default()
                        })
                        .collect(),
                }],
            }],
        }
    }

    #[test]
    fn the_window_that_had_the_keyboard_keeps_its_mark() {
        let before = view(&[("a", false), ("b", true)]);
        let mut now = view(&[("a", false), ("b", false)]);
        now.keep_focus_of(&before);
        assert_eq!(now.focused_id(), Some("b"));
        assert_eq!(now, before);
    }

    #[test]
    fn a_window_that_has_the_keyboard_is_left_alone() {
        let before = view(&[("a", false), ("b", true)]);
        let mut now = view(&[("a", true), ("b", false)]);
        now.keep_focus_of(&before);
        assert_eq!(now.focused_id(), Some("a"));
    }

    #[test]
    fn a_window_that_is_gone_is_not_brought_back() {
        let before = view(&[("a", false), ("b", true)]);
        let mut now = view(&[("a", false)]);
        now.keep_focus_of(&before);
        assert_eq!(now.focused_id(), None);
    }

    #[test]
    fn with_no_window_focused_before_none_is_after() {
        let before = view(&[("a", false)]);
        let mut now = view(&[("a", false)]);
        now.keep_focus_of(&before);
        assert_eq!(now.focused_id(), None);
    }
}

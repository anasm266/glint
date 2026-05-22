use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tauri::{AppHandle, Emitter};

use crate::session::Session;

pub const SPAWN_LINK_WINDOW_MS: u64 = 120_000;

/// Maps child conversation ids to parent sessions for multitask rollup.
#[derive(Debug, Default)]
pub struct SessionRouting {
    pub child_to_parent: HashMap<String, String>,
    /// Parent session id -> timestamp ms until which orphan children may link.
    pub spawn_window_until_ms: HashMap<String, u64>,
}

impl SessionRouting {
    pub fn link_child(&mut self, child: &str, parent: &str) {
        if child.is_empty() || parent.is_empty() || child == parent {
            return;
        }
        self.child_to_parent.insert(child.to_string(), parent.to_string());
    }

    pub fn remove_child(&mut self, child: &str) {
        self.child_to_parent.remove(child);
    }

    pub fn note_task_spawn(&mut self, parent_id: &str, ts: u64) {
        if parent_id.is_empty() {
            return;
        }
        let until = ts.saturating_add(SPAWN_LINK_WINDOW_MS);
        self.spawn_window_until_ms
            .insert(parent_id.to_string(), until);
    }

    /// Pick the parent with the most recent active Task spawn window.
    pub fn try_link_orphan_child(&self, _raw_conv: &str, ts: u64) -> Option<String> {
        let mut best: Option<(String, u64)> = None;
        for (parent, until) in &self.spawn_window_until_ms {
            if *until >= ts {
                if best.as_ref().map(|(_, u)| *until > *u).unwrap_or(true) {
                    best = Some((parent.clone(), *until));
                }
            }
        }
        best.map(|(p, _)| p)
    }

    pub fn resolve_parent(&self, raw_conv: &str, p: &serde_json::Value) -> String {
        if let Some(parent) = self.child_to_parent.get(raw_conv) {
            return parent.clone();
        }
        if let Some(parent) = p.get("parent_conversation_id").and_then(|v| v.as_str()) {
            if !parent.is_empty() && parent != raw_conv {
                return parent.to_string();
            }
        }
        if !raw_conv.is_empty() {
            return raw_conv.to_string();
        }
        p.get("session_id")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string()
    }

    pub fn active_children(&self, parent_id: &str, sessions: &HashMap<String, Session>) -> usize {
        self.child_to_parent
            .iter()
            .filter(|(_, p)| p.as_str() == parent_id)
            .filter(|(child, _)| sessions.contains_key(child.as_str()))
            .count()
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Corner {
    Tl,
    Tr,
    Bl,
    Br,
}

impl Default for Corner {
    fn default() -> Self {
        Corner::Tr
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub corner: Corner,
    pub opacity: f32,
    pub codex_connected: bool,
    pub cursor_connected: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            corner: Corner::Tr,
            opacity: 0.85,
            codex_connected: false,
            cursor_connected: false,
        }
    }
}

#[derive(Debug)]
pub struct AppState {
    sessions: RwLock<HashMap<String, Session>>,
    routing: RwLock<SessionRouting>,
    settings: RwLock<Settings>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            sessions: RwLock::new(HashMap::new()),
            routing: RwLock::new(SessionRouting::default()),
            settings: RwLock::new(Settings::default()),
        }
    }

    pub fn settings(&self) -> Settings {
        self.settings.read().clone()
    }

    pub fn set_settings<F: FnOnce(&mut Settings)>(&self, f: F) -> Settings {
        let mut s = self.settings.write();
        f(&mut s);
        s.clone()
    }

    pub fn snapshot(&self) -> Vec<Session> {
        let s = self.sessions.read();
        let mut v: Vec<Session> = s.values().cloned().collect();
        // Most recently active first; the UI does its own priority sort.
        v.sort_by(|a, b| b.last_event_at_ms.cmp(&a.last_event_at_ms));
        v
    }

    pub fn with_sessions<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&mut HashMap<String, Session>) -> R,
    {
        let mut s = self.sessions.write();
        f(&mut s)
    }

    pub fn with_session_state<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&mut HashMap<String, Session>, &mut SessionRouting) -> R,
    {
        let mut s = self.sessions.write();
        let mut r = self.routing.write();
        f(&mut s, &mut r)
    }

    pub fn emit_snapshot(&self, app: &AppHandle) {
        let snap = self.snapshot();
        let _ = app.emit("sessions:update", &snap);
    }

    pub fn remove_session(&self, id: &str) {
        self.sessions.write().remove(id);
        self.routing.write().remove_child(id);
    }
}

pub type SharedState = Arc<AppState>;

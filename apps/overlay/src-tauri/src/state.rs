use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter};

use crate::file_log;
use crate::session::{self, Session};

pub const SPAWN_LINK_WINDOW_MS: u64 = 120_000;
/// Drop child conversations with no rollup events for this long.
pub const CHILD_QUIESCE_MS: u64 = 20_000;

/// Maps child conversation ids to parent sessions for multitask rollup.
#[derive(Debug, Default)]
pub struct SessionRouting {
    pub child_to_parent: HashMap<String, String>,
    /// Parent session id -> timestamp ms until which orphan children may link.
    pub spawn_window_until_ms: HashMap<String, u64>,
    /// Last rollup event timestamp per child conversation id.
    pub child_last_event_ms: HashMap<String, u64>,
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

    pub fn note_task_spawn(&mut self, session_id: &str, ts: u64) {
        let parent_id = self.resolve_root(session_id);
        if parent_id.is_empty() {
            return;
        }
        let until = ts.saturating_add(SPAWN_LINK_WINDOW_MS);
        self.spawn_window_until_ms
            .insert(parent_id.to_string(), until);
    }

    pub fn resolve_root(&self, id: &str) -> String {
        let mut current = id.to_string();
        for _ in 0..32 {
            match self.child_to_parent.get(&current) {
                Some(p) if !p.is_empty() && p != &current => current = p.clone(),
                _ => break,
            }
        }
        current
    }

    pub fn touch_child(&mut self, child: &str, ts: u64) {
        if !child.is_empty() {
            self.child_last_event_ms.insert(child.to_string(), ts);
        }
    }

    /// Remove child conversations with no rollup activity for [`CHILD_QUIESCE_MS`].
    /// Returns true if any child was unlinked from the parent session.
    pub fn prune_stale_children(&mut self, entry: &mut Session, ts: u64) -> bool {
        let before = entry.active_child_conversations.len();
        let stale: Vec<String> = entry
            .active_child_conversations
            .iter()
            .filter(|child| {
                self.child_last_event_ms
                    .get(*child)
                    .map(|last| ts.saturating_sub(*last) > CHILD_QUIESCE_MS)
                    .unwrap_or(true)
            })
            .cloned()
            .collect();
        for child in stale {
            entry.active_child_conversations.remove(&child);
            self.child_to_parent.remove(&child);
            self.child_last_event_ms.remove(&child);
        }
        entry.active_child_conversations.len() != before
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
        best.map(|(p, _)| self.resolve_root(&p))
    }

    pub fn resolve_parent(&self, raw_conv: &str, p: &serde_json::Value) -> String {
        let mut id = if let Some(parent) = self.child_to_parent.get(raw_conv) {
            parent.clone()
        } else if let Some(parent) = p.get("parent_conversation_id").and_then(|v| v.as_str()) {
            if !parent.is_empty() && parent != raw_conv {
                parent.to_string()
            } else {
                String::new()
            }
        } else {
            String::new()
        };
        if id.is_empty() {
            if !raw_conv.is_empty() {
                id = raw_conv.to_string();
            } else {
                id = p.get("session_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
            }
        }
        self.resolve_root(&id)
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
        file_log::log_snapshot(&snap);
        let _ = app.emit("sessions:update", &snap);
    }

    /// Wall-clock ms for quiescence when hooks go idle after `pending_stop`.
    pub fn wall_clock_ms() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64
    }

    /// Prune quiet children and apply deferred parent stops (no new hook required).
    pub fn sweep_pending_stops(&self) -> bool {
        let now = Self::wall_clock_ms();
        let changed = self.with_session_state(|map, routing| {
            session::reconcile_pending_stops(map, routing, now)
        });
        changed
    }

    pub fn remove_session(&self, id: &str) {
        self.sessions.write().remove(id);
        self.routing.write().remove_child(id);
    }
}

pub type SharedState = Arc<AppState>;

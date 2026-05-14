use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tauri::{AppHandle, Emitter};

use crate::session::Session;

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
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            corner: Corner::Tr,
            opacity: 0.85,
            codex_connected: false,
        }
    }
}

#[derive(Debug)]
pub struct AppState {
    sessions: RwLock<HashMap<String, Session>>,
    settings: RwLock<Settings>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            sessions: RwLock::new(HashMap::new()),
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

    pub fn emit_snapshot(&self, app: &AppHandle) {
        let snap = self.snapshot();
        let _ = app.emit("sessions:update", &snap);
    }
}

pub type SharedState = Arc<AppState>;

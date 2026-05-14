//! Install and remove our Codex hook entries in `~/.codex/config.toml`.
//!
//! We always preserve user-authored config: only entries we wrote are
//! removed on disconnect, identified by an `overlay_managed = true` field
//! we add to every hook table we create.

use anyhow::{anyhow, Context, Result};
use std::path::{Path, PathBuf};
use toml_edit::{ArrayOfTables, DocumentMut, Item, Table};

const MANAGED_KEY: &str = "overlay_managed";
const EVENTS: &[&str] = &[
    "SessionStart",
    "PreToolUse",
    "PostToolUse",
    "UserPromptSubmit",
    "Stop",
];

pub fn config_path() -> Result<PathBuf> {
    let home = dirs::home_dir().ok_or_else(|| anyhow!("no home dir"))?;
    Ok(home.join(".codex").join("config.toml"))
}

pub fn is_installed() -> bool {
    let Ok(path) = config_path() else { return false };
    let Ok(text) = std::fs::read_to_string(&path) else { return false };
    let Ok(doc) = text.parse::<DocumentMut>() else { return false };
    EVENTS.iter().all(|ev| has_managed_entry(&doc, ev))
}

pub fn install(hook_exe: &Path) -> Result<()> {
    let path = config_path()?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("creating {}", parent.display()))?;
    }

    let original = std::fs::read_to_string(&path).unwrap_or_default();

    // One-time backup. Never overwrite an existing backup.
    let backup = path.with_extension("toml.overlay-backup");
    if !backup.exists() && !original.is_empty() {
        std::fs::write(&backup, &original)
            .with_context(|| format!("writing backup {}", backup.display()))?;
    }

    let mut doc: DocumentMut = if original.is_empty() {
        DocumentMut::new()
    } else {
        original
            .parse()
            .with_context(|| format!("parsing {}", path.display()))?
    };

    enable_feature_flag(&mut doc);

    let command = build_command(hook_exe);
    for ev in EVENTS {
        upsert_managed_entry(&mut doc, ev, &command);
    }

    std::fs::write(&path, doc.to_string())
        .with_context(|| format!("writing {}", path.display()))?;
    Ok(())
}

pub fn remove() -> Result<()> {
    let path = config_path()?;
    let Ok(text) = std::fs::read_to_string(&path) else {
        return Ok(());
    };
    let mut doc: DocumentMut = text.parse().with_context(|| format!("parsing {}", path.display()))?;
    for ev in EVENTS {
        remove_managed_entries(&mut doc, ev);
    }
    std::fs::write(&path, doc.to_string())
        .with_context(|| format!("writing {}", path.display()))?;
    Ok(())
}

fn build_command(hook_exe: &Path) -> String {
    // Never use canonicalize() — on Windows it produces \\?\ UNC prefixes that
    // break process spawning. Never wrap in quotes — Codex on Windows parses the
    // command naively and treats the quote characters as part of the filename.
    // Forward slashes work on Windows and need no TOML escaping.
    // The event name is NOT appended here; the hook binary reads it from the
    // `hook_event_name` field in the stdin JSON payload instead.
    hook_exe.to_string_lossy().replace('\\', "/").to_string()
}

fn enable_feature_flag(doc: &mut DocumentMut) {
    let features = doc
        .as_table_mut()
        .entry("features")
        .or_insert_with(|| Item::Table(Table::new()));
    if let Item::Table(t) = features {
        t.set_implicit(false);
        t["codex_hooks"] = toml_edit::value(true);
    }
}

fn ensure_array<'a>(doc: &'a mut DocumentMut, event: &str) -> &'a mut ArrayOfTables {
    let hooks = doc
        .as_table_mut()
        .entry("hooks")
        .or_insert_with(|| Item::Table(Table::new()));
    let Item::Table(hooks_table) = hooks else {
        // If user put `hooks` as something other than a table, replace it.
        *hooks = Item::Table(Table::new());
        let Item::Table(hooks_table) = hooks else { unreachable!() };
        hooks_table.set_implicit(true);
        let aot = ArrayOfTables::new();
        hooks_table.insert(event, Item::ArrayOfTables(aot));
        let Some(Item::ArrayOfTables(aot)) = hooks_table.get_mut(event) else {
            unreachable!()
        };
        return aot;
    };
    hooks_table.set_implicit(true);

    if !matches!(hooks_table.get(event), Some(Item::ArrayOfTables(_))) {
        hooks_table.insert(event, Item::ArrayOfTables(ArrayOfTables::new()));
    }
    let Some(Item::ArrayOfTables(aot)) = hooks_table.get_mut(event) else {
        unreachable!()
    };
    aot
}

fn has_managed_entry(doc: &DocumentMut, event: &str) -> bool {
    let Some(Item::Table(hooks)) = doc.as_table().get("hooks") else {
        return false;
    };
    let Some(Item::ArrayOfTables(aot)) = hooks.get(event) else {
        return false;
    };
    aot.iter().any(|t| is_managed(t))
}

fn is_managed(t: &Table) -> bool {
    t.get(MANAGED_KEY)
        .and_then(|v| v.as_value())
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
}

fn upsert_managed_entry(doc: &mut DocumentMut, event: &str, command: &str) {
    let aot = ensure_array(doc, event);

    // Drop any existing managed entry first; we always rewrite our own.
    let mut i = 0;
    while i < aot.len() {
        let drop = aot.get(i).map(is_managed).unwrap_or(false);
        if drop {
            aot.remove(i);
        } else {
            i += 1;
        }
    }

    let mut group = Table::new();
    group[MANAGED_KEY] = toml_edit::value(true);

    let mut hook = Table::new();
    hook["type"] = toml_edit::value("command");
    hook["command"] = toml_edit::value(command.to_string());

    let mut handlers = ArrayOfTables::new();
    handlers.push(hook);
    group.insert("hooks", Item::ArrayOfTables(handlers));

    aot.push(group);
}

fn remove_managed_entries(doc: &mut DocumentMut, event: &str) {
    let Some(Item::Table(hooks)) = doc.as_table_mut().get_mut("hooks") else {
        return;
    };
    let Some(Item::ArrayOfTables(aot)) = hooks.get_mut(event) else {
        return;
    };
    let mut i = 0;
    while i < aot.len() {
        if aot.get(i).map(is_managed).unwrap_or(false) {
            aot.remove(i);
        } else {
            i += 1;
        }
    }
    if aot.is_empty() {
        hooks.remove(event);
    }
}

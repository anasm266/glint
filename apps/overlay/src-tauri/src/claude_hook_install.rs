//! Install and remove overlay hook entries in `~/.claude/settings.json`.
//!
//! Claude Code uses a nested schema: `hooks → EventName → [matcher groups] → { hooks: [...] }`.
//! Only entries we wrote (`overlay_managed = true`) are removed on disconnect.

use anyhow::{anyhow, Context, Result};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

const MANAGED_KEY: &str = "overlay_managed";
const EVENTS: &[&str] = &[
    "SessionStart",
    "SessionEnd",
    "UserPromptSubmit",
    "PreToolUse",
    "PostToolUse",
    "Stop",
    "StopFailure",
    "SubagentStart",
    "SubagentStop",
];

pub fn config_path() -> Result<PathBuf> {
    let home = dirs::home_dir().ok_or_else(|| anyhow!("no home dir"))?;
    Ok(home.join(".claude").join("settings.json"))
}

pub fn is_installed() -> bool {
    let Ok(path) = config_path() else {
        return false;
    };
    is_installed_at(&path)
}

pub fn is_installed_at(path: &Path) -> bool {
    let Ok(text) = std::fs::read_to_string(path) else {
        return false;
    };
    let Ok(doc) = serde_json::from_str::<Value>(&text) else {
        return false;
    };
    EVENTS.iter().all(|ev| has_managed_entry(&doc, ev))
}

pub fn install(hook_exe: &Path) -> Result<()> {
    let path = config_path()?;
    install_at(&path, hook_exe)
}

pub fn install_at(path: &Path, hook_exe: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("creating {}", parent.display()))?;
    }

    let original = std::fs::read_to_string(path).unwrap_or_default();

    let backup = path.with_extension("json.overlay-backup");
    if !backup.exists() && !original.is_empty() {
        std::fs::write(&backup, &original)
            .with_context(|| format!("writing backup {}", backup.display()))?;
    }

    let mut doc: Value = if original.is_empty() {
        json!({ "hooks": {} })
    } else {
        serde_json::from_str(&original)
            .with_context(|| format!("parsing {}", path.display()))?
    };

    let hooks = doc
        .as_object_mut()
        .and_then(|o| o.get_mut("hooks"))
        .and_then(|h| h.as_object_mut());
    let hooks = match hooks {
        Some(h) => h,
        None => {
            doc["hooks"] = json!({});
            doc["hooks"].as_object_mut().expect("hooks object")
        }
    };

    let command = build_command(hook_exe);
    for ev in EVENTS {
        upsert_managed_entry(hooks, ev, &command);
    }

    let out = serde_json::to_string_pretty(&doc).context("serializing settings.json")?;
    std::fs::write(path, out).with_context(|| format!("writing {}", path.display()))?;
    Ok(())
}

pub fn remove() -> Result<()> {
    let path = config_path()?;
    remove_at(&path)
}

pub fn remove_at(path: &Path) -> Result<()> {
    let Ok(text) = std::fs::read_to_string(path) else {
        return Ok(());
    };
    let mut doc: Value =
        serde_json::from_str(&text).with_context(|| format!("parsing {}", path.display()))?;
    let Some(hooks) = doc.get_mut("hooks").and_then(|h| h.as_object_mut()) else {
        return Ok(());
    };
    for ev in EVENTS {
        remove_managed_entries(hooks, ev);
    }
    let out = serde_json::to_string_pretty(&doc).context("serializing settings.json")?;
    std::fs::write(path, out).with_context(|| format!("writing {}", path.display()))?;
    Ok(())
}

fn build_command(hook_exe: &Path) -> String {
    hook_exe.to_string_lossy().replace('\\', "/")
}

fn has_managed_entry(doc: &Value, event: &str) -> bool {
    let Some(groups) = doc
        .get("hooks")
        .and_then(|h| h.get(event))
        .and_then(|v| v.as_array())
    else {
        return false;
    };
    groups.iter().any(|group| group_has_managed_hook(group))
}

fn group_has_managed_hook(group: &Value) -> bool {
    group
        .get("hooks")
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().any(is_managed_hook))
        .unwrap_or(false)
}

fn is_managed_hook(hook: &Value) -> bool {
    if hook
        .get(MANAGED_KEY)
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
    {
        return true;
    }
    hook.get("command")
        .and_then(|v| v.as_str())
        .map(|c| c.contains("overlay-hook"))
        .unwrap_or(false)
}

fn upsert_managed_entry(hooks: &mut serde_json::Map<String, Value>, event: &str, command: &str) {
    let groups = hooks
        .entry(event.to_string())
        .or_insert_with(|| Value::Array(vec![]));
    let Some(items) = groups.as_array_mut() else {
        *groups = Value::Array(vec![]);
        let items = groups.as_array_mut().unwrap();
        items.push(managed_matcher_group(command));
        return;
    };

    for group in items.iter_mut() {
        if let Some(hooks_arr) = group.get_mut("hooks").and_then(|v| v.as_array_mut()) {
            let mut i = 0;
            while i < hooks_arr.len() {
                if is_managed_hook(&hooks_arr[i]) {
                    hooks_arr.remove(i);
                } else {
                    i += 1;
                }
            }
        }
    }

    items.retain(|group| {
        group
            .get("hooks")
            .and_then(|v| v.as_array())
            .map(|arr| !arr.is_empty())
            .unwrap_or(true)
    });

    items.push(managed_matcher_group(command));
}

fn managed_matcher_group(command: &str) -> Value {
    json!({
        "hooks": [{
            "type": "command",
            "command": command,
            "async": true,
            MANAGED_KEY: true
        }]
    })
}

fn remove_managed_entries(hooks: &mut serde_json::Map<String, Value>, event: &str) {
    let Some(groups) = hooks.get_mut(event).and_then(|v| v.as_array_mut()) else {
        return;
    };

    for group in groups.iter_mut() {
        if let Some(hooks_arr) = group.get_mut("hooks").and_then(|v| v.as_array_mut()) {
            let mut i = 0;
            while i < hooks_arr.len() {
                if is_managed_hook(&hooks_arr[i]) {
                    hooks_arr.remove(i);
                } else {
                    i += 1;
                }
            }
        }
    }

    groups.retain(|group| {
        group
            .get("hooks")
            .and_then(|v| v.as_array())
            .map(|arr| !arr.is_empty())
            .unwrap_or(false)
    });

    if groups.is_empty() {
        hooks.remove(event);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    fn temp_settings_path(label: &str) -> PathBuf {
        let mut dir = env::temp_dir();
        dir.push(format!(
            "overlay-claude-hooks-{}-{}",
            std::process::id(),
            label
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir.push("settings.json");
        dir
    }

    #[test]
    fn install_remove_roundtrip() {
        let path = temp_settings_path("roundtrip");
        let _ = std::fs::remove_file(&path);
        let hook = PathBuf::from("C:/tools/overlay-hook.exe");
        install_at(&path, &hook).unwrap();
        assert!(is_installed_at(&path));
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("overlay-hook"));
        assert!(text.contains("SessionStart"));
        assert!(text.contains("StopFailure"));
        assert!(text.contains("\"async\": true"));
        remove_at(&path).unwrap();
        assert!(!is_installed_at(&path));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn preserves_user_hooks() {
        let path = temp_settings_path("preserve");
        let user = json!({
            "hooks": {
                "Stop": [
                    {
                        "matcher": ".*",
                        "hooks": [{ "type": "command", "command": "./my-audit.sh" }]
                    }
                ]
            }
        });
        std::fs::write(&path, serde_json::to_string_pretty(&user).unwrap()).unwrap();
        install_at(&path, Path::new("C:/overlay-hook.exe")).unwrap();
        let doc: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        let stop = doc["hooks"]["Stop"].as_array().unwrap();
        assert_eq!(stop.len(), 2);
        let user_hooks = stop[0]["hooks"].as_array().unwrap();
        assert_eq!(user_hooks.len(), 1);
        assert!(!is_managed_hook(&user_hooks[0]));
        let ours = stop[1]["hooks"].as_array().unwrap();
        assert!(is_managed_hook(&ours[0]));
        remove_at(&path).unwrap();
        let doc: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        let stop = doc["hooks"]["Stop"].as_array().unwrap();
        assert_eq!(stop.len(), 1);
        assert!(!is_managed_hook(&stop[0]["hooks"].as_array().unwrap()[0]));
        let _ = std::fs::remove_file(&path);
    }
}

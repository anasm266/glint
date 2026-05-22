//! Install and remove overlay hook entries in `~/.cursor/hooks.json`.
//!
//! Only entries we wrote (`overlay_managed = true`) are removed on disconnect.

use anyhow::{anyhow, Context, Result};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

const MANAGED_KEY: &str = "overlay_managed";
const EVENTS: &[&str] = &[
    "sessionStart",
    "stop",
    "preToolUse",
    "postToolUse",
    "beforeSubmitPrompt",
    "afterFileEdit",
];

pub fn config_path() -> Result<PathBuf> {
    let home = dirs::home_dir().ok_or_else(|| anyhow!("no home dir"))?;
    Ok(home.join(".cursor").join("hooks.json"))
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
        json!({ "version": 1, "hooks": {} })
    } else {
        serde_json::from_str(&original)
            .with_context(|| format!("parsing {}", path.display()))?
    };

    if !doc.get("version").is_some() {
        doc["version"] = json!(1);
    }
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

    let out = serde_json::to_string_pretty(&doc).context("serializing hooks.json")?;
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
    if hooks.is_empty() && doc.get("version") == Some(&json!(1)) {
        // Keep minimal valid file if user had only our hooks.
    }
    let out = serde_json::to_string_pretty(&doc).context("serializing hooks.json")?;
    std::fs::write(path, out).with_context(|| format!("writing {}", path.display()))?;
    Ok(())
}

fn build_command(hook_exe: &Path) -> String {
    hook_exe.to_string_lossy().replace('\\', "/")
}

fn has_managed_entry(doc: &Value, event: &str) -> bool {
    let Some(arr) = doc
        .get("hooks")
        .and_then(|h| h.get(event))
        .and_then(|v| v.as_array())
    else {
        return false;
    };
    arr.iter().any(|entry| is_managed_entry(entry))
}

fn is_managed_entry(entry: &Value) -> bool {
    if entry
        .get(MANAGED_KEY)
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
    {
        return true;
    }
    entry
        .get("command")
        .and_then(|v| v.as_str())
        .map(|c| c.contains("overlay-hook"))
        .unwrap_or(false)
}

fn upsert_managed_entry(hooks: &mut serde_json::Map<String, Value>, event: &str, command: &str) {
    let arr = hooks
        .entry(event.to_string())
        .or_insert_with(|| Value::Array(vec![]));
    let Some(items) = arr.as_array_mut() else {
        *arr = Value::Array(vec![]);
        let items = arr.as_array_mut().unwrap();
        push_managed(items, command);
        return;
    };

    let mut i = 0;
    while i < items.len() {
        if is_managed_entry(&items[i]) {
            items.remove(i);
        } else {
            i += 1;
        }
    }
    push_managed(items, command);
}

fn push_managed(items: &mut Vec<Value>, command: &str) {
    items.push(json!({
        "command": command,
        MANAGED_KEY: true
    }));
}

fn remove_managed_entries(hooks: &mut serde_json::Map<String, Value>, event: &str) {
    let Some(arr) = hooks.get_mut(event).and_then(|v| v.as_array_mut()) else {
        return;
    };
    let mut i = 0;
    while i < arr.len() {
        if is_managed_entry(&arr[i]) {
            arr.remove(i);
        } else {
            i += 1;
        }
    }
    if arr.is_empty() {
        hooks.remove(event);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    fn temp_hooks_path() -> PathBuf {
        let mut dir = env::temp_dir();
        dir.push(format!("overlay-cursor-hooks-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir.push("hooks.json");
        dir
    }

    #[test]
    fn install_remove_roundtrip() {
        let path = temp_hooks_path();
        let _ = std::fs::remove_file(&path);
        let hook = PathBuf::from("C:/tools/overlay-hook.exe");
        install_at(&path, &hook).unwrap();
        assert!(is_installed_at(&path));
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("overlay-hook"));
        assert!(text.contains("sessionStart"));
        assert!(text.contains("afterFileEdit"));
        remove_at(&path).unwrap();
        assert!(!is_installed_at(&path));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn preserves_user_hooks() {
        let path = temp_hooks_path();
        let user = json!({
            "version": 1,
            "hooks": {
                "stop": [{ "command": "./my-audit.sh" }]
            }
        });
        std::fs::write(&path, serde_json::to_string_pretty(&user).unwrap()).unwrap();
        install_at(&path, Path::new("C:/overlay-hook.exe")).unwrap();
        let doc: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        let stop = doc["hooks"]["stop"].as_array().unwrap();
        assert_eq!(stop.len(), 2);
        assert!(!is_managed_entry(&stop[0]));
        assert!(is_managed_entry(&stop[1]));
        remove_at(&path).unwrap();
        let doc: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        let stop = doc["hooks"]["stop"].as_array().unwrap();
        assert_eq!(stop.len(), 1);
        assert!(!is_managed_entry(&stop[0]));
        let _ = std::fs::remove_file(&path);
    }
}

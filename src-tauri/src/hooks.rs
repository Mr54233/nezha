use std::fs;
use std::path::Path;
use std::sync::mpsc;
use std::time::Duration;

use serde_json::Value;
use tauri::{AppHandle, Emitter, Manager};

use crate::session::is_task_active;
use crate::TaskManager;

const HOOK_SCRIPT: &str = r#"import fs from 'fs';
import path from 'path';
let input = '';
process.stdin.on('data', d => input += d);
process.stdin.on('end', () => {
  try {
    const data = JSON.parse(input);
    const sid = data.session_id;
    if (!sid) process.exit(0);
    const raw = data.last_assistant_message || data.message || '';
    const msg = raw.length > 500 ? raw.slice(0, 497) + '...' : raw;
    const dir = path.join(process.env.CLAUDE_PROJECT_DIR || '', '.nezha', 'events');
    fs.mkdirSync(dir, { recursive: true });
    fs.writeFileSync(path.join(dir, sid + '.json'), JSON.stringify({
      event: data.hook_event_name || 'hook',
      session_id: sid,
      message: msg,
      notification_type: data.notification_type || '',
      ts: Date.now()
    }));
  } catch {}
});
"#;

fn hooks_dir(project_path: &str) -> std::path::PathBuf {
    Path::new(project_path).join(".nezha").join("hooks")
}

fn events_dir(project_path: &str) -> std::path::PathBuf {
    Path::new(project_path).join(".nezha").join("events")
}

fn settings_local_path(project_path: &str) -> std::path::PathBuf {
    Path::new(project_path).join(".claude").join("settings.local.json")
}

pub fn has_hooks_consent(project_path: &str) -> bool {
    let config_path = Path::new(project_path).join(".nezha").join("config.toml");
    let content = match fs::read_to_string(&config_path) {
        Ok(c) => c,
        Err(_) => return false,
    };
    let config: toml::Value = match toml::from_str(&content) {
        Ok(v) => v,
        Err(_) => return false,
    };
    match config.get("agent").and_then(|a| a.get("hooks_consent")) {
        Some(toml::Value::Boolean(true)) => true,
        // If consent was never explicitly set but hooks already exist, treat as consented
        None => hooks_already_exist(project_path),
        _ => false,
    }
}

fn hooks_already_exist(project_path: &str) -> bool {
    let settings_path = settings_local_path(project_path);
    if !settings_path.exists() {
        return false;
    }
    let Ok(content) = fs::read_to_string(&settings_path) else {
        return false;
    };
    content.contains("notification-hook.mjs")
}

pub fn ensure_hook_scripts(project_path: &str) -> Result<(), String> {
    let dir = hooks_dir(project_path);
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;

    let _ = fs::remove_file(dir.join("nezha-hook-stop.mjs"));
    let _ = fs::remove_file(dir.join("nezha-hook-notification.mjs"));
    let _ = fs::remove_file(dir.join("nezha-hook.mjs"));

    fs::write(dir.join("notification-hook.mjs"), HOOK_SCRIPT).map_err(|e| e.to_string())?;

    Ok(())
}

pub fn inject_hooks_config(project_path: &str) -> Result<(), String> {
    let settings_path = settings_local_path(project_path);

    let mut settings: Value = if settings_path.exists() {
        let content =
            fs::read_to_string(&settings_path).map_err(|e| e.to_string())?;
        serde_json::from_str(&content).unwrap_or(Value::Object(serde_json::Map::new()))
    } else {
        if let Some(parent) = settings_path.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        Value::Object(serde_json::Map::new())
    };

    let hooks = settings
        .as_object_mut()
        .unwrap()
        .entry("hooks")
        .or_insert_with(|| Value::Object(serde_json::Map::new()))
        .as_object_mut()
        .unwrap();

    let nezha_command = "node \"$CLAUDE_PROJECT_DIR\"/.nezha/hooks/notification-hook.mjs";

    let stop_entry = serde_json::json!({
        "type": "command",
        "command": nezha_command,
        "timeout": 5
    });

    let permission_entry = serde_json::json!({
        "type": "command",
        "command": nezha_command,
        "timeout": 5
    });

    append_hook_entry(hooks, "Stop", None, stop_entry);
    append_hook_entry(hooks, "Notification", Some("permission_prompt"), permission_entry);

    let json = serde_json::to_string_pretty(&settings).map_err(|e| e.to_string())?;
    fs::write(&settings_path, json).map_err(|e| e.to_string())?;

    Ok(())
}

fn append_hook_entry(
    hooks: &mut serde_json::Map<String, Value>,
    event_name: &str,
    matcher: Option<&str>,
    entry: Value,
) {
    let target_command = entry.get("command").and_then(Value::as_str).unwrap_or("");
    let config_list = hooks.entry(event_name).or_insert_with(|| Value::Array(Vec::new()));
    let arr = config_list.as_array_mut().unwrap();

    // Remove existing Nezha hook entry (avoid duplicates)
    arr.retain(|item| {
        item.get("hooks")
            .and_then(Value::as_array)
            .map(|hooks| {
                !hooks.iter().any(|h| {
                    h.get("command").and_then(Value::as_str).unwrap_or("") == target_command
                })
            })
            .unwrap_or(true)
    });

    // Build new config item
    let mut config_item = serde_json::Map::new();
    if let Some(m) = matcher {
        config_item.insert("matcher".to_string(), Value::String(m.to_string()));
    }
    config_item.insert("hooks".to_string(), Value::Array(vec![entry]));
    arr.push(Value::Object(config_item));
}

pub fn remove_hooks_config(project_path: &str) -> Result<(), String> {
    let settings_path = settings_local_path(project_path);

    if !settings_path.exists() {
        return Ok(());
    }

    let content = fs::read_to_string(&settings_path).map_err(|e| e.to_string())?;
    let mut settings: Value = serde_json::from_str(&content)
        .unwrap_or(Value::Object(serde_json::Map::new()));

    let nezha_command = "node \"$CLAUDE_PROJECT_DIR\"/.nezha/hooks/notification-hook.mjs";

    if let Some(hooks) = settings.get_mut("hooks").and_then(Value::as_object_mut) {
        for (_key, config_list) in hooks.iter_mut() {
            if let Some(arr) = config_list.as_array_mut() {
                arr.retain(|item| {
                    item.get("hooks")
                        .and_then(Value::as_array)
                        .map(|hooks| {
                            !hooks.iter().any(|h| {
                                h.get("command").and_then(Value::as_str).unwrap_or("") == nezha_command
                            })
                        })
                        .unwrap_or(true)
                });
            }
        }

        // Remove empty config lists
        let keys_to_remove: Vec<String> = hooks
            .iter()
            .filter(|(_, v)| v.as_array().map(|a| a.is_empty()).unwrap_or(false))
            .map(|(k, _)| k.clone())
            .collect();
        for key in keys_to_remove {
            hooks.remove(&key);
        }

        // Remove empty hooks object
        if hooks.is_empty() {
            if let Some(obj) = settings.as_object_mut() {
                obj.remove("hooks");
            }
        }
    }

    let json = serde_json::to_string_pretty(&settings).map_err(|e| e.to_string())?;
    fs::write(&settings_path, json).map_err(|e| e.to_string())?;

    Ok(())
}

fn find_task_id_by_session(app: &AppHandle, session_id: &str) -> Option<String> {
    let tm = app.state::<TaskManager>();
    let sessions = tm.claude_sessions.lock();
    for (task_id, info) in sessions.iter() {
        if info.session_id == session_id {
            return Some(task_id.clone());
        }
    }
    None
}

fn resolve_task_id(
    session_id: &str,
    pre_session_id: &Option<String>,
    current_task_id: &str,
    find_by_session: impl Fn(&str) -> Option<String>,
) -> Option<String> {
    if let Some(ref pre_sid) = pre_session_id {
        if session_id == pre_sid {
            return Some(current_task_id.to_string());
        }
        // pre_session_id is set but doesn't match — ignore this event
        // to avoid processing hooks from non-Nezha Claude Code instances
        return None;
    }
    find_by_session(session_id)
}

pub fn spawn_hooks_event_watcher(
    app: AppHandle,
    task_id: String,
    project_path: String,
    pre_session_id: Option<String>,
) {
    let events_path = events_dir(&project_path);
    let _ = fs::create_dir_all(&events_path);

    std::thread::spawn(move || {
        use notify::{RecursiveMode, Watcher};

        let (tx, rx) = mpsc::channel::<notify::Result<notify::Event>>();

        let mut watcher_opt = notify::RecommendedWatcher::new(tx, notify::Config::default())
            .ok()
            .and_then(|mut w| {
                w.watch(&events_path, RecursiveMode::NonRecursive).ok()?;
                Some(w)
            });

        let mut last_idle_ts: u64 = 0;
        let mut last_input_required_ts: u64 = 0;

        while is_task_active(&app, &task_id) {
            if let Ok(entries) = fs::read_dir(&events_path) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.extension().map_or(false, |e| e == "json") {
                        let (emitted_ts, emitted_status) = process_event_file(
                            &app, &path, &pre_session_id, &task_id,
                            last_idle_ts, last_input_required_ts,
                        );
                        if let Some(ts) = emitted_ts {
                            if emitted_status.as_deref() == Some("idle") {
                                last_idle_ts = ts;
                            } else {
                                last_input_required_ts = ts;
                            }
                            let _ = fs::remove_file(&path);
                        } else if let Ok(meta) = fs::metadata(&path) {
                            if let Ok(modified) = meta.modified() {
                                if let Ok(age) = modified.elapsed() {
                                    if age > Duration::from_secs(30) {
                                        let _ = fs::remove_file(&path);
                                    }
                                }
                            }
                        }
                    }
                }
            }

            if watcher_opt.is_some() {
                match rx.recv_timeout(Duration::from_secs(1)) {
                    Ok(_) | Err(mpsc::RecvTimeoutError::Timeout) => {}
                    Err(mpsc::RecvTimeoutError::Disconnected) => watcher_opt = None,
                }
            } else {
                std::thread::sleep(Duration::from_millis(500));
            }
        }

        if let Some(sid) = pre_session_id {
            let _ = fs::remove_file(events_path.join(format!("{}.json", sid)));
        }
    });
}

fn process_event_file(
    app: &AppHandle,
    path: &std::path::Path,
    pre_session_id: &Option<String>,
    current_task_id: &str,
    last_idle_ts: u64,
    last_input_required_ts: u64,
) -> (Option<u64>, Option<String>) {
    let content = match fs::read_to_string(path) {
        Ok(c) if c.len() <= 65536 => c,
        _ => return (None, None),
    };

    let event: Value = match serde_json::from_str(&content) {
        Ok(v) => v,
        Err(_) => return (None, None),
    };

    let session_id = match event.get("session_id").and_then(Value::as_str) {
        Some(id) => id.to_string(),
        None => return (None, None),
    };

    let event_type = event
        .get("event")
        .and_then(Value::as_str)
        .unwrap_or("stop");

    let notification_type = event
        .get("notification_type")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();

    let message = event
        .get("message")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();

    let ts = event.get("ts").and_then(Value::as_u64).unwrap_or(0);

    let status = if notification_type == "permission_prompt" {
        "input_required"
    } else {
        "idle"
    };

    let last_ts = if status == "idle" { last_idle_ts } else { last_input_required_ts };
    if last_ts > 0 && ts > last_ts && ts - last_ts < 10_000 {
        return (None, None);
    }

    let matched_task_id = resolve_task_id(
        &session_id,
        pre_session_id,
        current_task_id,
        |sid| find_task_id_by_session(app, sid),
    );

    if let Some(tid) = matched_task_id {
        if is_task_active(app, &tid) {
            let _ = app.emit(
                "task-status",
                serde_json::json!({
                    "task_id": tid,
                    "status": status,
                    "hook_event": event_type,
                    "hook_message": message,
                }),
            );
            return (Some(ts), Some(status.to_string()));
        }
    }
    (None, None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_task_id_pre_session_matches() {
        let result = resolve_task_id(
            "sess-abc",
            &Some("sess-abc".to_string()),
            "task-123",
            |_| panic!("fallback should not be called"),
        );
        assert_eq!(result, Some("task-123".to_string()));
    }

    #[test]
    fn resolve_task_id_pre_session_mismatch_uses_fallback() {
        let result = resolve_task_id(
            "sess-other",
            &Some("sess-abc".to_string()),
            "task-123",
            |sid| {
                if sid == "sess-other" {
                    Some("task-456".to_string())
                } else {
                    None
                }
            },
        );
        assert_eq!(result, Some("task-456".to_string()));
    }

    #[test]
    fn resolve_task_id_no_pre_session_uses_fallback() {
        let result = resolve_task_id(
            "sess-xyz",
            &None,
            "task-123",
            |sid| {
                if sid == "sess-xyz" {
                    Some("task-789".to_string())
                } else {
                    None
                }
            },
        );
        assert_eq!(result, Some("task-789".to_string()));
    }

    #[test]
    fn resolve_task_id_no_match_returns_none() {
        let result = resolve_task_id(
            "sess-unknown",
            &None,
            "task-123",
            |_| None,
        );
        assert!(result.is_none());
    }
}

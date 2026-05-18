use std::fs;
use std::path::Path;
use std::sync::mpsc;
use std::time::Duration;

use serde_json::Value;
use tauri::{AppHandle, Emitter, Manager};

use crate::session::is_task_active;
use crate::TaskManager;

const HOOK_STOP_SCRIPT: &str = r#"import fs from 'fs';
import path from 'path';
let input = '';
process.stdin.on('data', d => input += d);
process.stdin.on('end', () => {
  try {
    const data = JSON.parse(input);
    const sid = data.session_id;
    if (!sid) process.exit(0);
    const dir = path.join(process.env.CLAUDE_PROJECT_DIR || '', '.nezha', 'events');
    fs.mkdirSync(dir, { recursive: true });
    fs.writeFileSync(path.join(dir, sid + '.json'), JSON.stringify({ event: 'stop', session_id: sid, ts: Date.now() }));
  } catch {}
});
"#;

const HOOK_NOTIFICATION_SCRIPT: &str = r#"import fs from 'fs';
import path from 'path';
let input = '';
process.stdin.on('data', d => input += d);
process.stdin.on('end', () => {
  try {
    const data = JSON.parse(input);
    const sid = data.session_id;
    if (!sid) process.exit(0);
    const dir = path.join(process.env.CLAUDE_PROJECT_DIR || '', '.nezha', 'events');
    fs.mkdirSync(dir, { recursive: true });
    fs.writeFileSync(path.join(dir, sid + '.json'), JSON.stringify({ event: 'notification', session_id: sid, ts: Date.now() }));
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

pub fn ensure_hook_scripts(project_path: &str) -> Result<(), String> {
    let dir = hooks_dir(project_path);
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;

    let stop_path = dir.join("nezha-hook-stop.mjs");
    let notif_path = dir.join("nezha-hook-notification.mjs");

    fs::write(&stop_path, HOOK_STOP_SCRIPT).map_err(|e| e.to_string())?;
    fs::write(&notif_path, HOOK_NOTIFICATION_SCRIPT).map_err(|e| e.to_string())?;

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

    let stop_config = serde_json::json!([{
        "hooks": [{
            "type": "command",
            "command": "node \"$CLAUDE_PROJECT_DIR\"/.nezha/hooks/nezha-hook-stop.mjs",
            "timeout": 5
        }]
    }]);
    let notif_config = serde_json::json!([{
        "hooks": [{
            "type": "command",
            "command": "node \"$CLAUDE_PROJECT_DIR\"/.nezha/hooks/nezha-hook-notification.mjs",
            "timeout": 5
        }]
    }]);

    hooks.insert("Stop".to_string(), stop_config);
    hooks.insert("Notification".to_string(), notif_config);

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

/// Resolve an event's session_id to a task_id.
/// If the pre-generated session_id matches, return the real task_id.
/// Otherwise, fall back to session-to-task lookup via `find_task_id_by_session`.
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

        while is_task_active(&app, &task_id) {
            // Check for existing event files first
            if let Ok(entries) = fs::read_dir(&events_path) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.extension().map_or(false, |e| e == "json") {
                        process_event_file(&app, &path, &pre_session_id, &task_id);
                        let _ = fs::remove_file(&path);
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

        // Cleanup: remove event files for this task's session
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
) {
    let content = match fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => return,
    };

    let event: Value = match serde_json::from_str(&content) {
        Ok(v) => v,
        Err(_) => return,
    };

    let session_id = match event.get("session_id").and_then(Value::as_str) {
        Some(id) => id.to_string(),
        None => return,
    };

    let event_type = event
        .get("event")
        .and_then(Value::as_str)
        .unwrap_or("stop");

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
                    "status": "idle",
                    "hook_event": event_type,
                }),
            );
        }
    }
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

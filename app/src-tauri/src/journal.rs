//! Journal d'activité : les « quittances » (JSONL, auditables).

use std::fs;
use tauri::{AppHandle, Manager};

pub(crate) fn journal_path(app: &AppHandle) -> std::path::PathBuf {
    app.path()
        .app_data_dir()
        .expect("app_data_dir")
        .join("journal.jsonl")
}

pub(crate) fn now_ts() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

pub(crate) fn log_event(app: &AppHandle, kind: &str, detail: &str) {
    let line = serde_json::json!({
        "ts": now_ts(),
        "kind": kind,
        "detail": detail
    });
    if let Ok(mut f) = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(journal_path(app))
    {
        use std::io::Write;
        let _ = writeln!(f, "{}", line);
    }
}

#[tauri::command]
pub(crate) fn get_journal(app: AppHandle, limit: usize) -> Vec<serde_json::Value> {
    let Ok(content) = fs::read_to_string(journal_path(&app)) else {
        return Vec::new();
    };
    let mut events: Vec<serde_json::Value> = content
        .lines()
        .filter_map(|l| serde_json::from_str(l).ok())
        .collect();
    events.reverse(); // du plus récent au plus ancien
    events.truncate(limit.min(500));
    events
}

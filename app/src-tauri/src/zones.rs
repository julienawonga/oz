//! Zones interdites de l'écran (rectangles en pixels physiques).

use serde::{Deserialize, Serialize};
use std::fs;
use std::sync::Mutex;
use tauri::{AppHandle, Manager, State};

#[derive(Serialize, Deserialize, Clone)]
pub struct Zone {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    pub label: String,
}

pub struct ZonesState(pub Mutex<Vec<Zone>>);

pub(crate) fn zones_path(app: &AppHandle) -> std::path::PathBuf {
    app.path()
        .app_data_dir()
        .expect("app_data_dir")
        .join("zones.json")
}

pub(crate) fn load_zones(app: &AppHandle) -> Vec<Zone> {
    fs::read_to_string(zones_path(app))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

#[tauri::command]
pub(crate) fn get_zones(state: State<'_, ZonesState>) -> Vec<Zone> {
    state.0.lock().unwrap().clone()
}

#[tauri::command]
pub(crate) fn set_zones(app: AppHandle, state: State<'_, ZonesState>, zones: Vec<Zone>) -> Result<(), String> {
    let path = zones_path(&app);
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    fs::write(&path, serde_json::to_string_pretty(&zones).unwrap())
        .map_err(|e| e.to_string())?;
    *state.0.lock().unwrap() = zones;
    Ok(())
}

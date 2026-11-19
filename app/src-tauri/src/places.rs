//! Lieux nommés : « ta maison c'est ici », « va au travail ».
//! Position de la fenêtre de l'œuf (pixels physiques), dans places.json.

use serde::{Deserialize, Serialize};
use std::fs;
use tauri::{AppHandle, Emitter, Manager};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Place {
    pub name: String,
    pub x: i32,
    pub y: i32,
}

fn places_path(app: &AppHandle) -> std::path::PathBuf {
    app.path().app_data_dir().expect("app_data_dir").join("places.json")
}

pub(crate) fn load(app: &AppHandle) -> Vec<Place> {
    fs::read_to_string(places_path(app))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn save(app: &AppHandle, places: &[Place]) -> Result<(), String> {
    let path = places_path(app);
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    fs::write(&path, serde_json::to_string_pretty(places).unwrap()).map_err(|e| e.to_string())
}

/// « La maison », « ma maison », « Maison » : même lieu.
fn normalize(name: &str) -> String {
    let n = name.trim().to_lowercase();
    for prefix in ["ma ", "ta ", "mon ", "ton ", "la ", "le ", "l'", "les "] {
        if let Some(rest) = n.strip_prefix(prefix) {
            return rest.trim().to_string();
        }
    }
    n
}

pub(crate) fn find(app: &AppHandle, name: &str) -> Option<Place> {
    let key = normalize(name);
    if key.is_empty() {
        return None;
    }
    load(app).into_iter().find(|p| normalize(&p.name) == key)
}

/// Outil « lieux » : nommer_ici, aller, lister, oublier.
pub(crate) fn tool(app: &AppHandle, op: &str, name: &str) -> String {
    let mut places = load(app);
    let list = |places: &[Place]| {
        if places.is_empty() {
            "Aucun lieu enregistré.".to_string()
        } else {
            format!("Lieux enregistrés : {}.", places.iter().map(|p| p.name.as_str()).collect::<Vec<_>>().join(", "))
        }
    };
    match op {
        "lister" => list(&places),
        "nommer_ici" => {
            let name = name.trim();
            if name.is_empty() {
                return "Il faut un nom pour ce lieu.".into();
            }
            let Some(pos) = app.get_webview_window("oz").and_then(|w| w.outer_position().ok()) else {
                return "Position d'Oz illisible.".into();
            };
            let key = normalize(name);
            places.retain(|p| normalize(&p.name) != key);
            places.push(Place { name: key.clone(), x: pos.x, y: pos.y });
            match save(app, &places) {
                Ok(()) => format!("Lieu « {key} » enregistré à l'endroit où se trouve Oz."),
                Err(e) => format!("Impossible d'enregistrer le lieu : {e}"),
            }
        }
        "aller" => match find(app, name) {
            Some(p) => {
                let _ = app.emit("oz://do", serde_json::json!({"action": "goto", "x": p.x, "y": p.y}));
                format!("Oz part vers « {} ».", p.name)
            }
            None => format!("Lieu « {} » inconnu. {}", name.trim(), list(&places)),
        },
        "oublier" => {
            let key = normalize(name);
            let before = places.len();
            places.retain(|p| normalize(&p.name) != key);
            if places.len() == before {
                return format!("Lieu « {} » inconnu.", name.trim());
            }
            match save(app, &places) {
                Ok(()) => format!("Lieu « {key} » oublié."),
                Err(e) => format!("Impossible d'enregistrer : {e}"),
            }
        }
        other => format!("Opération inconnue : {other}"),
    }
}

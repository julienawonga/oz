//! Position globale du curseur (click-through pixel-perfect).

use serde::Serialize;
use windows::Win32::Foundation::POINT;
use windows::Win32::UI::WindowsAndMessaging::GetCursorPos;

/// Ce dont la boucle de l'œuf a besoin, en UN seul aller-retour : le curseur
/// relatif à la fenêtre, en pixels logiques. Avant, trois appels IPC toutes
/// les 60 ms (curseur, position, écran) saturaient le canal webview <-> Rust.
#[derive(Serialize)]
pub struct CursorLocal {
    x: f64,
    y: f64,
}

#[tauri::command]
pub(crate) fn cursor_in_window(window: tauri::WebviewWindow) -> Result<CursorLocal, String> {
    let mut p = POINT::default();
    unsafe {
        GetCursorPos(&mut p).map_err(|e| e.to_string())?;
    }
    let pos = window.outer_position().map_err(|e| e.to_string())?;
    let scale = window.scale_factor().unwrap_or(1.0);
    Ok(CursorLocal {
        x: (p.x - pos.x) as f64 / scale,
        y: (p.y - pos.y) as f64 / scale,
    })
}

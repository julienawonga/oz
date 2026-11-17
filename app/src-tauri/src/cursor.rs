//! Position globale du curseur (click-through pixel-perfect).

use serde::Serialize;
use windows::Win32::Foundation::POINT;
use windows::Win32::UI::WindowsAndMessaging::GetCursorPos;

#[derive(Serialize)]
pub struct CursorPos {
    x: i32,
    y: i32,
}

/// Position physique (device pixels) du curseur sur l'écran virtuel.
#[tauri::command]
pub(crate) fn get_cursor_pos() -> Result<CursorPos, String> {
    let mut p = POINT::default();
    unsafe {
        GetCursorPos(&mut p).map_err(|e| e.to_string())?;
    }
    Ok(CursorPos { x: p.x, y: p.y })
}

//! Vision : capture d'une fenêtre AUTORISÉE puis description par un modèle.
//! Le broker vérifie la permission AVANT la capture.

use base64::Engine as _;
use tauri::{AppHandle, Manager, State};

use crate::brain::{http_client, BrainState};
use crate::broker::{request_access, BrokerState};
use crate::journal::log_event;
use crate::memory::remember;
use crate::windows_api::{active_window, enumerate_windows, friendly_name, WindowHandle};

/// Trouve la fenêtre visée (vide = celle que l'utilisateur utilise) et vérifie
/// qu'Oz a le droit de la regarder. Sans accord : demande d'accès sur l'œuf.
pub(crate) fn lookable_window(app: &AppHandle, target: &str) -> Result<WindowHandle, String> {
    let windows = enumerate_windows();
    let found = if target.trim().is_empty() {
        // Pas de cible → la fenêtre que l'utilisateur utilise (pas l'œuf lui-même)
        active_window(windows)
    } else {
        let q = target.to_lowercase();
        windows.into_iter().find(|w| {
            w.title.to_lowercase().contains(&q) || w.process.to_lowercase().contains(&q)
        })
    };
    let Some(w) = found else {
        return Err(if target.trim().is_empty() {
            "Je ne trouve aucune fenêtre à regarder.".to_string()
        } else {
            format!("Je ne trouve pas de fenêtre « {} »", target)
        });
    };

    // 🔐 LE BROKER DÉCIDE : la capture exige look / act / trust
    match can_look(app, &w) {
        Some(true) => Ok(w),
        Some(false) => {
            request_access(app, &w, "look");
            Err(format!("J'ai besoin de ton accord pour regarder {}.", friendly_name(&w.process)))
        }
        None => Err(format!("Je ne trouve pas de fenêtre « {} »", target)),
    }
}

/// Some(true) : regard permis ; Some(false) : accord nécessaire ;
/// None : sanctuaire (la fenêtre n'existe pas pour Oz).
pub(crate) fn can_look(app: &AppHandle, w: &WindowHandle) -> Option<bool> {
    let level = app.state::<BrokerState>().0.lock().unwrap().level_for(w);
    match level.as_str() {
        "sanctuary" => None,
        "look" | "act" | "trust" => Some(true),
        _ => Some(false),
    }
}

/// Regarde une fenêtre autorisée et décrit ce qu'il voit via le modèle vision.
#[tauri::command]
pub(crate) async fn look_at(
    state: State<'_, BrainState>,
    app: AppHandle,
    target: String,
    question: String,
) -> Result<String, String> {
    let w = lookable_window(&app, &target)?;
    let jpg = w.capture_jpeg()?;
    let b64 = base64::engine::general_purpose::STANDARD.encode(&jpg);
    log_event(&app, "vision", &format!("a regardé « {} »", w.title));

    let cfg = state.config.lock().unwrap().clone();

    if cfg.api_key.is_empty() {
        return Err("pas de clé Gemini (onglet Modèles)".into());
    }
    let body = serde_json::json!({
        "contents": [{
            "role": "user",
            "parts": [
                {"text": format!("{} Réponds en {}, très brièvement, en texte simple sans markdown.", question, crate::prefs::language_name(&app))},
                {"inlineData": {"mimeType": "image/jpeg", "data": b64}}
            ]
        }],
        // lecture d'écran : pas besoin de réfléchir longtemps
        "generationConfig": {"thinkingConfig": {"thinkingLevel": "low"}}
    });
    let url = format!(
        "https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent",
        cfg.vision_model
    );
    let mut body = body;
    let mut resp = http_client()
        .post(&url)
        .header("x-goog-api-key", &cfg.api_key)
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("Gemini vision injoignable : {e}"))?;
    if resp.status() == reqwest::StatusCode::BAD_REQUEST {
        // modèle vision qui refuse le niveau de réflexion : on s'en passe
        body.as_object_mut().unwrap().remove("generationConfig");
        resp = http_client()
            .post(&url)
            .header("x-goog-api-key", &cfg.api_key)
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("Gemini vision injoignable : {e}"))?;
    }
    if !resp.status().is_success() {
        return Err(crate::brain::http_error("Gemini vision", resp).await);
    }
    let json: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
    let desc = json["candidates"][0]["content"]["parts"][0]["text"]
        .as_str()
        .unwrap_or("…")
        .to_string();
    remember(&app, w.key(), desc.chars().take(300).collect());
    Ok(desc)
}

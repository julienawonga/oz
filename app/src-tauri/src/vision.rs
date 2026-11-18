//! Vision : capture d'une fenêtre AUTORISÉE puis description par un modèle.
//! Le broker vérifie la permission AVANT la capture.

use base64::Engine as _;
use tauri::{AppHandle, State};

use crate::brain::{http_client, BrainState};
use crate::broker::{request_access, BrokerState};
use crate::journal::log_event;
use crate::memory::remember;
use crate::windows_api::{active_window, enumerate_windows, friendly_name};

/// Regarde une fenêtre autorisée et décrit ce qu'il voit via le modèle vision.
#[tauri::command]
pub(crate) async fn look_at(
    state: State<'_, BrainState>,
    broker: State<'_, BrokerState>,
    app: AppHandle,
    target: String,
    question: String,
) -> Result<String, String> {
    let windows = enumerate_windows();
    let found = if target.trim().is_empty() {
        // Pas de cible → la fenêtre que Julien utilise (pas l'œuf lui-même)
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
    let level = broker.0.lock().unwrap().level_for(&w);
    if level == "sanctuary" {
        return Err(format!("Je ne trouve pas de fenêtre « {} »", target));
    }
    if !matches!(level.as_str(), "look" | "act" | "trust") {
        request_access(&app, &w, "look");
        return Err(format!("J'ai besoin de ton accord pour regarder {}.", friendly_name(&w.process)));
    }

    let png = w.capture_png()?;
    let b64 = base64::engine::general_purpose::STANDARD.encode(&png);
    log_event(&app, "vision", &format!("a regardé « {} »", w.title));

    let cfg = state.config.lock().unwrap().clone();

    // ── Gemini vision : le même modèle que la conversation ──
    if cfg.provider == "gemini" && !cfg.api_key.is_empty() {
        let body = serde_json::json!({
            "contents": [{
                "role": "user",
                "parts": [
                    {"text": format!("{} Réponds en français, très brièvement.", question)},
                    {"inlineData": {"mimeType": "image/png", "data": b64}}
                ]
            }]
        });
        let url = format!(
            "https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent",
            cfg.vision_model
        );
        let resp = http_client()
            .post(&url)
            .header("x-goog-api-key", &cfg.api_key)
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("Gemini vision injoignable : {e}"))?;
        let json: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
        let desc = json["candidates"][0]["content"]["parts"][0]["text"]
            .as_str()
            .unwrap_or("…")
            .to_string();
        remember(&app, w.key(), desc.chars().take(300).collect());
        return Ok(desc);
    }

    // ── Ollama vision (moondream) ──
    let body = serde_json::json!({
        "model": cfg.vision_model,
        "prompt": format!("{} Réponds en français, très brièvement.", question),
        "images": [b64],
        "stream": false
    });
    let url = format!("{}/api/generate", cfg.url);
    let client = http_client();
    let resp = client
        .post(&url)
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("Ollama injoignable ({e})"))?;
    let json: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
    let desc = json["response"].as_str().unwrap_or("…").to_string();
    // 🖼️ Auto-mémoire : il retient ce qu'il a vu de ce lieu
    remember(&app, w.key(), desc.chars().take(300).collect());
    Ok(desc)
}

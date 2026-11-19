//! Sonde « Regarde avec moi », hors application :
//!   cargo run --example watch_probe <filtre de fenêtre> [question]
//! Ouvre gemini-3.8-live, envoie 3 images (1/s) de la fenêtre en
//! `realtimeInput.video`, puis pose la question. Chronomètre la capture,
//! la taille des images et le premier son.
use base64::Engine;
use futures_util::{SinkExt, StreamExt};
use std::time::Instant;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::Message;

fn main() {
    let filter = std::env::args().nth(1).expect("filtre de fenêtre requis").to_lowercase();
    let question = std::env::args().nth(2).unwrap_or("Qu'est-ce que tu vois sur mon écran, là ?".into());
    let w = oz_lib::windows_api::enumerate_windows()
        .into_iter()
        .find(|w| w.title.to_lowercase().contains(&filter) || w.process.to_lowercase().contains(&filter))
        .expect("aucune fenêtre ne correspond");
    println!("fenêtre : {} — {}", w.process, w.title);
    let _ = rustls::crypto::ring::default_provider().install_default();
    tauri::async_runtime::block_on(async move {
        let key = keyring::Entry::new("oz", "gemini_api_key").unwrap().get_password().expect("clé absente du coffre");
        let url = "wss://generativelanguage.googleapis.com/ws/google.ai.generativelanguage.v1beta.GenerativeService.BidiGenerateContent";
        let mut req = url.into_client_request().unwrap();
        req.headers_mut().insert("x-goog-api-key", key.parse().unwrap());
        let (mut ws, _) = tokio_tungstenite::connect_async(req).await.expect("connexion");
        let setup = serde_json::json!({"setup": {
            "model": "models/gemini-3.8-live",
            "generationConfig": {
                "responseModalities": ["AUDIO"],
                "speechConfig": {"languageCode": "fr-FR", "voiceConfig": {"prebuiltVoiceConfig": {"voiceName": "Kore"}}}
            },
            "systemInstruction": {"parts": [{"text": "Tu es Oz, un petit œuf compagnon de bureau. Tu vois en direct la fenêtre que Julien partage avec toi. Réponds en français, en une ou deux phrases."}]},
            "outputAudioTranscription": {},
            "contextWindowCompression": {"slidingWindow": {}}
        }});
        ws.send(Message::Text(setup.to_string())).await.unwrap();
        // attendre le setup
        loop {
            let Some(Ok(m)) = ws.next().await else { panic!("fermé pendant le setup") };
            let t = match m { Message::Text(t) => t.to_string(), Message::Binary(b) => String::from_utf8_lossy(&b).to_string(), _ => continue };
            if t.contains("setupComplete") { println!("setup accepté"); break; }
            if t.contains("error") { panic!("{t}"); }
        }
        for i in 0..3 {
            let t = Instant::now();
            let jpg = oz_lib::windows_api::capture_window_jpeg(w.raw()).expect("capture");
            let cap_ms = t.elapsed().as_millis();
            let b64 = base64::engine::general_purpose::STANDARD.encode(&jpg);
            let m = serde_json::json!({"realtimeInput": {"video": {"mimeType": "image/jpeg", "data": b64}}});
            ws.send(Message::Text(m.to_string())).await.unwrap();
            println!("image {} : capture {cap_ms} ms, {} ko", i + 1, jpg.len() / 1024);
            tokio::time::sleep(std::time::Duration::from_millis(1000u64.saturating_sub(cap_ms as u64))).await;
        }
        let t0 = Instant::now();
        let m = serde_json::json!({"clientContent": {"turns": [{"role": "user", "parts": [{"text": question}]}], "turnComplete": true}});
        ws.send(Message::Text(m.to_string())).await.unwrap();
        let mut audio = 0usize;
        let mut said = String::new();
        loop {
            let msg = match tokio::time::timeout(std::time::Duration::from_secs(20), ws.next()).await {
                Ok(Some(Ok(m))) => m,
                Ok(Some(Err(e))) => { println!("erreur ws : {e}"); break; }
                Ok(None) => { println!("fermé"); break; }
                Err(_) => { println!("TIMEOUT"); break; }
            };
            let t = match msg {
                Message::Text(t) => t.to_string(),
                Message::Binary(b) => String::from_utf8_lossy(&b).to_string(),
                Message::Close(c) => { println!("CLOSE {c:?}"); break; }
                _ => continue,
            };
            let v: serde_json::Value = serde_json::from_str(&t).unwrap_or_default();
            if v.get("error").is_some() { println!("ERREUR {t}"); break; }
            let sc = &v["serverContent"];
            if let Some(tr) = sc["outputTranscription"]["text"].as_str() { said.push_str(tr); }
            if let Some(parts) = sc["modelTurn"]["parts"].as_array() {
                for p in parts {
                    if p["inlineData"]["data"].is_string() {
                        audio += 1;
                        if audio == 1 { println!("{:>5} ms  premier son", t0.elapsed().as_millis()); }
                    }
                }
            }
            if sc["turnComplete"].as_bool() == Some(true) {
                println!("{:>5} ms  tour terminé : « {} »", t0.elapsed().as_millis(), said.trim());
                break;
            }
        }
    });
}

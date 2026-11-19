//! Sonde de la voix, hors application :
//!   cargo run --example voice_probe            -> lecture d'un texte (chemin voice_feed + voice_end)
//!   cargo run --example voice_probe -- fichier.wav  -> envoie une voix enregistrée (chemin micro)
//!     et affiche tout ce que Gemini renvoie, dont la transcription.
use futures_util::{SinkExt, StreamExt};
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::Message;

/// WAV PCM 16 bits mono -> PCM 16 kHz (interpolation linéaire)
fn wav_to_16k(path: &str) -> Vec<i16> {
    let b = std::fs::read(path).expect("wav illisible");
    let rate = u32::from_le_bytes([b[24], b[25], b[26], b[27]]) as f64;
    let s: Vec<i16> = b[44..].chunks_exact(2).map(|c| i16::from_le_bytes([c[0], c[1]])).collect();
    let n = (s.len() as f64 * 16000.0 / rate) as usize;
    (0..n)
        .map(|i| {
            let p = i as f64 * rate / 16000.0;
            let j = p as usize;
            let f = p - j as f64;
            let a = s[j.min(s.len() - 1)] as f64;
            let c = s[(j + 1).min(s.len() - 1)] as f64;
            (a + (c - a) * f) as i16
        })
        .collect()
}

fn main() {
    let wav = std::env::args().nth(1);
    let model = "models/gemini-3.8-live";
    let _ = rustls::crypto::ring::default_provider().install_default();
    tauri::async_runtime::block_on(async move {
        use base64::Engine as _;
        let key = keyring::Entry::new("oz", "gemini_api_key").unwrap().get_password().expect("clé absente du coffre");
        println!("clé : {} caractères, modèle : {model}", key.len());
        let url = "wss://generativelanguage.googleapis.com/ws/google.ai.generativelanguage.v1beta.GenerativeService.BidiGenerateContent";
        let mut req = url.into_client_request().unwrap();
        req.headers_mut().insert("x-goog-api-key", key.parse().unwrap());
        let (mut ws, _) = tokio_tungstenite::connect_async(req).await.expect("connexion");
        println!("websocket connecté");
        let setup = serde_json::json!({"setup": {"model": model,
            "generationConfig": {"responseModalities": ["AUDIO"]}, "inputAudioTranscription": {}}});
        ws.send(Message::Text(setup.to_string())).await.unwrap();
        let text = "Lis à voix haute, mot pour mot, sans rien ajouter ni commenter, ce texte : Bonjour Julien, je suis là.";
        let mut sent = false;
        let mut audio = 0usize;
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
        while std::time::Instant::now() < deadline {
            let msg = match tokio::time::timeout(std::time::Duration::from_secs(12), ws.next()).await {
                Ok(Some(Ok(m))) => m,
                Ok(Some(Err(e))) => { println!("erreur ws : {e}"); break; }
                Ok(None) => { println!("fermé"); break; }
                Err(_) => { println!("TIMEOUT : rien reçu en 12 s"); break; }
            };
            let t = match msg {
                Message::Text(t) => t.to_string(),
                Message::Binary(b) => String::from_utf8_lossy(&b).to_string(),
                Message::Close(c) => { println!("CLOSE {c:?}"); break; }
                _ => continue,
            };
            if t.contains("inlineData") { audio += 1; if audio == 1 { println!("premier chunk audio ✅"); } continue; }
            let compact = t.split_whitespace().collect::<Vec<_>>().join(" ");
            if compact != "{}" { println!("<- {}", compact.chars().take(300).collect::<String>()); }
            if t.contains("setupComplete") && !sent {
                sent = true;
                if let Some(path) = &wav {
                    // comme le micro : morceaux de ~0,25 s, au rythme réel
                    let pcm = wav_to_16k(path);
                    for c in pcm.chunks(4000) {
                        let bytes: Vec<u8> = c.iter().flat_map(|s| s.to_le_bytes()).collect();
                        let d = base64::engine::general_purpose::STANDARD.encode(bytes);
                        let m = serde_json::json!({"realtimeInput": {"audio": {"mimeType": "audio/pcm;rate=16000", "data": d}}});
                        ws.send(Message::Text(m.to_string())).await.unwrap();
                        tokio::time::sleep(std::time::Duration::from_millis(250)).await;
                    }
                    ws.send(Message::Text(serde_json::json!({"realtimeInput": {"audioStreamEnd": true}}).to_string())).await.unwrap();
                    println!("-> {:.1} s de voix envoyées + audioStreamEnd", pcm.len() as f64 / 16000.0);
                } else {
                    for m in [
                        serde_json::json!({"clientContent": {"turns": [{"role": "user", "parts": [{"text": text}]}], "turnComplete": false}}),
                        serde_json::json!({"clientContent": {"turns": [], "turnComplete": true}}),
                    ] { ws.send(Message::Text(m.to_string())).await.unwrap(); }
                    println!("-> texte + turnComplete envoyés");
                }
            }
            if t.contains("turnComplete") { break; }
        }
        println!("RÉSULTAT : {audio} chunk(s) audio");
    });
}

//! Sonde de la session Live unique (voix + outils), hors application :
//!   cargo run --example talk_probe [question]
//! LANG_CODE=fr-FR : impose la langue de la voix (speechConfig.languageCode).
//! NONBLOCK=1 : ajoute express_emotion en NON_BLOCKING, résultat en SILENT
//! (vérifie que le modèle accepte ce comportement, comme dans Oz).
//! Ouvre gemini-3.8-live avec outils et transcriptions, pose une question en
//! texte, répond aux appels d'outils par un faux résultat, et chronomètre.
use futures_util::{SinkExt, StreamExt};
use std::time::Instant;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::Message;

fn main() {
    let question = std::env::args().nth(1).unwrap_or("Quelles applications sont ouvertes sur mon écran ?".into());
    let thinking = std::env::var("THINKING").ok();
    let _ = rustls::crypto::ring::default_provider().install_default();
    tauri::async_runtime::block_on(async move {
        let key = keyring::Entry::new("oz", "gemini_api_key").unwrap().get_password().expect("clé absente du coffre");
        let url = "wss://generativelanguage.googleapis.com/ws/google.ai.generativelanguage.v1beta.GenerativeService.BidiGenerateContent";
        let mut req = url.into_client_request().unwrap();
        req.headers_mut().insert("x-goog-api-key", key.parse().unwrap());
        let (mut ws, _) = tokio_tungstenite::connect_async(req).await.expect("connexion");
        let mut gen_cfg = serde_json::json!({
            "responseModalities": ["AUDIO"],
            "speechConfig": {"voiceConfig": {"prebuiltVoiceConfig": {"voiceName": "Kore"}}}
        });
        if let Ok(lang) = std::env::var("LANG_CODE") {
            gen_cfg["speechConfig"]["languageCode"] = serde_json::json!(lang);
        }
        if let Some(level) = &thinking {
            gen_cfg["thinkingConfig"] = serde_json::json!({"thinkingLevel": level});
        }
        let nonblock = std::env::var("NONBLOCK").is_ok();
        let mut decls = vec![serde_json::json!({
            "name": "lister_fenetres",
            "description": "Liste les applications et fenêtres ouvertes sur l'écran de l'utilisateur."
        })];
        if nonblock {
            decls.push(serde_json::json!({
                "name": "express_emotion",
                "description": "Montre ton émotion. Appelle-le EN MÊME TEMPS que ta réponse quand tu ressens quelque chose.",
                "behavior": "NON_BLOCKING",
                "parameters": {"type": "object", "properties": {"emotion": {"type": "string", "enum": ["joie", "rire", "malice"]}}, "required": ["emotion"]}
            }));
        }
        let setup = serde_json::json!({"setup": {
            "model": "models/gemini-3.8-live",
            "generationConfig": gen_cfg,
            "systemInstruction": {"parts": [{"text": "Tu es Oz, un petit œuf compagnon de bureau. Réponds en français, en une ou deux phrases. Utilise tes outils quand c'est utile."}]},
            "tools": [{"functionDeclarations": decls}],
            "inputAudioTranscription": {},
            "outputAudioTranscription": {},
            "contextWindowCompression": {"slidingWindow": {}},
            "sessionResumption": {}
        }});
        ws.send(Message::Text(setup.to_string())).await.unwrap();
        let mut t0 = Instant::now();
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
            let ms = t0.elapsed().as_millis();
            let v: serde_json::Value = serde_json::from_str(&t).unwrap_or_default();
            if v.get("setupComplete").is_some() {
                println!("{ms:>5} ms  setup accepté");
                t0 = Instant::now();
                let m = serde_json::json!({"clientContent": {"turns": [{"role": "user", "parts": [{"text": question}]}], "turnComplete": true}});
                ws.send(Message::Text(m.to_string())).await.unwrap();
                continue;
            }
            if let Some(calls) = v["toolCall"]["functionCalls"].as_array() {
                println!("{ms:>5} ms  APPEL D'OUTIL : {}", serde_json::to_string(calls).unwrap());
                let responses: Vec<_> = calls.iter().map(|c| {
                    if c["name"] == "express_emotion" {
                        serde_json::json!({"id": c["id"], "name": c["name"], "response": {"result": "Montré.", "scheduling": "SILENT"}})
                    } else {
                        serde_json::json!({"id": c["id"], "name": c["name"],
                            "response": {"result": "Applications ouvertes : Spotify, Chrome, WhatsApp, Terminal."}})
                    }
                }).collect();
                let m = serde_json::json!({"toolResponse": {"functionResponses": responses}});
                ws.send(Message::Text(m.to_string())).await.unwrap();
                continue;
            }
            let sc = &v["serverContent"];
            if let Some(parts) = sc["modelTurn"]["parts"].as_array() {
                for p in parts {
                    if p["inlineData"]["data"].is_string() {
                        audio += 1;
                        if audio == 1 { println!("{ms:>5} ms  premier son ✅"); }
                    }
                }
            }
            if let Some(tr) = sc["outputTranscription"]["text"].as_str() { said.push_str(tr); }
            if sc["turnComplete"].as_bool() == Some(true) {
                println!("{ms:>5} ms  tour terminé ({audio} chunks) : « {} »", said.trim());
                if audio > 0 { break; }
            }
            if v.get("error").is_some() || t.contains("\"error\"") {
                println!("{ms:>5} ms  ERREUR {}", t.chars().take(300).collect::<String>());
            }
            if v.get("goAway").is_some() {
                println!("{ms:>5} ms  goAway {}", t.chars().take(160).collect::<String>());
            }
        }
    });
}

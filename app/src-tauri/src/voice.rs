//! Voix : session Gemini Live (full-duplex), micro streaming, TTS, STT local.

use futures_util::{SinkExt, StreamExt};
use std::fs;
use std::sync::{Mutex, OnceLock};
use tauri::{AppHandle, Emitter, Manager};
use tokio_tungstenite::tungstenite::Message as WsMessage;
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

use crate::brain::{http_client, BrainState};
use crate::journal::log_event;

pub(crate) static WHISPER_CTX: OnceLock<WhisperContext> = OnceLock::new();

pub(crate) fn whisper_model_path() -> Option<std::path::PathBuf> {
    std::env::var("APPDATA").ok().map(|d| {
        std::path::PathBuf::from(d)
            .join("com.julienawonga.oz")
            .join("models")
            .join("ggml-base.bin")
    })
}

pub(crate) fn init_whisper() -> Result<&'static WhisperContext, String> {
    if let Some(ctx) = WHISPER_CTX.get() {
        return Ok(ctx);
    }
    let path = whisper_model_path().ok_or("chemin du modèle introuvable")?;
    if !path.exists() {
        return Err("ggml-base.bin absent des modèles".into());
    }
    let ctx = WhisperContext::new_with_params(
        path.to_str().unwrap(),
        WhisperContextParameters::default(),
    )
    .map_err(|e| e.to_string())?;
    let _ = WHISPER_CTX.set(ctx);
    Ok(WHISPER_CTX.get().unwrap())
}

/// (config Gemini dispo pour les briques vocales)
pub(crate) fn gemini_voice_config(app: &AppHandle) -> Option<(String, String)> {
    // la clé vit dans le coffre Windows, chargée en mémoire dans BrainState
    let cfg = app.state::<BrainState>().config.lock().unwrap().clone();
    if cfg.api_key.is_empty() { return None; }
    Some((cfg.model, cfg.api_key))
}

pub(crate) fn bytemuck_cast(samples: &[i16]) -> Vec<u8> {
    let mut out = Vec::with_capacity(samples.len() * 2);
    for s in samples {
        out.extend_from_slice(&s.to_le_bytes());
    }
    out
}

/// Reçoit du PCM 16 kHz mono (Int16) et retourne le texte transcrit.
#[tauri::command]
pub(crate) async fn transcribe(app: AppHandle, samples: Vec<i16>) -> Result<String, String> {
    // ── Gemini transcription (provider gemini + clé) : plus rapide que whisper local ──
    {
        let cfg = gemini_voice_config(&app);
        if let Some((model, key)) = cfg {
            use base64::Engine as _;
            let b64 = base64::engine::general_purpose::STANDARD.encode(bytemuck_cast(&samples));
            let body = serde_json::json!({
                "contents": [{
                    "role": "user",
                    "parts": [
                        {"text": "Transcris exactement cet audio en français. Réponds UNIQUEMENT avec la transcription, rien d'autre."},
                        {"inlineData": {"mimeType": "audio/pcm;rate=16000", "data": b64}}
                    ]
                }]
            });
            let url = format!(
                "https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent",
                model
            );
            let resp = http_client()
                .post(&url)
                .header("x-goog-api-key", &key)
                .json(&body)
                .send()
                .await
                .map_err(|e| format!("Gemini STT injoignable : {e}"))?;
            let json: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
            let text = json["candidates"][0]["content"]["parts"][0]["text"]
                .as_str()
                .unwrap_or("")
                .trim()
                .to_string();
            if !text.is_empty() {
                return Ok(text);
            }
            // si Gemini échoue, on retombe sur whisper local ci-dessous
        }
    }

    let ctx = init_whisper()?;
    let mut state = ctx.create_state().map_err(|e| e.to_string())?;
    let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
    params.set_language(Some("fr"));
    params.set_print_progress(false);
    params.set_print_special(false);
    params.set_print_realtime(false);
    params.set_print_timestamps(false);
    let mut audio = vec![0.0f32; samples.len()];
    whisper_rs::convert_integer_to_float_audio(&samples, &mut audio)
        .map_err(|e| e.to_string())?;
    state.full(params, &audio).map_err(|e| e.to_string())?;
    let n = state.full_n_segments();
    let mut text = String::new();
    for i in 0..n {
        if let Some(seg) = state.get_segment(i) {
            if let Ok(s) = seg.to_str_lossy() {
                text.push_str(&s);
            }
        }
    }
    Ok(text.trim().to_string())
}

/// Lit le texte à voix haute avec la voix française du système (SAPI).
/// Canal unique : une nouvelle parole INTERROMPT la précédente
/// (on tue le processus PowerShell du TTS en cours).
pub(crate) static TTS_CHILD: Mutex<Option<std::process::Child>> = Mutex::new(None);

pub(crate) fn wav_header(data_len: u32, sample_rate: u32) -> Vec<u8> {
    let mut h = Vec::with_capacity(44);
    h.extend_from_slice(b"RIFF");
    h.extend_from_slice(&(36 + data_len).to_le_bytes());
    h.extend_from_slice(b"WAVEfmt ");
    h.extend_from_slice(&16u32.to_le_bytes());
    h.extend_from_slice(&1u16.to_le_bytes()); // PCM
    h.extend_from_slice(&1u16.to_le_bytes()); // mono
    h.extend_from_slice(&sample_rate.to_le_bytes());
    h.extend_from_slice(&(sample_rate * 2).to_le_bytes());
    h.extend_from_slice(&2u16.to_le_bytes());
    h.extend_from_slice(&16u16.to_le_bytes());
    h.extend_from_slice(b"data");
    h.extend_from_slice(&data_len.to_le_bytes());
    h
}

use futures_util::stream::SplitSink;
pub(crate) type WsStream = tokio_tungstenite::WebSocketStream<
    tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
>;
pub(crate) type WsTx = SplitSink<WsStream, WsMessage>;
pub(crate) static LIVE_TX: tokio::sync::Mutex<Option<WsTx>> = tokio::sync::Mutex::const_new(None);
pub(crate) static LIVE_CONNECT_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
// chunks micro mis en mémoire pendant l'ouverture de session (zéro mot perdu)
pub(crate) static MIC_PENDING: tokio::sync::Mutex<Vec<String>> = tokio::sync::Mutex::const_new(Vec::new());

/// Vide le buffer micro vers la session Live (ordre préservé)
pub(crate) async fn flush_mic_pending() -> Result<(), String> {
    let mut pending = MIC_PENDING.lock().await;
    let queued: Vec<String> = std::mem::take(&mut *pending);
    let mut guard = LIVE_TX.lock().await;
    let Some(tx) = guard.as_mut() else {
        // session toujours absente : on remet les chunks en attente
        *pending = queued;
        return Ok(());
    };
    for d in queued {
        let msg = serde_json::json!({
            "realtimeInput": {"audio": {"mimeType": "audio/pcm;rate=16000", "data": d}}
        });
        tx.send(WsMessage::Text(msg.to_string()))
            .await
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Émet un événement vocal au frontend (audio, reset, erreur, transcription)
pub(crate) fn voice_emit(app: &AppHandle, event: &str, payload: &str) {
    let _ = app.emit(&format!("oz://voice-{event}"), payload.to_string());
}

/// Ouvre la session Gemini Live si absente (persistante entre les réponses).
pub(crate) async fn ensure_live_session(app: &AppHandle, key: &str) -> Result<(), String> {
    eprintln!("[oz] voice : ensure_live_session appelé (clé de {} caractères)", key.len());
    {
        let guard = LIVE_TX.lock().await;
        if guard.is_some() {
            return Ok(()); // session déjà ouverte
        }
    }
    // verrou de connexion : un seul connect à la fois (les chunks micro
    // arrivent en rafale et tous verraient "session absente" sinon)
    let _conn = LIVE_CONNECT_LOCK.lock().await;
    {
        let guard = LIVE_TX.lock().await;
        if guard.is_some() {
            return Ok(()); // un autre appel a connecté pendant qu'on attendait
        }
    }
    let url = "wss://generativelanguage.googleapis.com/ws/google.ai.generativelanguage.v1beta.GenerativeService.BidiGenerateContent";
    use tokio_tungstenite::tungstenite::client::IntoClientRequest;
    let mut req = url
        .into_client_request()
        .map_err(|e| format!("requête invalide ({e})"))?;
    req.headers_mut().insert(
        "x-goog-api-key",
        tokio_tungstenite::tungstenite::http::HeaderValue::from_str(key)
            .map_err(|e| e.to_string())?,
    );
    let (mut ws, _) = tokio::time::timeout(
        std::time::Duration::from_secs(15),
        tokio_tungstenite::connect_async(req),
    )
    .await
    .map_err(|_| {
        log_event(app, "voice", "live : connexion en timeout (15s)");
        voice_emit(app, "error", "Gemini Live : connexion en timeout");
        "Gemini Live : connexion en timeout (15s)".to_string()
    })?
    .map_err(|e| {
        log_event(app, "voice", &format!("live : connexion échouée : {e}"));
        voice_emit(app, "error", &format!("Gemini Live : connexion échouée ({e})"));
        format!("Gemini Live : connexion échouée ({e})")
    })?;
    log_event(app, "voice", "live : websocket connecté");

    // ── 1. SETUP : doit être le PREMIER message du flux ──
    let setup = serde_json::json!({
        "setup": {
            "model": "models/gemini-3.8-live",
            "generationConfig": {"responseModalities": ["AUDIO"]},
            "inputAudioTranscription": {}
        }
    });
    ws.send(WsMessage::Text(setup.to_string()))
        .await
        .map_err(|e| e.to_string())?;
    log_event(app, "voice", "live : setup envoyé");

    // ── 2. attendre setupComplete (frame TEXT ou BINARY) ──
    loop {
        let msg = tokio::time::timeout(std::time::Duration::from_secs(10), ws.next()).await;
        let msg = match msg {
            Ok(m) => m,
            Err(_) => return Err("Gemini Live : timeout setup".into()),
        };
        match msg {
            Some(Ok(WsMessage::Text(t))) => {
                log_event(app, "voice", &format!("live : TEXT {}", t.chars().take(120).collect::<String>()));
                if t.contains("setupComplete") {
                    break;
                }
            }
            Some(Ok(WsMessage::Binary(b))) => {
                let text = String::from_utf8_lossy(&b).to_string();
                log_event(app, "voice", &format!("live : BIN {}", text.chars().take(120).collect::<String>()));
                if text.contains("setupComplete") {
                    break;
                }
            }
            Some(Ok(WsMessage::Ping(p))) => {
                let _ = ws.send(WsMessage::Pong(p)).await;
            }
            Some(Ok(WsMessage::Close(c))) => {
                return Err(format!("Gemini Live : fermé pendant le setup ({c:?})"));
            }
            Some(Ok(_)) => {}
            Some(Err(e)) => return Err(e.to_string()),
            None => return Err("Gemini Live : connexion fermée (setup)".into()),
        }
    }

    // ── 3. Split : émetteur dans l'état, lecteur en tâche de fond ──
    let (tx, mut rx) = ws.split();
    {
        let mut guard = LIVE_TX.lock().await;
        *guard = Some(tx);
    }

    let reader_app = app.clone();
    tokio::spawn(async move {
        let mut audio_chunks = 0usize;
        let mut frames = 0usize;
        loop {
            let msg = match rx.next().await {
                Some(m) => m,
                None => break,
            };
            let text = match msg {
                Ok(WsMessage::Text(t)) => t.to_string(),
                Ok(WsMessage::Binary(b)) => String::from_utf8_lossy(&b).to_string(),
                Ok(WsMessage::Close(c)) => {
                    log_event(&reader_app, "voice", &format!("live : CLOSE {c:?}"));
                    voice_emit(&reader_app, "error", "connexion fermée par le serveur");
                    break;
                }
                Ok(WsMessage::Ping(p)) => {
                    log_event(&reader_app, "voice", "live : PING");
                    if let Some(tx) = LIVE_TX.lock().await.as_mut() {
                        let _ = tx.send(WsMessage::Pong(p)).await;
                    }
                    continue;
                }
                Ok(_) => continue,
                Err(e) => {
                    log_event(&reader_app, "voice", &format!("live : erreur ws : {e}"));
                    break;
                }
            };
            frames += 1;
            if frames <= 20 {
                log_event(&reader_app, "voice", &format!("live : frame {} : {}", frames, text.chars().take(120).collect::<String>()));
            }
            let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) else { continue };

            if v.get("error").is_some() {
                let detail = v["error"]["message"].as_str().unwrap_or("?").to_string();
                log_event(&reader_app, "voice", &format!("live : erreur serveur : {detail}"));
                voice_emit(&reader_app, "error", &detail);
                continue;
            }
            if let Some(parts) = v["serverContent"]["modelTurn"]["parts"].as_array() {
                for part in parts {
                    if let Some(d) = part["inlineData"]["data"].as_str() {
                        audio_chunks += 1;
                        if audio_chunks == 1 {
                            log_event(&reader_app, "voice", "live : premier chunk audio reçu ✅");
                        }
                        voice_emit(&reader_app, "audio", d);
                    }
                    if let Some(tr) = part["text"].as_str() {
                        voice_emit(&reader_app, "oz-text", tr);
                    }
                }
            }
            if v["serverContent"]["interrupted"].as_bool() == Some(true) {
                voice_emit(&reader_app, "interrupted", "");
            }
            if let Some(tr) = v["serverContent"]["inputTranscription"]["text"].as_str() {
                voice_emit(&reader_app, "user-text", tr);
            }
            if v["serverContent"]["turnComplete"].as_bool() == Some(true) {
                log_event(&reader_app, "voice", &format!("live : tour terminé ({} chunk(s) audio)", audio_chunks));
                audio_chunks = 0;
                voice_emit(&reader_app, "turn-complete", "");
            }
        }
        let mut guard = LIVE_TX.lock().await;
        *guard = None;
        MIC_PENDING.lock().await.clear();
        voice_emit(&reader_app, "disconnected", "");
    });
    Ok(())
}

/// Envoie un morceau de texte incrémental (turnComplete=false) — streaming
#[tauri::command]
pub(crate) async fn voice_feed(app: AppHandle, text: String) -> Result<(), String> {
    eprintln!("[oz] voice : voice_feed appelé ({} caractères)", text.chars().count());
    let key = gemini_key_from_state(&app).await;
    if key.is_empty() {
        return Err("Aucune clé Gemini : ajoute-la dans le panneau, onglet Modèles".into());
    }
    ensure_live_session(&app, &key).await?;
    log_event(&app, "voice", &format!("live : feed {} caractère(s) : {}", text.chars().count(), text.chars().take(60).collect::<String>()));
    let mut guard = LIVE_TX.lock().await;
    let Some(tx) = guard.as_mut() else {
        return Err("Gemini Live : session absente".into());
    };
    let msg = serde_json::json!({
        "clientContent": {"turns": [{"role": "user", "parts": [{"text": text}]}], "turnComplete": false}
    });
    tx.send(WsMessage::Text(msg.to_string()))
        .await
        .map_err(|e| e.to_string())
}

/// Termine le tour : le modèle parle, l'audio sort via le lecteur (events)
#[tauri::command]
pub(crate) async fn voice_end(app: AppHandle) -> Result<(), String> {
    let key = gemini_key_from_state(&app).await;
    if key.is_empty() {
        return Err("Aucune clé Gemini : ajoute-la dans le panneau, onglet Modèles".into());
    }
    ensure_live_session(&app, &key).await?;
    log_event(&app, "voice", "live : turnComplete envoyé");
    let mut guard = LIVE_TX.lock().await;
    let Some(tx) = guard.as_mut() else {
        return Err("Gemini Live : session absente".into());
    };
    let msg = serde_json::json!({
        "clientContent": {"turns": [], "turnComplete": true}
    });
    tx.send(WsMessage::Text(msg.to_string()))
        .await
        .map_err(|e| e.to_string())
}

// ═══════════════ Micro streaming → realtimeInput ═══════════════
// La voix de l'utilisateur part en continu vers Gemini : plus de
// "enregistrer puis attendre" — transcription et réponse quasi immédiates.

#[tauri::command]
pub(crate) async fn mic_chunk(app: AppHandle, data: String) -> Result<(), String> {
    static FIRST: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(true);
    if FIRST.swap(false, std::sync::atomic::Ordering::Relaxed) {
        log_event(&app, "voice", "premier mic_chunk reçu du frontend");
    }
    {
        let cfg = app.state::<BrainState>().config.lock().unwrap().clone();
        if cfg.api_key.is_empty() {
            return Err("Aucune clé Gemini : ajoute-la dans le panneau, onglet Modèles".into());
        }
    }
    // session prête ? → envoie direct. Sinon → bufferise (zéro mot perdu)
    {
        let guard = LIVE_TX.lock().await;
        if guard.is_none() {
            MIC_PENDING.lock().await.push(data);
            ensure_live_session(&app, &gemini_key_from_state(&app).await).await?;
            flush_mic_pending().await?;
            return Ok(());
        }
    }
    flush_mic_pending().await?; // rattrape d'éventuels chunks en attente
    let mut guard = LIVE_TX.lock().await;
    let Some(tx) = guard.as_mut() else {
        return Err("Gemini Live : session absente".into());
    };
    let msg = serde_json::json!({
        "realtimeInput": {
            "audio": {"mimeType": "audio/pcm;rate=16000", "data": data}
        }
    });
    tx.send(WsMessage::Text(msg.to_string()))
        .await
        .map_err(|e| e.to_string())
}

pub(crate) async fn gemini_key_from_state(app: &AppHandle) -> String {
    app.state::<BrainState>().config.lock().unwrap().api_key.clone()
}

#[tauri::command]
pub(crate) async fn mic_end() -> Result<(), String> {
    flush_mic_pending().await?;
    let mut guard = LIVE_TX.lock().await;
    let Some(tx) = guard.as_mut() else { return Ok(()) };
    let msg = serde_json::json!({"realtimeInput": {"audioStreamEnd": true}});
    tx.send(WsMessage::Text(msg.to_string()))
        .await
        .map_err(|e| e.to_string())
}

/// Coupe la voix en cours (Échap) : lecteur Gemini TTS ; le flux Live est
/// arrêté côté frontend (les sources audio planifiées sont stoppées).
#[tauri::command]
pub(crate) fn stop_voice() {
    if let Ok(mut guard) = TTS_CHILD.lock() {
        if let Some(old) = guard.as_mut() {
            let _ = old.kill();
            let _ = old.wait();
        }
        *guard = None;
    }
}

#[tauri::command]
pub(crate) async fn speak(
    app: AppHandle,
    text: String,
) -> Result<(), String> {
    let cfg = app.state::<BrainState>().config.lock().unwrap().clone();

    // NB : le chemin gemini-live n'utilise plus speak() — la voix est
    // streamée via voice_feed/voice_end PENDANT le streaming du chat.
    // speak() sert aux backends "fini-à-la-fin" (SAPI, gemini-tts REST).
    if cfg.audio_model.starts_with("gemini-live") {
        return Ok(()); // géré en streaming par voice_feed/voice_end
    }

    // ── Voix Gemini TTS (audio_model = "gemini-tts:Kore") ──
    if cfg.audio_model.starts_with("gemini") && !cfg.api_key.is_empty() {
        let voice = cfg.audio_model.split(':').nth(1).unwrap_or("Kore").to_string();
        let key = cfg.api_key.clone();
        let tmp = app.path().app_data_dir().unwrap().join("oz-voice.wav");

        // tue la voix en cours (interruption naturelle)
        if let Ok(mut guard) = TTS_CHILD.lock() {
            if let Some(old) = guard.as_mut() {
                let _ = old.kill();
                let _ = old.wait();
            }
            *guard = None;
        }

        let body = serde_json::json!({
            "contents": [{"parts": [{"text": text}]}],
            "generationConfig": {
                "responseModalities": ["AUDIO"],
                "speechConfig": {"voiceConfig": {"prebuiltVoiceConfig": {"voiceName": voice}}}
            }
        });
        let url = "https://generativelanguage.googleapis.com/v1beta/models/gemini-2.5-flash-preview-tts:generateContent";
        let resp = http_client()
            .post(url)
            .header("x-goog-api-key", &key)
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("Gemini TTS injoignable : {e}"))?;
        let json: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
        let Some(audio_b64) = json["candidates"][0]["content"]["parts"][0]["inlineData"]["data"]
            .as_str()
        else {
            return Err("Gemini TTS : pas d'audio dans la réponse".into());
        };
        use base64::Engine as _;
        let pcm = base64::engine::general_purpose::STANDARD
            .decode(audio_b64)
            .map_err(|e| e.to_string())?;
        let mut wav = wav_header(pcm.len() as u32, 24000);
        wav.extend_from_slice(&pcm);
        fs::write(&tmp, &wav).map_err(|e| e.to_string())?;

        // Lecture du WAV via MediaPlayer, interruptible
        let path_uri = format!(
            "file:///{}/",
            tmp.to_string_lossy().replace('\\', "/")
        );
        let script = format!(
            "Add-Type -AssemblyName presentationCore; $p = New-Object System.Windows.Media.MediaPlayer; $p.Open([Uri]::new('{}')); Start-Sleep -Milliseconds 400; $p.Play(); Start-Sleep -Seconds 40",
            path_uri
        );
        let _ = script.clone();
        let mut cmd = std::process::Command::new("powershell");
        cmd.args(["-NoProfile", "-Command", &script]);
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x08000000);
        }
        match cmd.spawn() {
            Ok(child) => {
                if let Ok(mut guard) = TTS_CHILD.lock() {
                    *guard = Some(child);
                }
                if let Ok(mut guard) = TTS_CHILD.lock() {
                    if let Some(c) = guard.as_mut() {
                        let _ = c.wait();
                    }
                    *guard = None;
                }
            }
            Err(_) => {}
        }
        return Ok(());
    }

    // ── Voix système SAPI (défaut) ──
    let tmp = app.path().app_data_dir().unwrap().join("tts.txt");
    fs::write(&tmp, &text).map_err(|e| e.to_string())?;
    let script = format!(
        "Add-Type -AssemblyName System.Speech; \
         $s = New-Object System.Speech.Synthesis.SpeechSynthesizer; \
         $v = ($s.GetInstalledVoices() | Where-Object {{ $_.VoiceInfo.Culture.Name -like 'fr*' }} | Select-Object -First 1).VoiceInfo.Name; \
         if ($v) {{ $s.SelectVoice($v) }}; \
         $s.Rate = 1; \
         $s.Speak((Get-Content -Raw -Encoding UTF8 '{}'))",
        tmp.to_str().unwrap().replace('\\', "\\\\")
    );
    std::thread::spawn(move || {
        let mut cmd = std::process::Command::new("powershell");
        cmd.args(["-NoProfile", "-Command", &script]);
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x08000000);
        }
        let _ = cmd.output();
    });
    Ok(())
}

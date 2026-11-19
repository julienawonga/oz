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
type TxSlot = tokio::sync::Mutex<Option<WsTx>>;

// ═══════════════ Deux sessions Gemini Live ═══════════════
// TALK  : la conversation à voix haute. Reçoit le micro, comprend, appelle
//         les outils (le broker reste garant des permissions) et répond
//         directement en audio. Pas d'étape texte : premier son en ~1 s.
// SPEAK : lit à voix haute les réponses du cerveau texte (messages tapés,
//         reprise après une permission accordée).
// Avant, le micro passait par une session d'écoute, puis le cerveau texte,
// puis SPEAK : 3 à 8 s avant la première syllabe.
#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) enum Live {
    Talk,
    Speak,
}

static TALK_TX: TxSlot = tokio::sync::Mutex::const_new(None);
static SPEAK_TX: TxSlot = tokio::sync::Mutex::const_new(None);
static TALK_CONNECT: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
// numéro de la session ouverte : le lecteur d'une ancienne session (fermée
// par un changement de langue) ne doit pas effacer l'émetteur de la nouvelle
static TALK_GEN: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
static SPEAK_GEN: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
static SPEAK_CONNECT: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
// poignée de reprise de la conversation : le serveur ferme les sessions
// régulièrement, on rouvre avec elle sans perdre le fil
static RESUME_HANDLE: Mutex<Option<String>> = Mutex::new(None);
// chunks micro mis en mémoire pendant l'ouverture de session (zéro mot perdu)
pub(crate) static MIC_PENDING: tokio::sync::Mutex<Vec<String>> = tokio::sync::Mutex::const_new(Vec::new());

impl Live {
    fn tx(self) -> &'static TxSlot {
        match self {
            Live::Talk => &TALK_TX,
            Live::Speak => &SPEAK_TX,
        }
    }
    fn connect_lock(self) -> &'static tokio::sync::Mutex<()> {
        match self {
            Live::Talk => &TALK_CONNECT,
            Live::Speak => &SPEAK_CONNECT,
        }
    }
    fn generation(self) -> &'static std::sync::atomic::AtomicU64 {
        match self {
            Live::Talk => &TALK_GEN,
            Live::Speak => &SPEAK_GEN,
        }
    }
    fn name(self) -> &'static str {
        match self {
            Live::Talk => "conversation",
            Live::Speak => "parole",
        }
    }
}

/// Envoie un message JSON sur une session ouverte.
async fn send_on(ch: Live, msg: serde_json::Value) -> Result<(), String> {
    let mut guard = ch.tx().lock().await;
    let Some(tx) = guard.as_mut() else {
        return Err(format!("Gemini Live ({}) : session absente", ch.name()));
    };
    tx.send(WsMessage::Text(msg.to_string()))
        .await
        .map_err(|e| e.to_string())
}

/// Vide le buffer micro vers la session de conversation (ordre préservé)
pub(crate) async fn flush_mic_pending() -> Result<(), String> {
    let mut pending = MIC_PENDING.lock().await;
    let queued: Vec<String> = std::mem::take(&mut *pending);
    let mut guard = TALK_TX.lock().await;
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

/// Voix choisie dans le panneau : « gemini-live:Kore » -> « Kore »
fn live_voice_name(app: &AppHandle) -> String {
    let cfg = app.state::<BrainState>().config.lock().unwrap().clone();
    cfg.audio_model.split(':').nth(1).unwrap_or("Kore").to_string()
}

/// Ouvre la session demandée si absente (persistante entre les tours).
pub(crate) async fn ensure_live_session(app: &AppHandle, key: &str, ch: Live) -> Result<(), String> {
    if ch.tx().lock().await.is_some() {
        return Ok(()); // session déjà ouverte
    }
    // un seul connect à la fois par canal (les chunks micro arrivent en rafale)
    let _conn = ch.connect_lock().lock().await;
    if ch.tx().lock().await.is_some() {
        return Ok(()); // un autre appel a connecté pendant qu'on attendait
    }
    let fail = |msg: String| {
        if ch == Live::Talk {
            *RESUME_HANDLE.lock().unwrap() = None; // poignée périmée : on repartira de zéro
        }
        log_event(app, "voice", &format!("live {} : {msg}", ch.name()));
        voice_emit(app, "error", &format!("Gemini Live : {msg}"));
        format!("Gemini Live : {msg}")
    };
    // Reprendre la conversation précédente : Google ferme une session restée
    // inactive, et sa poignée expire. Dans ce cas, session neuve tout de
    // suite : avant, le premier clic échouait (« session not found or
    // expired ») et il fallait recliquer.
    let resumed = ch == Live::Talk && RESUME_HANDLE.lock().unwrap().is_some();
    let ws = match open_ws(app, key, ch).await {
        Ok(ws) => ws,
        Err(e) if resumed => {
            *RESUME_HANDLE.lock().unwrap() = None;
            let short: String = e.chars().take(90).collect();
            log_event(app, "voice", &format!("live conversation : reprise impossible ({short}), nouvelle session"));
            open_ws(app, key, ch).await.map_err(fail)?
        }
        Err(e) => return Err(fail(e)),
    };
    log_event(app, "voice", &format!("live {} : session ouverte", ch.name()));

    // ── 3. Split : émetteur dans l'état, lecteur en tâche de fond ──
    let (tx, rx) = ws.split();
    let session_id = ch.generation().fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
    *ch.tx().lock().await = Some(tx);
    tokio::spawn(read_live(app.clone(), ch, rx, session_id));
    Ok(())
}

/// Connexion et setup d'une session Live (sans signaler l'erreur à l'œuf).
async fn open_ws(app: &AppHandle, key: &str, ch: Live) -> Result<WsStream, String> {
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
    // erreurs remontées telles quelles : c'est l'appelant qui décide de réessayer
    let fail = |msg: String| msg;
    let (mut ws, _) = tokio::time::timeout(
        std::time::Duration::from_secs(15),
        tokio_tungstenite::connect_async(req),
    )
    .await
    .map_err(|_| fail("connexion en timeout (15s)".into()))?
    .map_err(|e| fail(format!("connexion échouée ({e})")))?;

    // ── 1. SETUP : doit être le PREMIER message du flux ──
    let setup = match ch {
        Live::Talk => {
            let mut session = serde_json::json!({});
            if let Some(h) = RESUME_HANDLE.lock().unwrap().clone() {
                session["handle"] = serde_json::json!(h); // reprend la conversation où elle était
            }
            serde_json::json!({"setup": {
                "model": "models/gemini-3.8-live",
                "generationConfig": {
                    "responseModalities": ["AUDIO"],
                    "speechConfig": {
                        // langue imposée : sinon un bruit transcrit en chinois le faisait répondre en chinois
                        "languageCode": crate::prefs::language_code(app),
                        "voiceConfig": {"prebuiltVoiceConfig": {"voiceName": live_voice_name(app)}}
                    }
                },
                "systemInstruction": {"parts": [{"text": crate::brain::live_system_prompt(app)}]},
                "tools": crate::tools::declarations(app),
                "inputAudioTranscription": {},
                "outputAudioTranscription": {},
                // sans compression, une session audio est coupée au bout de ~15 min
                "contextWindowCompression": {"slidingWindow": {}},
                "sessionResumption": session
            }})
        }
        Live::Speak => serde_json::json!({"setup": {
            "model": "models/gemini-3.8-live",
            "generationConfig": {
                "responseModalities": ["AUDIO"],
                "speechConfig": {
                    "languageCode": crate::prefs::language_code(app),
                    "voiceConfig": {"prebuiltVoiceConfig": {"voiceName": live_voice_name(app)}}
                }
            },
            "systemInstruction": {"parts": [{"text":
                "Tu es la voix d'Oz. Tu lis à voix haute, mot pour mot, le texte qu'on te donne. Tu n'ajoutes rien, tu ne commentes rien, tu ne réponds jamais au contenu."}]}
        }}),
    };
    ws.send(WsMessage::Text(setup.to_string()))
        .await
        .map_err(|e| e.to_string())?;

    // ── 2. attendre setupComplete (frame TEXT ou BINARY) ──
    loop {
        let msg = tokio::time::timeout(std::time::Duration::from_secs(10), ws.next())
            .await
            .map_err(|_| fail("timeout du setup".into()))?;
        let text = match msg {
            Some(Ok(WsMessage::Text(t))) => t.to_string(),
            Some(Ok(WsMessage::Binary(b))) => String::from_utf8_lossy(&b).to_string(),
            Some(Ok(WsMessage::Ping(p))) => {
                let _ = ws.send(WsMessage::Pong(p)).await;
                continue;
            }
            Some(Ok(WsMessage::Close(c))) => return Err(fail(format!("fermé pendant le setup ({c:?})"))),
            Some(Ok(_)) => continue,
            Some(Err(e)) => return Err(fail(e.to_string())),
            None => return Err(fail("connexion fermée pendant le setup".into())),
        };
        if text.contains("setupComplete") {
            break;
        }
        if text.contains("\"error\"") {
            return Err(fail(text.chars().take(160).collect()));
        }
    }
    Ok(ws)
}

/// Lecteur d'une session Live : traduit les messages serveur en événements.
async fn read_live(app: AppHandle, ch: Live, mut rx: futures_util::stream::SplitStream<WsStream>, session_id: u64) {
    let mut audio_chunks = 0usize;
    // tour de conversation en cours (session TALK)
    let mut heard = String::new();
    let mut said = String::new();
    // un outil a été appelé : le tour clos juste après n'a encore rien dit
    let mut tool_pending = false;
    while let Some(msg) = rx.next().await {
        let text = match msg {
            Ok(WsMessage::Text(t)) => t.to_string(),
            Ok(WsMessage::Binary(b)) => String::from_utf8_lossy(&b).to_string(),
            Ok(WsMessage::Close(c)) => {
                log_event(&app, "voice", &format!("live {} : fermée ({c:?})", ch.name()));
                break;
            }
            Ok(WsMessage::Ping(p)) => {
                if let Some(tx) = ch.tx().lock().await.as_mut() {
                    let _ = tx.send(WsMessage::Pong(p)).await;
                }
                continue;
            }
            Ok(_) => continue,
            Err(e) => {
                log_event(&app, "voice", &format!("live {} : erreur ws : {e}", ch.name()));
                break;
            }
        };
        let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) else { continue };

        if v.get("error").is_some() {
            let detail = v["error"]["message"].as_str().unwrap_or("?").to_string();
            log_event(&app, "voice", &format!("live {} : erreur serveur : {detail}", ch.name()));
            voice_emit(&app, "error", &detail);
            continue;
        }
        let sc = &v["serverContent"];
        let turn_complete = sc["turnComplete"].as_bool() == Some(true);

        match ch {
            Live::Talk => {
                if let Some(h) = v["sessionResumptionUpdate"]["newHandle"].as_str() {
                    *RESUME_HANDLE.lock().unwrap() = Some(h.to_string());
                }
                if v.get("goAway").is_some() {
                    log_event(&app, "voice", "live conversation : le serveur va fermer, reprise à la prochaine parole");
                }
                // Outils : exécutés à part pour ne pas bloquer la lecture du flux
                if let Some(calls) = v["toolCall"]["functionCalls"].as_array() {
                    tool_pending = true;
                    let calls = calls.clone();
                    let app2 = app.clone();
                    tokio::spawn(async move {
                        let mut responses = Vec::new();
                        for call in calls {
                            let name = call["name"].as_str().unwrap_or("").to_string();
                            let result = crate::tools::run(&app2, &name, &call["args"]).await;
                            responses.push(serde_json::json!({
                                "id": call["id"], "name": name, "response": {"result": result}
                            }));
                        }
                        let msg = serde_json::json!({"toolResponse": {"functionResponses": responses}});
                        if let Err(e) = send_on(Live::Talk, msg).await {
                            log_event(&app2, "voice", &format!("live conversation : résultat d'outil non envoyé : {e}"));
                        }
                    });
                    continue;
                }
                if let Some(tr) = sc["inputTranscription"]["text"].as_str() {
                    heard.push_str(tr);
                    voice_emit(&app, "user-text", tr);
                }
                if v["voiceActivity"]["type"].as_str() == Some("ACTIVITY_END") {
                    log_event(&app, "voice", "live conversation : fin de phrase de %OWNER%");
                }
                if let Some(tr) = sc["outputTranscription"]["text"].as_str() {
                    said.push_str(tr);
                    voice_emit(&app, "talk-text", tr);
                }
                if let Some(parts) = sc["modelTurn"]["parts"].as_array() {
                    for part in parts {
                        if let Some(d) = part["inlineData"]["data"].as_str() {
                            audio_chunks += 1;
                            if audio_chunks == 1 {
                                log_event(&app, "voice", "live conversation : premier son ✅");
                            }
                            voice_emit(&app, "talk-audio", d);
                        }
                    }
                }
                if sc["interrupted"].as_bool() == Some(true) {
                    voice_emit(&app, "interrupted", "");
                }
                // Tour clos sans parole : juste après un appel d'outil, la réponse
                // arrive dans le tour suivant (on garde ce que l'utilisateur a dit) ;
                // sinon c'était du bruit ou un outil sans réponse : on oublie,
                // pour ne pas coller ces mots à l'échange suivant.
                if turn_complete && said.trim().is_empty() {
                    if tool_pending {
                        tool_pending = false;
                    } else {
                        heard.clear();
                        audio_chunks = 0;
                    }
                }
                if turn_complete && !said.trim().is_empty() {
                    tool_pending = false;
                    log_event(&app, "voice", &format!("live conversation : tour terminé ({audio_chunks} chunk(s) audio)"));
                    let (user, oz) = (heard.trim().to_string(), said.trim().to_string());
                    // le texte de l'échange, relisible dans le journal
                    log_event(&app, "conversation", &format!("%OWNER% : « {user} » — Oz : « {oz} »"));
                    crate::brain::remember_exchange(&app, &user, &oz);
                    let _ = app.emit("oz://voice-talk-turn", serde_json::json!({"user": user, "oz": oz}));
                    heard.clear();
                    said.clear();
                    audio_chunks = 0;
                }
            }
            Live::Speak => {
                if let Some(parts) = sc["modelTurn"]["parts"].as_array() {
                    for part in parts {
                        if let Some(d) = part["inlineData"]["data"].as_str() {
                            audio_chunks += 1;
                            if audio_chunks == 1 {
                                log_event(&app, "voice", "live parole : premier chunk audio ✅");
                            }
                            voice_emit(&app, "audio", d);
                        }
                    }
                }
                if sc["interrupted"].as_bool() == Some(true) {
                    voice_emit(&app, "interrupted", "");
                }
                if turn_complete {
                    log_event(&app, "voice", &format!("live parole : tour terminé ({audio_chunks} chunk(s) audio)"));
                    audio_chunks = 0;
                    voice_emit(&app, "turn-complete", "");
                }
            }
        }
    }
    if ch.generation().load(std::sync::atomic::Ordering::SeqCst) != session_id {
        return; // une session plus récente a pris la place : on n'y touche pas
    }
    *ch.tx().lock().await = None;
    // Les deux sessions se rouvrent seules au besoin : la conversation au
    // prochain morceau de micro (avec la poignée de reprise, sans perdre le
    // fil), la parole à la prochaine réponse tapée.
    if ch == Live::Talk {
        MIC_PENDING.lock().await.clear();
    }
}

/// Envoie un morceau de texte à lire (turnComplete=false) — streaming
#[tauri::command]
pub(crate) async fn voice_feed(app: AppHandle, text: String) -> Result<(), String> {
    let key = gemini_key_from_state(&app).await;
    if key.is_empty() {
        return Err("Aucune clé Gemini : ajoute-la dans le panneau, onglet Modèles".into());
    }
    ensure_live_session(&app, &key, Live::Speak).await?;
    log_event(&app, "voice", &format!("live parole : feed {} caractère(s)", text.chars().count()));
    send_on(Live::Speak, serde_json::json!({
        "clientContent": {"turns": [{"role": "user", "parts": [{"text": text}]}], "turnComplete": false}
    }))
    .await
}

/// Termine le tour : le modèle lit, l'audio sort via le lecteur (events)
#[tauri::command]
pub(crate) async fn voice_end(app: AppHandle) -> Result<(), String> {
    let key = gemini_key_from_state(&app).await;
    if key.is_empty() {
        return Err("Aucune clé Gemini : ajoute-la dans le panneau, onglet Modèles".into());
    }
    ensure_live_session(&app, &key, Live::Speak).await?;
    send_on(Live::Speak, serde_json::json!({"clientContent": {"turns": [], "turnComplete": true}})).await
}

// ═══════════════ Micro streaming → session d'écoute ═══════════════

#[tauri::command]
pub(crate) async fn mic_chunk(app: AppHandle, data: String) -> Result<(), String> {
    LAST_MIC_CHUNK.store(crate::journal::now_ts(), std::sync::atomic::Ordering::Relaxed);
    let key = gemini_key_from_state(&app).await;
    if key.is_empty() {
        return Err("Aucune clé Gemini : ajoute-la dans le panneau, onglet Modèles".into());
    }
    // session prête ? -> envoie direct. Sinon -> bufferise (zéro mot perdu).
    // Le verrou est relâché AVANT ensure_live_session, qui le reprend.
    let session_open = TALK_TX.lock().await.is_some();
    if !session_open {
        MIC_PENDING.lock().await.push(data);
        ensure_live_session(&app, &key, Live::Talk).await?;
        return flush_mic_pending().await;
    }
    flush_mic_pending().await?; // rattrape d'éventuels chunks en attente
    send_on(Live::Talk, serde_json::json!({
        "realtimeInput": {"audio": {"mimeType": "audio/pcm;rate=16000", "data": data}}
    }))
    .await
}

/// Dernier morceau de micro reçu (secondes) : la vue en direct n'envoie
/// des images que pendant une conversation.
static LAST_MIC_CHUNK: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// L'utilisateur parle à Oz en ce moment (micro actif dans les `secs` dernières
/// secondes ; il est muet pendant qu'Oz parle, d'où une marge).
pub(crate) fn conversing_within(secs: u64) -> bool {
    crate::journal::now_ts().saturating_sub(LAST_MIC_CHUNK.load(std::sync::atomic::Ordering::Relaxed)) <= secs
}

/// Envoie une image (JPEG base64) à la conversation vocale, si elle est
/// ouverte. N'ouvre jamais de session : pas de conversation, pas d'image.
pub(crate) async fn send_video_frame(b64: String) -> Result<(), String> {
    if TALK_TX.lock().await.is_none() {
        return Ok(());
    }
    send_on(Live::Talk, serde_json::json!({
        "realtimeInput": {"video": {"mimeType": "image/jpeg", "data": b64}}
    }))
    .await
}

/// Ferme la conversation vocale et oublie sa poignée de reprise : la
/// prochaine session repart avec les réglages du moment (langue).
pub(crate) async fn reset_talk_session(app: &AppHandle) {
    if let Some(mut tx) = TALK_TX.lock().await.take() {
        let _ = tx.send(WsMessage::Close(None)).await;
    }
    *RESUME_HANDLE.lock().unwrap() = None;
    MIC_PENDING.lock().await.clear();
    log_event(app, "voice", "live conversation : réinitialisée (réglages modifiés)");
}

/// Consigne texte glissée dans la conversation vocale (initiative d'Oz) :
/// le modèle y répond à voix haute, comme à une phrase de l'utilisateur.
#[tauri::command]
pub(crate) async fn live_prompt(app: AppHandle, text: String) -> Result<(), String> {
    let text = crate::prefs::personalize(&app, &text);
    let key = gemini_key_from_state(&app).await;
    if key.is_empty() {
        return Err("Aucune clé Gemini : ajoute-la dans le panneau, onglet Modèles".into());
    }
    ensure_live_session(&app, &key, Live::Talk).await?;
    send_on(Live::Talk, serde_json::json!({
        "clientContent": {"turns": [{"role": "user", "parts": [{"text": text}]}], "turnComplete": true}
    }))
    .await
}

pub(crate) async fn gemini_key_from_state(app: &AppHandle) -> String {
    app.state::<BrainState>().config.lock().unwrap().api_key.clone()
}

#[tauri::command]
pub(crate) async fn mic_end() -> Result<(), String> {
    flush_mic_pending().await?;
    if TALK_TX.lock().await.is_none() {
        return Ok(());
    }
    send_on(Live::Talk, serde_json::json!({"realtimeInput": {"audioStreamEnd": true}})).await
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

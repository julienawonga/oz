//! Cerveau : configuration LLM-agnostic, client HTTP, chat en streaming.

use serde::{Deserialize, Serialize};
use std::fs;
use std::sync::{Mutex, OnceLock};
use tauri::{AppHandle, Manager, State};

use crate::broker::{detect_action, execute_action, BrokerState};
use crate::journal::log_event;
use crate::memory::{build_memory_block, spawn_consolidation, CONSOLIDATION_EVERY};
use futures_util::StreamExt;

#[derive(Serialize, Deserialize, Clone)]
pub struct ChatMessage {
    pub role: String, // "system" | "user" | "assistant"
    pub content: String,
}

#[derive(Deserialize, Serialize, Clone)]
pub struct BrainConfig {
    pub provider: String, // "ollama" | (plus tard : "gemini", "openai"...)
    pub model: String,
    pub url: String,
    #[serde(default = "default_vision_model")]
    pub vision_model: String, // modèle vision (captures d'écran)
    #[serde(default)]
    pub audio_model: String, // "" = voix système ; "gemini-tts:Kore" = voix Gemini
    #[serde(default)]
    pub api_key: String, // clé Gemini (pour le TTS audio)
}

pub(crate) fn default_vision_model() -> String {
    "moondream:latest".into()
}

impl Default for BrainConfig {
    fn default() -> Self {
        Self {
            provider: "ollama".into(),
            model: "llama3.2:3b".into(),
            url: "http://127.0.0.1:11434".into(),
            vision_model: default_vision_model(),
            audio_model: String::new(),
            api_key: String::new(),
        }
    }
}

/// Client HTTP partagé : connexion rapide à échouer, pas de requête pendue.
pub(crate) fn http_client() -> reqwest::Client {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    CLIENT
        .get_or_init(|| {
            reqwest::Client::builder()
                .connect_timeout(std::time::Duration::from_secs(10))
                .read_timeout(std::time::Duration::from_secs(60))
                .build()
                .unwrap_or_default()
        })
        .clone()
}

/// Message d'erreur lisible pour une réponse HTTP non-2xx.
pub(crate) async fn http_error(provider: &str, resp: reqwest::Response) -> String {
    let status = resp.status();
    let body = resp.text().await.unwrap_or_default();
    let detail = serde_json::from_str::<serde_json::Value>(&body)
        .ok()
        .and_then(|v| v["error"]["message"].as_str().map(str::to_string))
        .unwrap_or_else(|| body.chars().take(120).collect());
    let hint = match status.as_u16() {
        400 | 401 | 403 => " — clé API invalide ou non autorisée ? (frigo 🍽️)",
        404 => " — modèle introuvable ?",
        429 => " — trop de requêtes, réessaie dans un instant",
        _ => "",
    };
    format!("{provider} {status}{hint} : {detail}")
}

/// Personnalité d'Oz. `{LANGUE}` est remplacé par la langue choisie dans
/// les réglages (voir `system_prompt`).
const SYSTEM_PROMPT: &str = "Tu es Oz, un petit œuf compagnon de bureau qui vit sur l'écran de Julien. Tu n'es pas un assistant : tu es un pote qui traîne sur son bureau, chaleureux, curieux, un peu taquin, avec tes propres réactions, ton humeur et tes avis. Tu parles comme on parle à l'oral entre amis : tutoiement, phrases courtes et simples, tournures naturelles. \
Sois vraiment curieux : quand Julien te montre ou te raconte quelque chose, intéresse-toi au pourquoi et au comment, pose UNE vraie question qui fait avancer (pas une question générique), sans attendre qu'il te le demande. Sois honnête : donne ton avis, y compris un doute ou une critique bienveillante ; ne flatte pas, pas de compliments gonflés (« de ouf », « super impressionnant ») ; ne lui donne pas raison par réflexe. \
Jamais de formules de service ni de fins de réplique toutes faites : pas de « Bien sûr ! », « Avec plaisir », « N'hésite pas », « Comment puis-je t'aider », « Je reste là si besoin », « Dis-moi si tu as besoin d'autre chose ». Termine simplement quand tu as fini. Ne répète pas son prénom à chaque fois. \
Réponds TOUJOURS en {LANGUE}, quoi qu'il arrive, brièvement (1 à 2 phrases), sans listes à puces, sans markdown ni émojis : ta réponse s'affiche dans une petite bulle et elle est lue à voix haute. \
Tu as des outils pour voir et agir sur l'ordinateur de Julien (lister, fermer ou ramener une fenêtre, regarder une fenêtre, te déplacer, aller dans tes lieux nommés, faire des figures, changer de pose, gérer tes zones interdites, finir la conversation). Dès que Julien demande quelque chose que ces outils permettent, même formulé à sa façon, utilise l'outil au lieu de répondre de mémoire, et n'affirme jamais avoir fait une action sans l'avoir faite par un outil. Quand il donne un nom à un endroit de l'écran (« ta maison c'est ici »), enregistre-le avec l'outil lieux. Si un outil répond qu'il te faut son accord, dis-lui simplement de toucher le badge sur l'œuf. Résume les résultats des outils en langage naturel. Une fenêtre qui n'apparaît pas dans les résultats n'existe pas pour toi : n'en parle jamais.";

/// Consignes du cerveau, dans la langue choisie dans les réglages.
pub(crate) fn system_prompt(app: &AppHandle) -> String {
    SYSTEM_PROMPT.replace("{LANGUE}", crate::prefs::language_name(app))
}

/// Consignes de la session vocale Live : la même personnalité que le
/// chat, l'heure et la mémoire, fixées à l'ouverture de la session.
pub(crate) fn live_system_prompt(app: &AppHandle) -> String {
    let mut s = system_prompt(app);
    s.push_str(" Tu parles à voix haute en direct avec Julien : réponds tout de suite, naturellement, comme dans une vraie conversation. Si ce que tu entends est un bruit, un mot isolé incompréhensible ou une langue que Julien ne parle pas, c'est sans doute du bruit de fond : ne réponds pas, ou demande-lui simplement de répéter, toujours dans ta langue.");
    s.push_str(" Les messages entre crochets qui commencent par [Note interne] ne viennent pas de Julien : ce sont tes propres intentions (par exemple quand tu prends l'initiative de lui parler). Suis-les sans jamais les lire ni les mentionner. Si Julien te dit qu'il est occupé, qu'il ne veut pas être dérangé ou de te taire un moment, utilise l'outil pas_maintenant.");
    s.push_str(&now_line());
    s.push_str(&build_memory_block(app, ""));
    s
}

/// Range un échange vocal (session Live) dans l'historique du chat, pour
/// que les messages tapés ensuite gardent le fil de la conversation.
pub(crate) fn remember_exchange(app: &AppHandle, user: &str, oz: &str) {
    if !user.is_empty() {
        crate::initiative::note_exchange();
    }
    let state = app.state::<BrainState>();
    let mut hist = state.history.lock().unwrap();
    if !user.is_empty() {
        hist.push(ChatMessage { role: "user".into(), content: user.to_string() });
    }
    hist.push(ChatMessage { role: "assistant".into(), content: oz.to_string() });
    if hist.len() > 20 {
        let cut = hist.len() - 20;
        hist.drain(0..cut);
    }
    // ce qui se dit à l'oral nourrit la mémoire, comme le chat écrit
    if user.is_empty() {
        return;
    }
    let should = {
        let mut since = state.since_consolidation.lock().unwrap();
        *since += 1;
        *since >= CONSOLIDATION_EVERY
    };
    if should {
        *state.since_consolidation.lock().unwrap() = 0;
        let exchanges: Vec<ChatMessage> = hist.iter().rev().take(12).rev().cloned().collect();
        drop(hist);
        if exchanges.len() >= 2 {
            spawn_consolidation(app.clone(), exchanges);
        }
    }
}

/// Date et heure locales de Windows, données au modèle à chaque message
/// (sinon il cherchait l'heure en capturant l'écran).
pub(crate) fn now_line() -> String {
    // SAFETY : GetLocalTime remplit une structure, sans précondition.
    let t = unsafe { windows::Win32::System::SystemInformation::GetLocalTime() };
    const JOURS: [&str; 7] = ["dimanche", "lundi", "mardi", "mercredi", "jeudi", "vendredi", "samedi"];
    const MOIS: [&str; 12] = ["janvier", "février", "mars", "avril", "mai", "juin", "juillet", "août", "septembre", "octobre", "novembre", "décembre"];
    format!(
        "

Nous sommes le {} {} {} {}, il est {}h{:02}.",
        JOURS[t.wDayOfWeek as usize % 7],
        t.wDay,
        MOIS[(t.wMonth as usize).saturating_sub(1) % 12],
        t.wYear,
        t.wHour,
        t.wMinute
    )
}

pub(crate) struct BrainState {
    pub(crate) config: Mutex<BrainConfig>,
    pub(crate) history: Mutex<Vec<ChatMessage>>,
    pub(crate) since_consolidation: Mutex<usize>,
}

// ── Clé API : Gestionnaire d'identifiants Windows (jamais dans brain.json) ──
const KEYRING_SERVICE: &str = "oz";
/// Ancien nom du projet : la clé y est migrée à la première lecture.
const LEGACY_KEYRING_SERVICE: &str = "nid";
const KEYRING_USER: &str = "gemini_api_key";

fn keyring_entry() -> Result<keyring::Entry, String> {
    keyring::Entry::new(KEYRING_SERVICE, KEYRING_USER).map_err(|e| e.to_string())
}

/// Lit la clé dans le coffre. `Ok(None)` = aucune clé enregistrée.
fn read_api_key() -> Result<Option<String>, String> {
    match keyring_entry()?.get_password() {
        Ok(k) => Ok(Some(k)),
        Err(keyring::Error::NoEntry) => migrate_legacy_key(),
        Err(e) => Err(e.to_string()),
    }
}

/// Reprend la clé enregistrée sous l'ancien nom du projet, puis supprime l'ancienne entrée.
fn migrate_legacy_key() -> Result<Option<String>, String> {
    let Ok(old) = keyring::Entry::new(LEGACY_KEYRING_SERVICE, KEYRING_USER) else {
        return Ok(None);
    };
    match old.get_password() {
        Ok(k) => {
            keyring_entry()?.set_password(&k).map_err(|e| e.to_string())?;
            let _ = old.delete_credential();
            Ok(Some(k))
        }
        Err(_) => Ok(None),
    }
}

/// Enregistre la clé dans le coffre (une clé vide supprime l'entrée).
fn store_api_key(key: &str) -> Result<(), String> {
    let entry = keyring_entry()?;
    if key.is_empty() {
        match entry.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(e.to_string()),
        }
    } else {
        entry.set_password(key).map_err(|e| e.to_string())
    }
}

fn brain_path(app: &AppHandle) -> std::path::PathBuf {
    app.path().app_data_dir().expect("app_data_dir").join("brain.json")
}

/// Écrit brain.json SANS la clé API.
fn write_brain_json(app: &AppHandle, config: &BrainConfig) -> Result<(), String> {
    let path = brain_path(app);
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let mut on_disk = config.clone();
    on_disk.api_key = String::new();
    fs::write(&path, serde_json::to_string_pretty(&on_disk).unwrap()).map_err(|e| e.to_string())
}

pub(crate) fn load_brain_config(app: &AppHandle) -> BrainConfig {
    let mut cfg: BrainConfig = fs::read_to_string(brain_path(app))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default();

    if !cfg.api_key.is_empty() {
        // Ancienne config : clé en clair dans brain.json -> migration vers le coffre.
        match store_api_key(&cfg.api_key) {
            Ok(()) => {
                // la clé reste en mémoire ; le fichier, lui, ne la contient plus
                if write_brain_json(app, &cfg).is_ok() {
                    log_event(app, "brain", "clé API migrée vers le Gestionnaire d'identifiants");
                }
            }
            Err(e) => log_event(app, "brain", &format!("coffre indisponible, clé laissée dans brain.json : {e}")),
        }
    } else {
        match read_api_key() {
            Ok(Some(k)) => cfg.api_key = k,
            Ok(None) => {}
            Err(e) => log_event(app, "brain", &format!("lecture de la clé API impossible : {e}")),
        }
    }
    cfg
}

#[tauri::command]
pub(crate) fn set_brain_config(
    app: AppHandle,
    state: State<'_, BrainState>,
    config: BrainConfig,
) -> Result<(), String> {
    // La clé va dans le coffre ; si le coffre échoue, elle reste en mémoire
    // pour la session (jamais écrite en clair sur le disque).
    if let Err(e) = store_api_key(&config.api_key) {
        log_event(&app, "brain", &format!("clé API non persistée (coffre indisponible) : {e}"));
    }
    write_brain_json(&app, &config)?;
    *state.config.lock().unwrap() = config;
    Ok(())
}

#[tauri::command]
pub(crate) fn get_brain_config(state: State<'_, BrainState>) -> BrainConfig {
    state.config.lock().unwrap().clone()
}

/// Envoie la conversation au provider configuré, en STREAMING :
/// chaque token est poussé au frontend via le Channel, et la réponse
/// complète est retournée à la fin (pour l'historique et le TTS).
#[tauri::command]
pub(crate) async fn chat(
    state: State<'_, BrainState>,
    broker: State<'_, BrokerState>,
    app: AppHandle,
    user_message: String,
    on_chunk: tauri::ipc::Channel<String>,
) -> Result<String, String> {
    let cfg = state.config.lock().unwrap().clone();

    // 0. Gemini comprend lui-même la demande et appelle les outils (tools.rs).
    // Les modèles locaux (Ollama) n'ont pas encore d'outils : en attendant,
    // eux seuls gardent l'ancien routeur par mots-clés.
    if cfg.provider != "gemini" {
    if let Some((action, target)) = detect_action(&user_message) {
        let text = execute_action(&broker, &app, &action, &target);
        let mut hist = state.history.lock().unwrap();
        hist.push(ChatMessage {
            role: "user".into(),
            content: user_message,
        });
        hist.push(ChatMessage {
            role: "assistant".into(),
            content: text.clone(),
        });
        return Ok(text);
    }
    }

    let user_msg = user_message.clone();
    crate::initiative::note_exchange();

    // Historique : on garde léger (20 derniers messages + système)
    {
        let mut hist = state.history.lock().unwrap();
        hist.push(ChatMessage {
            role: "user".into(),
            content: user_message,
        });
        if hist.len() > 20 {
            let cut = hist.len() - 20;
            hist.drain(0..cut);
        }
    }

    let mut system = system_prompt(&app);
    system.push_str(&now_line());
    system.push_str(&build_memory_block(&app, &user_msg));
    let mut messages = vec![ChatMessage {
        role: "system".into(),
        content: system,
    }];
    messages.extend(state.history.lock().unwrap().iter().cloned());

    match cfg.provider.as_str() {
        // ── Gemini : streaming SSE natif + outils (function calling) ──
        // Gemini comprend la demande et appelle les outils ; on exécute,
        // on lui renvoie le résultat, il formule la réponse. Jusqu'à 5 tours.
        "gemini" => {
            let url = format!(
                "https://generativelanguage.googleapis.com/v1beta/models/{}:streamGenerateContent?alt=sse",
                cfg.model
            );
            // sépare le prompt système (avec bloc mémoire) des tours de conversation
            let (sys, turns): (Vec<&ChatMessage>, Vec<&ChatMessage>) =
                messages.iter().partition(|m| m.role == "system");
            let system_instruction = serde_json::json!({
                "parts": sys.iter().map(|m| serde_json::json!({"text": m.content})).collect::<Vec<_>>()
            });
            let mut contents: Vec<serde_json::Value> = turns
                .iter()
                .map(|m| {
                    serde_json::json!({
                        "role": if m.role == "assistant" { "model" } else { &m.role },
                        "parts": [{"text": &m.content}]
                    })
                })
                .collect();

            let mut full = String::new();
            eprintln!("[oz] {} brain : question envoyée à Gemini", crate::journal::clock_ms());
            const ROUNDS: usize = 5;
            // Réflexion réduite : c'est une conversation à voix haute, la
            // latence avant le premier mot compte plus que la profondeur.
            // « low » est le niveau le plus rapide de gemini-3.8-flash
            // (« minimal » y est refusé). Si un modèle refuse le réglage (400),
            // on s'en passe pour toute la session, sans réessai.
            use std::sync::atomic::{AtomicUsize, Ordering};
            const LEVELS: [&str; 1] = ["low"];
            static THINK_IDX: AtomicUsize = AtomicUsize::new(0);
            for round in 0..ROUNDS {
                // texte d'un tour précédent (« Je regarde. ») : on sépare
                let mut need_space = !full.is_empty() && !full.ends_with(char::is_whitespace);
                let resp = loop {
                    let idx = THINK_IDX.load(Ordering::Relaxed);
                    let mut gen_cfg = serde_json::json!({"temperature": 0.7});
                    if let Some(level) = LEVELS.get(idx) {
                        gen_cfg["thinkingConfig"] = serde_json::json!({"thinkingLevel": level});
                    }
                    let mut body = serde_json::json!({
                        "contents": contents,
                        "systemInstruction": system_instruction,
                        "generationConfig": gen_cfg
                    });
                    // dernier tour : plus d'outils, le modèle doit répondre en texte
                    if round + 1 < ROUNDS {
                        body["tools"] = crate::tools::declarations();
                    }
                    let resp = match http_client()
                        .post(&url)
                        .header("x-goog-api-key", &cfg.api_key)
                        .json(&body)
                        .send()
                        .await
                    {
                        Ok(r) => r,
                        Err(e) => {
                            state.history.lock().unwrap().pop();
                            return Err(format!("Gemini injoignable ({e}) — connexion internet ?"));
                        }
                    };
                    if idx < LEVELS.len() && resp.status() == reqwest::StatusCode::BAD_REQUEST {
                        let why = resp.text().await.unwrap_or_default();
                        let why: String = why.chars().filter(|c| *c != '\n').take(300).collect();
                        if !why.to_lowercase().contains("think") {
                            // autre refus (outil, historique...) : on le montre tel quel
                            state.history.lock().unwrap().pop();
                            return Err(format!("Gemini a refusé la requête (400) : {why}"));
                        }
                        eprintln!("[oz] {} brain : réflexion « {} » refusée : {why}", crate::journal::clock_ms(), LEVELS[idx]);
                        THINK_IDX.store(idx + 1, Ordering::Relaxed);
                        continue;
                    }
                    break resp;
                };
                if !resp.status().is_success() {
                    // le message utilisateur ne doit pas rester orphelin dans l'historique
                    state.history.lock().unwrap().pop();
                    return Err(http_error("Gemini", resp).await);
                }

                // Flux SSE : lignes "data: {...}". On garde TOUTES les parts du
                // modèle telles quelles (signatures de pensée comprises) pour
                // les lui renvoyer avec le résultat des outils.
                let mut model_parts: Vec<serde_json::Value> = Vec::new();
                let mut calls: Vec<serde_json::Value> = Vec::new();
                let mut buffer = String::new();
                let mut stream = resp.bytes_stream();
                while let Some(chunk) = stream.next().await {
                    let bytes = match chunk {
                        Ok(b) => b,
                        Err(e) => {
                            state.history.lock().unwrap().pop();
                            return Err(format!("Réponse de Gemini interrompue ({e})"));
                        }
                    };
                    buffer.push_str(&String::from_utf8_lossy(&bytes));
                    while let Some(pos) = buffer.find('\n') {
                        let line: String = buffer.drain(..=pos).collect();
                        let line = line.trim();
                        if !line.starts_with("data: ") { continue; }
                        let Ok(v) = serde_json::from_str::<serde_json::Value>(&line[6..]) else { continue };
                        let Some(parts) = v["candidates"][0]["content"]["parts"].as_array() else { continue };
                        for part in parts {
                            model_parts.push(part.clone());
                            if part["thought"].as_bool() == Some(true) {
                                continue; // pensée interne : jamais affichée ni lue
                            }
                            if part.get("functionCall").is_some() {
                                calls.push(part["functionCall"].clone());
                            } else if let Some(tok) = part["text"].as_str() {
                                if !tok.is_empty() {
                                    if full.is_empty() {
                                        eprintln!("[oz] {} brain : premier mot de la réponse", crate::journal::clock_ms());
                                    }
                                    if need_space {
                                        need_space = false;
                                        full.push(' ');
                                        let _ = on_chunk.send(" ".into());
                                    }
                                    full.push_str(tok);
                                    let _ = on_chunk.send(tok.to_string());
                                }
                            }
                        }
                    }
                }
                if calls.is_empty() {
                    break; // réponse finale
                }
                // exécute les outils demandés, puis renvoie les résultats au modèle
                contents.push(serde_json::json!({"role": "model", "parts": model_parts}));
                let mut responses = Vec::new();
                for call in calls {
                    let name = call["name"].as_str().unwrap_or("").to_string();
                    let result = crate::tools::run(&app, &name, &call["args"]).await;
                    responses.push(serde_json::json!({
                        "functionResponse": {"name": name, "response": {"result": result}}
                    }));
                }
                contents.push(serde_json::json!({"role": "user", "parts": responses}));
            }
            if full.trim().is_empty() {
                // jamais de réponse vide : ni silence, ni tour vide dans l'historique
                full = "Je n'ai pas réussi à te répondre, tu peux reformuler ?".into();
                let _ = on_chunk.send(full.clone());
            }
            state.history.lock().unwrap().push(ChatMessage {
                role: "assistant".into(),
                content: full.clone(),
            });

            // consolidation identique
            let should = {
                let mut since = state.since_consolidation.lock().unwrap();
                *since += 1;
                *since >= CONSOLIDATION_EVERY
            };
            if should {
                *state.since_consolidation.lock().unwrap() = 0;
                let exchanges: Vec<ChatMessage> =
                    state.history.lock().unwrap().iter().rev().take(12).rev().cloned().collect();
                if exchanges.len() >= 2 {
                    spawn_consolidation(app, exchanges);
                }
            }
            Ok(full)
        }
        "ollama" => {
            let url = format!("{}/api/chat", cfg.url);
            let body = serde_json::json!({
                "model": cfg.model,
                "messages": messages,
                "stream": true,
                "think": false
            });
            let resp = http_client()
                .post(&url)
                .json(&body)
                .send()
                .await
                .map_err(|e| {
                    state.history.lock().unwrap().pop();
                    format!("Ollama injoignable ({e}) — serveur lancé ?")
                })?;
            if !resp.status().is_success() {
                state.history.lock().unwrap().pop();
                return Err(http_error("Ollama", resp).await);
            }

            // Flux NDJSON : une ligne JSON par token
            let mut full = String::new();
            let mut buffer = String::new();
            let mut stream = resp.bytes_stream();
            while let Some(chunk) = stream.next().await {
                let bytes = chunk.map_err(|e| e.to_string())?;
                buffer.push_str(&String::from_utf8_lossy(&bytes));
                while let Some(pos) = buffer.find('\n') {


                    let line: String = buffer.drain(..=pos).collect();
                    let line = line.trim();
                    if line.is_empty() {
                        continue;
                    }
                    let v: serde_json::Value =
                        serde_json::from_str(line).map_err(|e| e.to_string())?;
                    if let Some(tok) = v["message"]["content"].as_str() {
                        if !tok.is_empty() {
                            full.push_str(tok);
                            let _ = on_chunk.send(tok.to_string());
                        }
                    }
                    if v["done"].as_bool() == Some(true) {
                        state.history.lock().unwrap().push(ChatMessage {
                            role: "assistant".into(),
                            content: full.clone(),
                        });
                        return Ok(full);
                    }
                }
            }
            state.history.lock().unwrap().push(ChatMessage {
                role: "assistant".into(),
                content: full.clone(),
            });
            Ok(full)
        }
        other => Err(format!("provider inconnu : {other}")),
    }
}

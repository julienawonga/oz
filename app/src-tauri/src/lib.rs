// ── Mini-Julien · Broker de permissions + Harnais ───────────────────
// Le cœur du contrat de confiance :
//  - énumère les fenêtres visibles de l'OS (API Win32)
//  - stocke les permissions par fenêtre : sanctuary | none | look | act | trust
//  - exécute les actions (close/focus/list) APRÈS vérification broker
//  - cerveau LLM-agnostic (Ollama local par défaut)
//  - voix : whisper.cpp (STT local) + voix système (TTS)
// Les permissions s'appliquent DANS LE CODE, pas dans le prompt :
// un LLM ne peut jamais capturer ou fermer une fenêtre sanctuaire.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::sync::{Mutex, OnceLock};
use tauri::{AppHandle, Emitter, Manager, State};
use windows::core::{BOOL, PWSTR};
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM, POINT};
use windows::Win32::System::Threading::{
    OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32,
    PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetCursorPos, GetForegroundWindow, GetWindowTextLengthW, GetWindowTextW,
    GetWindowThreadProcessId, GetWindowRect, IsWindowVisible,
};
use windows::Win32::Storage::Xps::{PrintWindow, PRINT_WINDOW_FLAGS};
const PW_RENDERFULLCONTENT: PRINT_WINDOW_FLAGS = PRINT_WINDOW_FLAGS(2u32);

// ═══════════════════════ Énumération des fenêtres ═══════════════════════

#[derive(Serialize, Clone)]
pub struct AppWindow {
    title: String,
    process: String,
}

unsafe fn window_title(hwnd: HWND) -> String {
    let len = GetWindowTextLengthW(hwnd);
    if len <= 0 {
        return String::new();
    }
    let mut buf = vec![0u16; len as usize + 1];
    GetWindowTextW(hwnd, &mut buf);
    String::from_utf16_lossy(&buf[..len as usize])
}

unsafe fn process_image_name(pid: u32) -> String {
    let Ok(handle) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) else {
        return "?".into();
    };
    let mut buf = vec![0u16; 512];
    let mut size = buf.len() as u32;
    let ok = QueryFullProcessImageNameW(handle, PROCESS_NAME_WIN32, PWSTR(buf.as_mut_ptr()), &mut size);
    if ok.is_err() {
        return "?".into();
    }
    let full = String::from_utf16_lossy(&buf[..size as usize]);
    full.rsplit(['\\', '/']).next().unwrap_or("?").to_string()
}

unsafe extern "system" fn enum_callback(hwnd: HWND, lparam: LPARAM) -> BOOL {
    let list = &mut *(lparam.0 as *mut Vec<WindowHandle>);
    if IsWindowVisible(hwnd).as_bool() {
        let title = window_title(hwnd);
        if !title.is_empty() {
            let mut pid = 0u32;
            GetWindowThreadProcessId(hwnd, Some(&mut pid));
            let process = process_image_name(pid);
            // On ne s'énumère pas soi-même
            if process != "nid.exe" && process != "Nid.exe" {
                list.push(WindowHandle {
                    hwnd: hwnd.0 as isize,
                    title,
                    process,
                });
            }
        }
    }
    true.into()
}

#[derive(Clone)]
pub struct WindowHandle {
    hwnd: isize,
    pub title: String,
    pub process: String,
}

impl WindowHandle {
    pub fn key(&self) -> String {
        format!("{}::{}", self.process, self.title)
    }
}

pub fn enumerate_windows() -> Vec<WindowHandle> {
    let mut list: Vec<WindowHandle> = Vec::new();
    unsafe {
        let rc = EnumWindows(
            Some(enum_callback),
            LPARAM(&mut list as *mut Vec<WindowHandle> as isize),
        );
        if rc.is_err() {
            return Vec::new();
        }
    }
    list.sort_by(|a, b| (&a.process, &a.title).cmp(&(&b.process, &b.title)));
    list.dedup_by(|a, b| a.process == b.process && a.title == b.title);
    // Filtre les hôtes système invisibles pour l'utilisateur (bruit Windows)
    const NOISE: &[&str] = &[
        "applicationframehost", "textinputhost", "searchhost",
        "startmenuexperiencehost", "shellexperiencehost", "msedgewebview2",
        "runtimebroker", "svchost", "conhost", "dllhost",
    ];
    list.retain(|w| {
        let base = w.process.trim_end_matches(".exe").to_lowercase();
        if NOISE.contains(&base.as_str()) {
            return false;
        }
        // "Program Manager" = la pseudo-fenêtre du bureau (explorer)
        if w.title == "Program Manager" {
            return false;
        }
        true
    });
    list
}

/// Nom d'application lisible (processus Windows → nom humain).
pub fn friendly_name(process: &str) -> String {
    let base = process.trim_end_matches(".exe").to_lowercase();
    let name = match base.as_str() {
        "msedge" => "Edge",
        "chrome" => "Chrome",
        "firefox" => "Firefox",
        "cmd" | "windowsterminal" | "wterminal" => "Terminal",
        "powershell" | "pwsh" => "PowerShell",
        "code" => "VS Code",
        "notepad" | "notepad++" => "Bloc-notes",
        "explorer" => "Explorateur",
        "onvue" => "OnVUE",
        "spotify" => "Spotify",
        "discord" => "Discord",
        "ollama" => "Ollama",
        "telegram" => "Telegram",
        "steam" => "Steam",
        "obs64" => "OBS",
        "systemsettings" => "Paramètres",
        "taskmgr" => "Gestionnaire des tâches",
        "vlc" => "VLC",
        "word" | "winword" => "Word",
        "excel" => "Excel",
        "outlook" => "Outlook",
        _ => {
            // Capitalise le premier caractère par défaut
            let mut c = base.chars();
            match c.next() {
                Some(f) => return f.to_uppercase().collect::<String>() + c.as_str(),
                None => return "App".into(),
            }
        }
    };
    name.to_string()
}

/// Nettoie un titre de fenêtre : retire les suffixes de navigateur connus,
/// puis raccourcit si trop long.
pub fn clean_title(title: &str) -> String {
    let mut t = title.to_string();
    for suffix in [
        " - Personnel - Microsoft Edge",
        " - Microsoft Edge",
        " - Google Chrome",
        " — Mozilla Firefox",
        " - Mozilla Firefox",
        " - Bloc-notes",
        " - Visual Studio Code",
    ] {
        if let Some(cleaned) = t.strip_suffix(suffix) {
            t = cleaned.to_string();
            break;
        }
    }
    // Un titre qui EST un chemin : ne garde que le dernier segment
    if t.contains('\\') {
        t = t
            .rsplit('\\')
            .next()
            .unwrap_or(&t)
            .to_string();
    }
    // Suffixes de navigateur — gère les tirets unicode et caractères invisibles
    for browser in ["Microsoft Edge", "Google Chrome", "Mozilla Firefox"] {
        if let Some(pos) = t.rfind(browser) {
            let before = &t[..pos];
            t = before
                .trim_end_matches(|c: char| {
                    matches!(c, '-' | '–' | '—' | ' ' | '\u{200b}' | '\u{200e}')
                })
                .to_string();
            break;
        }
    }
    if t.chars().count() > 45 {
        t = t.chars().take(42).collect::<String>() + "…";
    }
    t
}

#[tauri::command]
fn list_windows() -> Result<Vec<AppWindow>, String> {
    Ok(enumerate_windows()
        .into_iter()
        .map(|w| AppWindow {
            title: w.title,
            process: w.process,
        })
        .collect())
}

// ═══════════════════════ Permissions persistées ═══════════════════════

#[derive(Serialize, Deserialize, Clone)]
pub struct Permissions(HashMap<String, String>); // clé: "process::titre" → niveau

pub struct BrokerState(pub Mutex<Permissions>);

fn perms_path(app: &AppHandle) -> std::path::PathBuf {
    app.path()
        .app_data_dir()
        .expect("app_data_dir indisponible")
        .join("permissions.json")
}

fn load_permissions(app: &AppHandle) -> Permissions {
    let path = perms_path(app);
    fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or(Permissions(HashMap::new()))
}

fn save_permissions(app: &AppHandle, perms: &Permissions) -> Result<(), String> {
    let path = perms_path(app);
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    fs::write(&path, serde_json::to_string_pretty(perms).unwrap())
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn get_permissions(state: State<'_, BrokerState>) -> HashMap<String, String> {
    state.0.lock().unwrap().0.clone()
}

#[tauri::command]
fn set_permission(
    app: AppHandle,
    state: State<'_, BrokerState>,
    key: String,
    level: String,
) -> Result<(), String> {
    {
        let mut perms = state.0.lock().unwrap();
        if level == "none" {
            perms.0.remove(&key); // pas de permission = pas d'entrée
            log_event(&app, "door", &format!("refusé : {}", key));
        } else {
            perms.0.insert(key.clone(), level.clone());
            log_event(&app, "door", &format!("permission {} accordée : {}", level, key));
        }
        save_permissions(&app, &perms)?;
    }
    Ok(())
}

// ═══════════════ Curseur global (click-through pixel-perfect) ═══════════════

#[derive(Serialize)]
pub struct CursorPos {
    x: i32,
    y: i32,
}

/// Position physique (device pixels) du curseur sur l'écran virtuel.
#[tauri::command]
fn get_cursor_pos() -> Result<CursorPos, String> {
    let mut p = POINT::default();
    unsafe {
        GetCursorPos(&mut p).map_err(|e| e.to_string())?;
    }
    Ok(CursorPos { x: p.x, y: p.y })
}

// ═══════════════ Cerveau : couche Provider LLM-agnostic ═══════════════
// Le trait définit le contrat ; demain on ajoutera Gemini Live, OpenAI,
// Anthropic... sans toucher au reste du code. Ollama est l'implémentation
// locale par défaut (privacy-first).

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

fn default_vision_model() -> String {
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

const SYSTEM_PROMPT: &str = "Tu es Nid, un petit compagnon de bureau vivant dans son nid sur l'écran de Julien. Tu es chaleureux, curieux et légèrement taquin. Tu respectes son territoire : tu ne parles jamais des fenêtres qu'il a déclarées sanctuaires. Réponds TOUJOURS en français, très brièvement (1 à 2 phrases max) car tu parles dans une petite bulle de dialogue. Si Julien te demande une action (fermer, lister des fenêtres...), dis simplement que tu t'en occupes — le harnais s'en charge réellement.";

struct BrainState {
    config: Mutex<BrainConfig>,
    history: Mutex<Vec<ChatMessage>>,
    since_consolidation: Mutex<usize>,
}

fn load_brain_config(app: &AppHandle) -> BrainConfig {
    let path = app
        .path()
        .app_data_dir()
        .expect("app_data_dir")
        .join("brain.json");
    fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

#[tauri::command]
fn set_brain_config(
    app: AppHandle,
    state: State<'_, BrainState>,
    config: BrainConfig,
) -> Result<(), String> {
    let path = app.path().app_data_dir().unwrap().join("brain.json");
    fs::write(&path, serde_json::to_string_pretty(&config).unwrap())
        .map_err(|e| e.to_string())?;
    *state.config.lock().unwrap() = config;
    Ok(())
}

#[tauri::command]
fn get_brain_config(state: State<'_, BrainState>) -> BrainConfig {
    state.config.lock().unwrap().clone()
}

/// Envoie la conversation au provider configuré, en STREAMING :
/// chaque token est poussé au frontend via le Channel, et la réponse
/// complète est retournée à la fin (pour l'historique et le TTS).
#[tauri::command]
async fn chat(
    state: State<'_, BrainState>,
    broker: State<'_, BrokerState>,
    app: AppHandle,
    user_message: String,
    on_chunk: tauri::ipc::Channel<String>,
) -> Result<String, String> {
    // 0. Routeur d'intentions : les actions ne passent JAMAIS par le LLM.
    // Un mot-clé suffit — et c'est le broker qui vérifie les permissions.
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

    let cfg = state.config.lock().unwrap().clone();
    let user_msg = user_message.clone();

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

    let mut system = SYSTEM_PROMPT.to_string();
    system.push_str(&build_memory_block(&app, &user_msg));
    let mut messages = vec![ChatMessage {
        role: "system".into(),
        content: system,
    }];
    messages.extend(state.history.lock().unwrap().iter().cloned());

    match cfg.provider.as_str() {
        // ── Gemini : streaming SSE natif ──
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
            let contents: Vec<serde_json::Value> = turns
                .iter()
                .map(|m| {
                    serde_json::json!({
                        "role": if m.role == "assistant" { "model" } else { &m.role },
                        "parts": [{"text": &m.content}]
                    })
                })
                .collect();
            let body = serde_json::json!({
                "contents": contents,
                "systemInstruction": system_instruction,
                "generationConfig": {"temperature": 0.7}
            });
            let client = reqwest::Client::new();
            let mut resp = client
                .post(&url)
                .header("x-goog-api-key", &cfg.api_key)
                .json(&body)
                .send()
                .await
                .map_err(|e| format!("Gemini injoignable ({e})"))?;

            // Flux SSE : lignes "data: {...}"
            let mut full = String::new();
            let mut buffer = String::new();
            let mut stream = resp.bytes_stream();
            while let Some(chunk) = stream.next().await {
                let bytes = chunk.map_err(|e| e.to_string())?;
                buffer.push_str(&String::from_utf8_lossy(&bytes));
                while let Some(pos) = buffer.find('\n') {


                    let line: String = buffer.drain(..=pos).collect();
                    let line = line.trim();
                    if !line.starts_with("data: ") { continue; }
                    let Ok(v) = serde_json::from_str::<serde_json::Value>(&line[6..]) else { continue };
                    if let Some(tok) = v["candidates"][0]["content"]["parts"][0]["text"].as_str() {
                        if !tok.is_empty() {
                            full.push_str(tok);
                            let _ = on_chunk.send(tok.to_string());
                        }
                    }
                }
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
            let client = reqwest::Client::new();
            let mut resp = client
                .post(&url)
                .json(&body)
                .send()
                .await
                .map_err(|e| format!("Ollama injoignable ({e}) — serveur lancé ?"))?;

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

// ═══════════════════ Router d'intentions déterministe ═══════════════════
// Le petit modèle local (3B) est peu fiable pour le tool-calling JSON.
// Les actions sont donc détectées par mots-clés ICI, et exécutées par
// le broker avec vérification des permissions. Le LLM = pure conversation.

fn detect_action(msg: &str) -> Option<(String, String)> {
    let m = msg.to_lowercase();

    // Lister
    for trigger in [
        "quelles app", "quelle app", "apps ouvertes", "applications ouvertes",
        "fenêtres ouvertes", "fenetres ouvertes", "liste des fenêtres",
        "liste des fenetres", "qui est ouvert", "ce qui est ouvert",
    ] {
        if m.contains(trigger) {
            return Some(("list".into(), String::new()));
        }
    }

    // Fermer : "ferme X", "fermer la fenêtre X", "quitte X"
    for verb in ["ferme", "fermer", "quitte", "close"] {
        if let Some(t) =
            extract_target(&m, verb, &["la fenêtre", "le programme", "l'application"])
        {
            return Some(("close".into(), t));
        }
    }

    // Mettre au premier plan : "mets X au premier plan", "affiche X", "focus X"
    let focus_msg = m
        .replace("au premier plan", "")
        .replace("premier plan", "");
    for verb in [
        "mets", "mettre", "affiche", "afficher", "ramène", "ramener", "focus", "active",
    ] {
        if let Some(t) =
            extract_target(&focus_msg, verb, &["la fenêtre", "le programme", "l'application"])
        {
            return Some(("focus".into(), t));
        }
    }

    None
}

/// Extrait la cible après un verbe, en retirant les déterminants.
fn extract_target(msg: &str, verb: &str, stopwords: &[&str]) -> Option<String> {
    let idx = msg.find(verb)?;
    let after = &msg[idx + verb.len()..];
    let mut words: Vec<&str> = after
        .trim()
        .trim_start_matches([' ', ':', '\'', '"'])
        .split_whitespace()
        .collect();
    // retire les déterminants en tête
    while let Some(first) = words.first() {
        let w = first.trim_start_matches('\'');
        if w.is_empty()
            || [
                "le", "la", "les", "de", "du", "des", "un", "une", "mon", "ma", "cette", "ce",
                "l",
            ]
            .contains(&w)
            || stopwords.contains(&w)
        {
            words.remove(0);
        } else {
            break;
        }
    }
    let target = words.join(" ");
    if target.is_empty() {
        None
    } else {
        Some(target)
    }
}

// ═══════════════════════ Actions du harnais ═══════════════════════
// Chaque action passe par le broker : la permission est vérifiée ICI,
// dans le code Rust. Un LLM ne peut jamais contourner un sanctuaire.

use windows::Win32::UI::WindowsAndMessaging::{
    PostMessageW, SetForegroundWindow, ShowWindow, SW_RESTORE, WM_CLOSE,
};

fn execute_action(broker: &BrokerState, app: &AppHandle, action: &str, target: &str) -> String {
    let windows = enumerate_windows();
    match action {
        "list" => {
            // On ne révèle JAMAIS les sanctuaires, même à Julien
            let perms = broker.0.lock().unwrap();
            let mut seen = std::collections::HashSet::new();
            let mut names: Vec<String> = Vec::new();
            for w in windows {
                if !perms.0.get(&w.key()).map(|l| l != "sanctuary").unwrap_or(true) {
                    continue;
                }
                let app = friendly_name(&w.process);
                if seen.insert(app.clone()) {
                    let t = clean_title(&w.title);
                    if t.eq_ignore_ascii_case(&app) {
                        names.push(format!("• {}", app));
                    } else {
                        names.push(format!("• {} — {}", app, t));
                    }
                }
            }
            if names.is_empty() {
                "Aucune fenêtre visible 🌙".into()
            } else {
                format!("Applications ouvertes : {}", names.join("  "))
            }
        }
        "close" | "focus" => {
            let q = target.to_lowercase();
            let found = windows.into_iter().find(|w| {
                w.title.to_lowercase().contains(&q) || w.process.to_lowercase().contains(&q)
            });
            let Some(w) = found else {
                return format!("Je ne trouve pas de fenêtre « {} » 🤔", target);
            };
            let level = broker
                .0
                .lock()
                .unwrap()
                .0
                .get(&w.key())
                .cloned()
                .unwrap_or_else(|| "none".into());
            if level != "act" && level != "trust" {
                return format!(
                    "Je n'ai pas la permission d'agir sur « {} » — tu peux me l'accorder à la porte 🚪",
                    w.title
                );
            }
            let hwnd = HWND(w.hwnd as *mut core::ffi::c_void);
            unsafe {
                if action == "close" {
                    let _ = PostMessageW(Some(hwnd), WM_CLOSE, WPARAM(0), LPARAM(0));
                    log_event(app, "action", &format!("fermé « {} »", w.title));
                    format!("Voilà, « {} » est fermé ✅", w.title)
                } else {
                    ShowWindow(hwnd, SW_RESTORE);
                    let _ = SetForegroundWindow(hwnd);
                    log_event(app, "action", &format!("mis au premier plan : « {} »", w.title));
                    format!("« {} » est au premier plan 🎯", w.title)
                }
            }
        }
        _ => "Action inconnue".into(),
    }
}

// ═══════════════════ La maison : journal + album ═══════════════════
// 📔 Journal : les quittances — chaque événement (action, vision,
// permission) est logué en JSONL, auditable.
// 🖼️ Album : la mémoire par fenêtre — ce que Nid retient de chaque lieu.

fn journal_path(app: &AppHandle) -> std::path::PathBuf {
    app.path()
        .app_data_dir()
        .expect("app_data_dir")
        .join("journal.jsonl")
}

fn now_ts() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn log_event(app: &AppHandle, kind: &str, detail: &str) {
    let line = serde_json::json!({
        "ts": now_ts(),
        "kind": kind,
        "detail": detail
    });
    if let Ok(mut f) = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(journal_path(app))
    {
        use std::io::Write;
        let _ = writeln!(f, "{}", line);
    }
}

#[tauri::command]
fn get_journal(app: AppHandle, limit: usize) -> Vec<serde_json::Value> {
    let Ok(content) = fs::read_to_string(journal_path(&app)) else {
        return Vec::new();
    };
    let mut events: Vec<serde_json::Value> = content
        .lines()
        .filter_map(|l| serde_json::from_str(l).ok())
        .collect();
    events.reverse(); // du plus récent au plus ancien
    events.truncate(limit.min(500));
    events
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Memory {
    pub key: String, // "process::titre" de la fenêtre
    pub note: String,
    pub ts: u64,
}

fn memories_path(app: &AppHandle) -> std::path::PathBuf {
    app.path()
        .app_data_dir()
        .expect("app_data_dir")
        .join("memories.json")
}

fn load_memories(app: &AppHandle) -> Vec<Memory> {
    fs::read_to_string(memories_path(app))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn save_memories(app: &AppHandle, memories: &[Memory]) -> Result<(), String> {
    let path = memories_path(app);
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    fs::write(&path, serde_json::to_string_pretty(memories).unwrap())
        .map_err(|e| e.to_string())
}

/// Ajoute/rafraîchit un souvenir : un souvenir par fenêtre, remplacé à chaque vision
/// (le lifecycle learn/merge/prune complet arrive avec le polissage de l'album).
fn remember(app: &AppHandle, key: String, note: String) {
    let mut memories = load_memories(app);
    memories.retain(|m| m.key != key); // remplace l'ancien souvenir de ce lieu
    memories.push(Memory { key, note, ts: now_ts() });
    let _ = save_memories(app, &memories);
}

#[tauri::command]
fn add_memory(app: AppHandle, key: String, note: String) -> Result<(), String> {
    remember(&app, key, note);
    Ok(())
}

#[tauri::command]
fn get_memories(app: AppHandle) -> Vec<Memory> {
    load_memories(&app)
}

#[tauri::command]
fn clear_memory(app: AppHandle, key: String) -> Result<(), String> {
    let mut memories = load_memories(&app);
    memories.retain(|m| m.key != key);
    save_memories(&app, &memories)
}

// ═══════════════════ Zones interdites de l'écran ═══════════════════
// Des rectangles (pixels physiques) où Nid refuse d'aller ou de rester.
// Persistés dans zones.json.

#[derive(Serialize, Deserialize, Clone)]
pub struct Zone {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    pub label: String,
}

pub struct ZonesState(pub Mutex<Vec<Zone>>);

fn zones_path(app: &AppHandle) -> std::path::PathBuf {
    app.path()
        .app_data_dir()
        .expect("app_data_dir")
        .join("zones.json")
}

fn load_zones(app: &AppHandle) -> Vec<Zone> {
    fs::read_to_string(zones_path(app))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

#[tauri::command]
fn get_zones(state: State<'_, ZonesState>) -> Vec<Zone> {
    state.0.lock().unwrap().clone()
}

#[tauri::command]
fn set_zones(app: AppHandle, state: State<'_, ZonesState>, zones: Vec<Zone>) -> Result<(), String> {
    let path = zones_path(&app);
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    fs::write(&path, serde_json::to_string_pretty(&zones).unwrap())
        .map_err(|e| e.to_string())?;
    *state.0.lock().unwrap() = zones;
    Ok(())
}

// ═══════════════ Vision : capture d'une fenêtre AUTORISÉE ═══════════════
// LE point du contrat : le broker vérifie la permission AVANT la capture.
// Un sanctuaire ne peut jamais être capturé, même si le LLM le demande.

use base64::Engine as _;
use windows::Win32::Graphics::Gdi::{
    BI_RGB, BITMAPINFO, CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject,
    DIB_RGB_COLORS, GetDC, ReleaseDC, SelectObject,
};

fn capture_window_png(hwnd: HWND) -> Result<Vec<u8>, String> {
    unsafe {
        let mut rc = windows::Win32::Foundation::RECT::default();
        GetWindowRect(hwnd, &mut rc).map_err(|e| e.to_string())?;
        let w = (rc.right - rc.left).max(1);
        let h = (rc.bottom - rc.top).max(1);

        let hdc_screen = GetDC(None);
        let hdc_mem = CreateCompatibleDC(Some(hdc_screen));
        let mut bi = BITMAPINFO::default();
        bi.bmiHeader.biWidth = w;
        bi.bmiHeader.biHeight = -h; // top-down
        bi.bmiHeader.biPlanes = 1;
        bi.bmiHeader.biBitCount = 32;
        bi.bmiHeader.biCompression = BI_RGB.0;
        let mut bits: *mut core::ffi::c_void = std::ptr::null_mut();
        let hbm = CreateDIBSection(
            Some(hdc_mem),
            &bi,
            DIB_RGB_COLORS,
            &mut bits,
            None,
            0,
        )
        .map_err(|e| e.to_string())?;
        let old = SelectObject(hdc_mem, hbm.into());
        let ok = PrintWindow(hwnd, hdc_mem, PW_RENDERFULLCONTENT).as_bool();

        // BGRA → RGBA
        let len = (w * h * 4) as usize;
        let slice = std::slice::from_raw_parts(bits as *const u8, len);
        let mut rgba = Vec::with_capacity(len);
        for px in slice.chunks_exact(4) {
            rgba.extend_from_slice(&[px[2], px[1], px[0], px[3]]);
        }

        SelectObject(hdc_mem, old);
        DeleteObject(hbm.into());
        DeleteDC(hdc_mem);
        ReleaseDC(None, hdc_screen);

        if !ok {
            return Err("PrintWindow a échoué".into());
        }
        let img = image::RgbaImage::from_raw(w as u32, h as u32, rgba)
            .ok_or("image invalide")?;
        let mut png: Vec<u8> = Vec::new();
        image::DynamicImage::ImageRgba8(img)
            .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
            .map_err(|e| e.to_string())?;
        Ok(png)
    }
}

/// Regarde une fenêtre autorisée et décrit ce qu'il voit via le modèle vision.
#[tauri::command]
async fn look_at(
    state: State<'_, BrainState>,
    broker: State<'_, BrokerState>,
    app: AppHandle,
    target: String,
    question: String,
) -> Result<String, String> {
    let windows = enumerate_windows();
    let found = if target.trim().is_empty() {
        // Pas de cible → la fenêtre au premier plan
        let fg = unsafe { GetForegroundWindow() };
        windows.into_iter().find(|w| w.hwnd == fg.0 as isize)
    } else {
        let q = target.to_lowercase();
        windows.into_iter().find(|w| {
            w.title.to_lowercase().contains(&q) || w.process.to_lowercase().contains(&q)
        })
    };
    let Some(w) = found else {
        return Err(format!("Je ne trouve pas de fenêtre « {} » 🤔", target));
    };

    // 🔐 LE BROKER DÉCIDE : la capture exige look / act / trust
    let level = broker
        .0
        .lock()
        .unwrap()
        .0
        .get(&w.key())
        .cloned()
        .unwrap_or_else(|| "none".into());
    if !matches!(level.as_str(), "look" | "act" | "trust") {
        return Err(format!(
            "Je n'ai pas la permission de regarder « {} » — tu peux me l'accorder à la porte 🚪",
            w.title
        ));
    }

    let png = capture_window_png(HWND(w.hwnd as *mut core::ffi::c_void))?;
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
        let resp = reqwest::Client::new()
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
    let client = reqwest::Client::new();
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

// ═══════════════ Mémoire multi-niveaux (inspirée de Waku Agent) ═══════════════
// 💬 Working (session) · 📖 Épisodique (résumés) · 🎯 Sémantique (faits)
// · 🖼️ Spatial (album, par fenêtre). Consolidation par lots de 6 messages :
// le petit modèle distille, avec les filtres anti-bruit de Waku
// (jamais l'état opérationnel, jamais les réponses de l'agent, dédoublonnage).

#[derive(Serialize, Deserialize, Clone)]
pub struct Fact {
    pub subject: String,
    pub content: String,
    pub ts: u64,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Episode {
    pub summary: String,
    pub ts: u64,
}

fn facts_path(app: &AppHandle) -> std::path::PathBuf {
    app.path().app_data_dir().expect("app_data_dir").join("semantic.json")
}
fn episodes_path(app: &AppHandle) -> std::path::PathBuf {
    app.path().app_data_dir().expect("app_data_dir").join("episodic.json")
}

fn load_facts(app: &AppHandle) -> Vec<Fact> {
    fs::read_to_string(facts_path(app))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn load_episodes(app: &AppHandle) -> Vec<Episode> {
    fs::read_to_string(episodes_path(app))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

/// Filtre anti-bruit (Waku : is_ops_state) — l'état opérationnel change
/// trop vite pour être une mémoire.
fn is_ops_state(content: &str) -> bool {
    let c = content.to_lowercase();
    ["solde", "crédit", "credit", "token", "rate limit", "429", "402",
     "insuffisant", "balance", "erreur d'outil", "mémoire contient", "mémoire ne"]
        .iter()
        .any(|kw| c.contains(kw))
}

/// Dédoublonnage sémantique : un fait dont le contenu est déjà couvert
/// (contient / est contenu) n'entre pas (Waku : "restates").
fn is_duplicate(facts: &[Fact], content: &str) -> bool {
    let c = content.to_lowercase();
    facts.iter().any(|f| {
        let e = f.content.to_lowercase();
        (e.len() > 15 && c.contains(&e)) || (c.len() > 15 && e.contains(&c))
    })
}

const CONSOLIDATION_EVERY: usize = 6; // Waku : "only consolidate after N new chats"

const CONSOLIDATION_PROMPT: &str = "Tu distilles la conversation récente d'un compagnon de bureau en mémoire long terme. \
Extrais : 1) les faits durables sur Julien (préférences, projets, personnes, décisions) — un fait = une phrase courte qui nomme son sujet ; \
2) un épisode : une phrase résumant ce qui s'est passé dans cette conversation. \
NE RETIENS JAMAIS l'état opérationnel de l'assistant (soldes, crédits, erreurs d'outils, limites, tokens), \
ni le contenu des réponses de l'assistant, ni ce que la mémoire contient ou non. \
N'extrais que ce que Julien a dit ou décidé. Réponds UNIQUEMENT en JSON : \
{\"facts\":[{\"subject\":\"...\",\"content\":\"...\"}],\"episode\":\"...\"}";

/// Consolidation : distille les N derniers échanges en faits + épisode.
/// Lancée en tâche de fond — ne bloque jamais une réponse.
fn spawn_consolidation(app: AppHandle, exchanges: Vec<ChatMessage>) {
    tauri::async_runtime::spawn(async move {
        let cfg = {
            let brain = app.state::<BrainState>();
            brain.config.lock().unwrap().clone()
        };
        let body = serde_json::json!({
            "model": cfg.model,
            "messages": [{"role": "system", "content": CONSOLIDATION_PROMPT},
                         {"role": "user", "content": serde_json::to_string(&exchanges).unwrap_or_default()}],
            "stream": false,
            "think": false,
            "format": "json"
        });
        let Ok(resp) = reqwest::Client::new()
            .post(format!("{}/api/chat", cfg.url))
            .json(&body)
            .send()
            .await
        else { return; };
        let Ok(json) = resp.json::<serde_json::Value>().await else { return; };
        let Some(text) = json["message"]["content"].as_str() else { return; };
        let Ok(v) = serde_json::from_str::<serde_json::Value>(text) else { return; };

        // Faits : filtres Waku puis écriture
        let mut facts = load_facts(&app);
        let mut kept = 0;
        if let Some(list) = v["facts"].as_array() {
            for f in list {
                let (Some(subject), Some(content)) =
                    (f["subject"].as_str(), f["content"].as_str())
                else { continue };
                if is_ops_state(content) || is_duplicate(&facts, content) {
                    continue;
                }
                facts.push(Fact {
                    subject: subject.chars().take(60).collect(),
                    content: content.chars().take(200).collect(),
                    ts: now_ts(),
                });
                kept += 1;
            }
        }
        if kept > 0 {
            if facts.len() > 200 { facts = facts.split_off(facts.len() - 200); } // vieillissement
            let _ = fs::write(facts_path(&app), serde_json::to_string_pretty(&facts).unwrap());
        }

        // Épisode
        if let Some(ep) = v["episode"].as_str() {
            if !ep.is_empty() && !is_ops_state(ep) {
                let mut episodes = load_episodes(&app);
                episodes.push(Episode { summary: ep.chars().take(200).collect(), ts: now_ts() });
                if episodes.len() > 100 { episodes = episodes.split_off(episodes.len() - 100); }
                let _ = fs::write(episodes_path(&app), serde_json::to_string_pretty(&episodes).unwrap());
            }
        }
        log_event(&app, "memory", &format!("consolidation : {} fait(s) retenu(s)", kept));
    });
}

#[tauri::command]
fn get_facts(app: AppHandle) -> Vec<Fact> {
    load_facts(&app)
}

#[tauri::command]
fn forget_fact(app: AppHandle, ts: u64) -> Result<(), String> {
    let mut facts = load_facts(&app);
    facts.retain(|f| f.ts != ts);
    fs::write(facts_path(&app), serde_json::to_string_pretty(&facts).unwrap())
        .map_err(|e| e.to_string())
}

/// Injection : "ce que Nid sait déjà" — derniers faits + souvenirs spatiaux
/// pertinents (Waku : le bloc "company brain").
fn build_memory_block(app: &AppHandle, user_message: &str) -> String {
    let mut block = String::new();
    let facts = load_facts(app);
    if !facts.is_empty() {
        block.push_str("\n\nCe que tu sais déjà de Julien :\n");
        for f in facts.iter().rev().take(8) {
            block.push_str(&format!("- {}\n", f.content));
        }
    }
    // Album spatial : les souvenirs dont le lieu est évoqué dans le message
    let memories = load_memories(app);
    let mentioned: Vec<&Memory> = memories
        .iter()
        .filter(|m| {
            m.key.split([':', ' ', '.', '-'])
                .filter(|w| w.len() > 3)
                .any(|w| user_message.to_lowercase().contains(&w.to_lowercase()))
        })
        .take(3)
        .collect();
    if !mentioned.is_empty() {
        block.push_str("\nSouvenirs de lieux déjà regardés :\n");
        for m in mentioned {
            block.push_str(&format!("- [{}] {}\n", m.key.split("::").next().unwrap_or(""), m.note));
        }
    }
    block
}

// ═══════════════ Voix : STT whisper local + TTS système ═══════════════

use futures_util::{SinkExt, StreamExt};
use tokio_tungstenite::tungstenite::Message as WsMessage;
use whisper_rs::{
    FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters,
};

static WHISPER_CTX: OnceLock<WhisperContext> = OnceLock::new();

fn whisper_model_path() -> Option<std::path::PathBuf> {
    std::env::var("APPDATA").ok().map(|d| {
        std::path::PathBuf::from(d)
            .join("com.julienawonga.nid")
            .join("models")
            .join("ggml-base.bin")
    })
}

fn init_whisper() -> Result<&'static WhisperContext, String> {
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
fn gemini_voice_config() -> Option<(String, String)> {
    // lit brain.json directement pour éviter les dépendances de State
    let dir = std::env::var("APPDATA").ok()?;
    let path = std::path::PathBuf::from(dir)
        .join("com.julienawonga.nid")
        .join("brain.json");
    let cfg: BrainConfig = serde_json::from_str(&fs::read_to_string(path).ok()?).ok()?;
    if cfg.api_key.is_empty() { return None; }
    Some((cfg.model.clone(), cfg.api_key.clone()))
}

fn bytemuck_cast(samples: &[i16]) -> Vec<u8> {
    let mut out = Vec::with_capacity(samples.len() * 2);
    for s in samples {
        out.extend_from_slice(&s.to_le_bytes());
    }
    out
}

/// Reçoit du PCM 16 kHz mono (Int16) et retourne le texte transcrit.
#[tauri::command]
async fn transcribe(samples: Vec<i16>) -> Result<String, String> {
    // ── Gemini transcription (provider gemini + clé) : plus rapide que whisper local ──
    {
        let cfg = gemini_voice_config();
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
            let resp = reqwest::Client::new()
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
static TTS_CHILD: Mutex<Option<std::process::Child>> = Mutex::new(None);

fn wav_header(data_len: u32, sample_rate: u32) -> Vec<u8> {
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

use futures_util::stream::{SplitSink, SplitStream};
use futures_util::SinkExt as _;
type WsStream = tokio_tungstenite::WebSocketStream<
    tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
>;
type WsTx = SplitSink<WsStream, WsMessage>;
type WsRx = SplitStream<WsStream>;
static LIVE_TX: tokio::sync::Mutex<Option<WsTx>> = tokio::sync::Mutex::const_new(None);
static LIVE_CONNECT_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
// chunks micro mis en mémoire pendant l'ouverture de session (zéro mot perdu)
static MIC_PENDING: tokio::sync::Mutex<Vec<String>> = tokio::sync::Mutex::const_new(Vec::new());
static MIC_COUNT: tokio::sync::Mutex<usize> = tokio::sync::Mutex::const_new(0);

/// Vide le buffer micro vers la session Live (ordre préservé)
async fn flush_mic_pending(app: &AppHandle) -> Result<(), String> {
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
fn voice_emit(app: &AppHandle, event: &str, payload: &str) {
    let _ = app.emit(&format!("nid://voice-{event}"), payload.to_string());
}

/// Ouvre la session Gemini Live si absente (persistante entre les réponses).
async fn ensure_live_session(app: &AppHandle, key: &str) -> Result<(), String> {
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
    let (mut ws, _) = tokio_tungstenite::connect_async(req)
        .await
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
                let _ = ws.send(WsMessage::Pong(p));
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
                        voice_emit(&reader_app, "audio", d);
                    }
                    if let Some(tr) = part["text"].as_str() {
                        voice_emit(&reader_app, "nid-text", tr);
                    }
                }
            }
            if let Some(tr) = v["serverContent"]["inputTranscription"]["text"].as_str() {
                voice_emit(&reader_app, "user-text", tr);
            }
            if v["serverContent"]["turnComplete"].as_bool() == Some(true) {
                voice_emit(&reader_app, "turn-complete", "");
            }
        }
        let mut guard = LIVE_TX.lock().await;
        *guard = None;
    });
    Ok(())
}

/// Envoie un morceau de texte incrémental (turnComplete=false) — streaming
#[tauri::command]
async fn voice_feed(app: AppHandle, text: String) -> Result<(), String> {
    let key = gemini_key_from_state(&app).await;
    if key.is_empty() {
        return Err("Pas de clé Gemini dans le frigo 🍽️".into());
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
async fn voice_end(app: AppHandle) -> Result<(), String> {
    let key = gemini_key_from_state(&app).await;
    if key.is_empty() {
        return Err("Pas de clé Gemini dans le frigo 🍽️".into());
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
async fn mic_chunk(app: AppHandle, data: String) -> Result<(), String> {
    let mut cnt = MIC_COUNT.lock().await;
    *cnt += 1;
    let n = *cnt;
    if n % 10 == 1 {
        log_event(&app, "voice", &format!("live : {} chunk(s) micro envoyés", n));
    }
    {
        let cfg = app.state::<BrainState>().config.lock().unwrap().clone();
        if cfg.api_key.is_empty() {
            return Err("Pas de clé Gemini dans le frigo 🍽️".into());
        }
    }
    // session prête ? → envoie direct. Sinon → bufferise (zéro mot perdu)
    {
        let guard = LIVE_TX.lock().await;
        if guard.is_none() {
            MIC_PENDING.lock().await.push(data);
            ensure_live_session(&app, &gemini_key_from_state(&app).await).await?;
            flush_mic_pending(&app).await?;
            return Ok(());
        }
    }
    flush_mic_pending(&app).await?; // rattrape d'éventuels chunks en attente
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

async fn gemini_key_from_state(app: &AppHandle) -> String {
    app.state::<BrainState>().config.lock().unwrap().api_key.clone()
}

#[tauri::command]
async fn mic_end(app: AppHandle) -> Result<(), String> {
    flush_mic_pending(&app).await?;
    let mut guard = LIVE_TX.lock().await;
    let Some(tx) = guard.as_mut() else { return Ok(()) };
    let msg = serde_json::json!({"realtimeInput": {"audioStreamEnd": true}});
    tx.send(WsMessage::Text(msg.to_string()))
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn speak(
    app: AppHandle,
    text: String,
    on_chunk: tauri::ipc::Channel<String>,
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
        let tmp = app.path().app_data_dir().unwrap().join("nid-voice.wav");

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
        let resp = reqwest::Client::new()
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

// ═══════════════════════ Démarrage ═══════════════════════

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // TLS : installe explicitement le provider crypto (requis par rustls
    // quand plusieurs crates l'exigent — le WebSocket Gemini Live en a besoin)
    let _ = rustls::crypto::ring::default_provider().install_default();

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let perms = load_permissions(app.handle());
            app.manage(BrokerState(Mutex::new(perms)));
            let brain = load_brain_config(app.handle());
            app.manage(BrainState {
                config: Mutex::new(brain),
                history: Mutex::new(Vec::new()),
                since_consolidation: Mutex::new(0),
            });
            app.manage(ZonesState(Mutex::new(load_zones(app.handle()))));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            list_windows,
            get_permissions,
            set_permission,
            get_cursor_pos,
            chat,
            get_brain_config,
            set_brain_config,
            transcribe,
            speak,
            look_at,
            get_zones,
            set_zones,
            get_journal,
            add_memory,
            get_memories,
            clear_memory,
            get_facts,
            forget_fact,
            voice_feed,
            voice_end,
            mic_chunk,
            mic_end
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

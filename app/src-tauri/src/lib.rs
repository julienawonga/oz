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
use tauri::{AppHandle, Manager, State};
use windows::core::{BOOL, PWSTR};
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM, POINT};
use windows::Win32::System::Threading::{
    OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32,
    PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetCursorPos, GetWindowTextLengthW, GetWindowTextW,
    GetWindowThreadProcessId, IsWindowVisible,
};

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
        } else {
            perms.0.insert(key, level);
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
}

impl Default for BrainConfig {
    fn default() -> Self {
        Self {
            provider: "ollama".into(),
            model: "llama3.2:3b".into(),
            url: "http://127.0.0.1:11434".into(),
        }
    }
}

const SYSTEM_PROMPT: &str = "Tu es Nid, un petit compagnon de bureau vivant dans son nid sur l'écran de Julien. Tu es chaleureux, curieux et légèrement taquin. Tu respectes son territoire : tu ne parles jamais des fenêtres qu'il a déclarées sanctuaires. Réponds TOUJOURS en français, très brièvement (1 à 2 phrases max) car tu parles dans une petite bulle de dialogue. Si Julien te demande une action (fermer, lister des fenêtres...), dis simplement que tu t'en occupes — le harnais s'en charge réellement.";

struct BrainState {
    config: Mutex<BrainConfig>,
    history: Mutex<Vec<ChatMessage>>,
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
    user_message: String,
    on_chunk: tauri::ipc::Channel<String>,
) -> Result<String, String> {
    // 0. Routeur d'intentions : les actions ne passent JAMAIS par le LLM.
    // Un mot-clé suffit — et c'est le broker qui vérifie les permissions.
    if let Some((action, target)) = detect_action(&user_message) {
        let text = execute_action(&broker, &action, &target);
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

    let mut messages = vec![ChatMessage {
        role: "system".into(),
        content: SYSTEM_PROMPT.into(),
    }];
    messages.extend(state.history.lock().unwrap().iter().cloned());

    match cfg.provider.as_str() {
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

fn execute_action(broker: &BrokerState, action: &str, target: &str) -> String {
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
                    format!("Voilà, « {} » est fermé ✅", w.title)
                } else {
                    ShowWindow(hwnd, SW_RESTORE);
                    let _ = SetForegroundWindow(hwnd);
                    format!("« {} » est au premier plan 🎯", w.title)
                }
            }
        }
        _ => "Action inconnue".into(),
    }
}

// ═══════════════ Voix : STT whisper local + TTS système ═══════════════

use futures_util::StreamExt;
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

/// Reçoit du PCM 16 kHz mono (Int16) et retourne le texte transcrit.
#[tauri::command]
fn transcribe(samples: Vec<i16>) -> Result<String, String> {
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

#[tauri::command]
fn speak(app: AppHandle, text: String) -> Result<(), String> {
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
        // 1. Tue la voix en cours s'il y en a une
        if let Ok(mut guard) = TTS_CHILD.lock() {
            if let Some(old) = guard.as_mut() {
                let _ = old.kill();
                let _ = old.wait();
            }
            *guard = None;
        }
        // 2. Lance la nouvelle voix
        let mut cmd = std::process::Command::new("powershell");
        cmd.args(["-NoProfile", "-Command", &script]);
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
        }
        match cmd.spawn() {
            Ok(child) => {
                if let Ok(mut guard) = TTS_CHILD.lock() {
                    *guard = Some(child);
                }
                // 3. Attend la fin pour libérer le slot
                if let Ok(mut guard) = TTS_CHILD.lock() {
                    if let Some(c) = guard.as_mut() {
                        let _ = c.wait();
                    }
                    *guard = None;
                }
            }
            Err(_) => {}
        }
    });
    Ok(())
}

// ═══════════════════════ Démarrage ═══════════════════════

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let perms = load_permissions(app.handle());
            app.manage(BrokerState(Mutex::new(perms)));
            let brain = load_brain_config(app.handle());
            app.manage(BrainState {
                config: Mutex::new(brain),
                history: Mutex::new(Vec::new()),
            });
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
            speak
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

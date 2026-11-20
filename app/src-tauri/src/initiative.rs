//! Initiative : Oz engage la conversation de lui-même.
//!
//! 1. SIGNAUX mesurés par le PC (aucun mot-clé) : retour après une absence,
//!    début de journée, longue session sans échange.
//! 2. GARDE-FOUS dans le code : réglage coupé, « pas maintenant », plein
//!    écran, conversation en cours, quota par heure, recul après les
//!    appels ignorés.
//! 3. JUGEMENT par Gemini : à partir du contexte (heure, fenêtre active,
//!    mémoire, derniers échanges), il décide de parler ou de se taire, et
//!    de la manière : appeler l'utilisateur par son prénom, ou lever la main.
//! Le frontend (egg/initiative.ts) met en scène et rend compte du résultat.

use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::Mutex;

use serde_json::json;
use tauri::{AppHandle, Emitter, Manager};
use windows::Win32::Foundation::RECT;
use windows::Win32::Graphics::Gdi::{GetMonitorInfoW, MonitorFromWindow, MONITORINFO, MONITOR_DEFAULTTONEAREST};
use windows::Win32::System::SystemInformation::{GetLocalTime, GetTickCount};
use windows::Win32::UI::Input::KeyboardAndMouse::{GetLastInputInfo, LASTINPUTINFO};
use windows::Win32::UI::WindowsAndMessaging::{GetClassNameW, GetForegroundWindow, GetWindowRect};

use crate::brain::{http_client, BrainState};
use crate::broker::BrokerState;
use crate::journal::{log_event, now_ts};

// ── Seuils ──
const TICK_SECS: u64 = 15;
const AWAY_SECS: u64 = 15 * 60; // absence comptée comme un départ
const BACK_IDLE_SECS: u64 = 10; // activité récente = l'utilisateur est revenu
const SESSION_SECS: u64 = 50 * 60; // longue session sans échange
const MIN_GAP_SECS: u64 = 12 * 60; // entre deux initiatives
const IGNORED_PENALTY_SECS: u64 = 25 * 60; // par appel ignoré d'affilée
const MAX_PER_HOUR: usize = 3;
const QUIET_AFTER_EXCHANGE_SECS: u64 = 3 * 60;

// ── État ──
static LAST_EXCHANGE: AtomicU64 = AtomicU64::new(0);
static LAST_INITIATIVE: AtomicU64 = AtomicU64::new(0);
static SNOOZE_UNTIL: AtomicU64 = AtomicU64::new(0);
static IGNORED_STREAK: AtomicU32 = AtomicU32::new(0);
/// Une initiative est en cours de mise en scène côté œuf.
static PENDING: AtomicBool = AtomicBool::new(false);
static RECENT: Mutex<Vec<u64>> = Mutex::new(Vec::new());

/// Un échange a eu lieu (voix ou texte) : Oz n'a pas à relancer tout de suite.
pub(crate) fn note_exchange() {
    LAST_EXCHANGE.store(now_ts(), Ordering::Relaxed);
}

/// « Pas maintenant » : Oz se tait pendant `minutes`.
pub(crate) fn snooze(app: &AppHandle, minutes: u64) -> String {
    let minutes = minutes.clamp(5, 12 * 60);
    SNOOZE_UNTIL.store(now_ts() + minutes * 60, Ordering::Relaxed);
    log_event(app, "initiative", &format!("en sourdine pour {minutes} min"));
    format!("D'accord, Oz ne prendra pas la parole de lui-même pendant {minutes} minutes ; le micro se coupe. Si tu as déjà répondu, ne dis plus rien ; sinon, un seul mot.")
}

/// Secondes depuis la dernière action clavier/souris.
fn idle_secs() -> u64 {
    let mut info = LASTINPUTINFO { cbSize: std::mem::size_of::<LASTINPUTINFO>() as u32, dwTime: 0 };
    // SAFETY : structure initialisée avec sa taille, lue par l'API.
    unsafe {
        if !GetLastInputInfo(&mut info).as_bool() {
            return 0;
        }
        (GetTickCount().wrapping_sub(info.dwTime) / 1000) as u64
    }
}

/// La fenêtre au premier plan couvre tout son écran (film, présentation, jeu).
fn foreground_is_fullscreen() -> bool {
    // SAFETY : appels Win32 en lecture sur la fenêtre au premier plan.
    unsafe {
        let h = GetForegroundWindow();
        if h.0.is_null() {
            return false;
        }
        let mut class = [0u16; 64];
        let n = GetClassNameW(h, &mut class) as usize;
        let class = String::from_utf16_lossy(&class[..n]);
        if class == "Progman" || class == "WorkerW" {
            return false; // le bureau lui-même
        }
        let mut r = RECT::default();
        if GetWindowRect(h, &mut r).is_err() {
            return false;
        }
        let mon = MonitorFromWindow(h, MONITOR_DEFAULTTONEAREST);
        let mut mi = MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };
        if !GetMonitorInfoW(mon, &mut mi).as_bool() {
            return false;
        }
        let m = mi.rcMonitor;
        r.left <= m.left && r.top <= m.top && r.right >= m.right && r.bottom >= m.bottom
    }
}

fn local_day_and_hour() -> (String, u16) {
    // SAFETY : GetLocalTime remplit une structure, sans précondition.
    let t = unsafe { GetLocalTime() };
    (format!("{:04}-{:02}-{:02}", t.wYear, t.wMonth, t.wDay), t.wHour)
}

fn day_path(app: &AppHandle) -> std::path::PathBuf {
    app.path().app_data_dir().expect("app_data_dir").join("initiative-day.txt")
}

/// Ce que l'utilisateur a au premier plan, jamais un sanctuaire.
fn active_window_line(app: &AppHandle) -> String {
    let wins = crate::windows_api::enumerate_windows();
    let Some(w) = crate::windows_api::active_window(wins) else {
        return "Fenêtre au premier plan : aucune.".into();
    };
    if app.state::<BrokerState>().0.lock().unwrap().level_for(&w) == "sanctuary" {
        return "Fenêtre au premier plan : privée.".into();
    }
    format!(
        "Fenêtre au premier plan : {} — {}.",
        crate::windows_api::friendly_name(&w.process),
        crate::windows_api::clean_title(&w.title)
    )
}

/// Les raisons de parler que les mesures font apparaître.
#[derive(Clone, Debug)]
enum Signal {
    Matin,
    Retour { minutes: u64 },
    LongueSession { minutes: u64 },
    Essai,
}

impl Signal {
    fn describe(&self) -> String {
        match self {
            Signal::Matin => "%OWNER% vient d'arriver sur son ordinateur pour la première fois aujourd'hui.".into(),
            Signal::Retour { minutes } => format!("%OWNER% revient après {minutes} minutes loin de son ordinateur."),
            Signal::LongueSession { minutes } => format!(
                "%OWNER% travaille depuis {minutes} minutes sans que vous ayez échangé un mot."
            ),
            Signal::Essai => "%OWNER% veut tester ta prise de parole spontanée : engage la conversation, sur un sujet de ton choix.".into(),
        }
    }
    fn name(&self) -> &'static str {
        match self {
            Signal::Matin => "matin",
            Signal::Retour { .. } => "retour",
            Signal::LongueSession { .. } => "longue session",
            Signal::Essai => "essai",
        }
    }
}

const JUDGE_PROMPT: &str = "Tu es le jugement d'Oz, un petit œuf compagnon de bureau, le pote de %OWNER%. Un signal vient de se produire sur l'ordinateur de %OWNER%. Décide si Oz engage la conversation maintenant, de lui-même, comme le ferait un ami présent dans la pièce. Bonnes raisons : le saluer le matin ou à son retour, prendre des nouvelles d'un sujet qu'il a évoqué (rendez-vous, projet, souci, envie), réagir à ce qu'il fait si c'est vraiment pertinent, lui proposer une pause après une longue session. Tais-toi si tu n'as rien de sincère ou de neuf à dire, si vous venez de parler, ou si %OWNER% semble concentré sur quelque chose de sérieux et que ça peut attendre. N'invente aucun fait : appuie-toi seulement sur le contexte fourni. Choisis aussi la manière. « appeler » : Oz appelle %OWNER% par son prénom, comme quelqu'un dans la pièce ; pour une salutation, un retour, quelque chose de chaleureux ou qui ne peut pas attendre. « lever_la_main » : un signe discret, %OWNER% viendra quand il voudra ; quand il est occupé ou que le sujet est secondaire. Dans le doute, tais-toi.";

fn judge_tools() -> serde_json::Value {
    json!([{ "functionDeclarations": [
        {
            "name": "speak_up",
            "description": "Oz engage la conversation.",
            "parameters": {"type": "object", "properties": {
                "maniere": {"type": "string", "enum": ["appeler", "lever_la_main"]},
                "sujet": {"type": "string", "description": "Ce dont Oz veut parler et pourquoi, en une phrase (note pour Oz, pas pour %OWNER%)."},
                "premiere_phrase": {"type": "string", "description": "Ce qu'Oz dira d'abord quand %OWNER% l'écoutera : oral, naturel, une ou deux phrases courtes, sans émoji."}
            }, "required": ["maniere", "sujet", "premiere_phrase"]}
        },
        {
            "name": "stay_silent",
            "description": "Oz ne dit rien pour l'instant.",
            "parameters": {"type": "object", "properties": {
                "raison": {"type": "string"}
            }, "required": ["raison"]}
        }
    ]}])
}

/// Demande à Gemini s'il faut parler. Renvoie l'appel de fonction choisi.
async fn judge(app: &AppHandle, signal: &Signal) -> Result<serde_json::Value, String> {
    let cfg = app.state::<BrainState>().config.lock().unwrap().clone();
    if cfg.api_key.is_empty() {
        return Err("pas de clé Gemini".into());
    }
    let model = cfg.model.clone();
    let history: Vec<String> = app
        .state::<BrainState>()
        .history
        .lock()
        .unwrap()
        .iter()
        .rev()
        .take(8)
        .rev()
        .map(|m| format!("{} : {}", if m.role == "user" { "%OWNER%" } else { "Oz" }, m.content))
        .collect();
    let mut ctx = format!("Signal : {}\n{}", signal.describe(), crate::brain::now_line().trim());
    ctx.push('\n');
    ctx.push_str(&active_window_line(app));
    ctx.push_str(&format!(
        "\nOz parle en {} : écris premiere_phrase dans cette langue.",
        crate::prefs::language_name(app)
    ));
    let ignored = IGNORED_STREAK.load(Ordering::Relaxed);
    if ignored > 0 {
        ctx.push_str(&format!("\n%OWNER% n'a pas répondu à tes {ignored} dernière(s) tentative(s) : sois plus réservé."));
    }
    ctx.push_str(&crate::memory::build_memory_block(app, ""));
    if history.is_empty() {
        ctx.push_str("\n\nPas d'échange récent.");
    } else {
        ctx.push_str("\n\nDerniers échanges :\n");
        ctx.push_str(&history.join("\n"));
    }

    let url = format!("https://generativelanguage.googleapis.com/v1beta/models/{model}:generateContent");
    let mut body = json!({
        "systemInstruction": {"parts": [{"text": crate::prefs::personalize(app, JUDGE_PROMPT)}]},
        "contents": [{"role": "user", "parts": [{"text": crate::prefs::personalize(app, &ctx)}]}],
        "tools": judge_tools(),
        "toolConfig": {"functionCallingConfig": {"mode": "ANY"}},
        "generationConfig": {"temperature": 0.8, "thinkingConfig": {"thinkingLevel": "low"}}
    });
    let mut resp = send(&url, &cfg.api_key, &body).await?;
    if resp.status() == reqwest::StatusCode::BAD_REQUEST {
        // réglage de réflexion refusé par ce modèle : on s'en passe
        body["generationConfig"] = json!({"temperature": 0.8});
        resp = send(&url, &cfg.api_key, &body).await?;
    }
    if !resp.status().is_success() {
        return Err(crate::brain::http_error("Gemini", resp).await);
    }
    let v: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
    v["candidates"][0]["content"]["parts"]
        .as_array()
        .and_then(|parts| parts.iter().find_map(|p| p.get("functionCall").cloned()))
        .ok_or_else(|| "réponse sans décision".into())
}

async fn send(url: &str, key: &str, body: &serde_json::Value) -> Result<reqwest::Response, String> {
    http_client()
        .post(url)
        .header("x-goog-api-key", key)
        .json(body)
        .send()
        .await
        .map_err(|e| format!("Gemini injoignable ({e})"))
}

/// Une mise en scène est en cours (oubliée au bout de 3 min si l'œuf
/// n'a jamais rendu compte, par exemple après un rechargement).
fn pending(now: u64) -> bool {
    PENDING.load(Ordering::Relaxed) && now.saturating_sub(LAST_INITIATIVE.load(Ordering::Relaxed)) < 180
}

/// Juge le signal et, si Oz doit parler, prévient l'œuf. Faux si le
/// jugement n'a pas pu avoir lieu (hors ligne, clé absente).
async fn consider(app: &AppHandle, signal: Signal) -> bool {
    let call = match judge(app, &signal).await {
        Ok(c) => c,
        Err(e) => {
            log_event(app, "initiative", &format!("{} : jugement impossible ({e})", signal.name()));
            return false;
        }
    };
    let args = &call["args"];
    if call["name"].as_str() != Some("speak_up") {
        log_event(
            app,
            "initiative",
            &format!("{} : reste silencieux ({})", signal.name(), args["raison"].as_str().unwrap_or("")),
        );
        return true;
    }
    let maniere = args["maniere"].as_str().unwrap_or("lever_la_main");
    log_event(
        app,
        "initiative",
        &format!("{} : {maniere} — {}", signal.name(), args["sujet"].as_str().unwrap_or("")),
    );
    PENDING.store(true, Ordering::Relaxed);
    LAST_INITIATIVE.store(now_ts(), Ordering::Relaxed);
    RECENT.lock().unwrap().push(now_ts());
    let _ = app.emit("oz://initiative", json!({
        "maniere": maniere,
        "sujet": args["sujet"],
        "premiere_phrase": args["premiere_phrase"],
    }));
    true
}

/// L'œuf rend compte : « answered », « ignored » ou « busy ».
#[tauri::command]
pub(crate) fn initiative_result(app: AppHandle, outcome: String) {
    PENDING.store(false, Ordering::Relaxed);
    match outcome.as_str() {
        "answered" => {
            IGNORED_STREAK.store(0, Ordering::Relaxed);
            note_exchange();
        }
        "ignored" => {
            IGNORED_STREAK.fetch_add(1, Ordering::Relaxed);
        }
        _ => {
            // occupé : l'initiative ne compte pas dans le quota
            RECENT.lock().unwrap().pop();
            LAST_INITIATIVE.store(0, Ordering::Relaxed);
        }
    }
    log_event(&app, "initiative", &format!("résultat : {outcome}"));
}

/// Bouton « Essayer » du panneau : passe les garde-fous de rythme.
#[tauri::command]
pub(crate) async fn initiative_try(app: AppHandle) -> Result<(), String> {
    if pending(now_ts()) {
        return Err("Une prise de parole est déjà en cours.".into());
    }
    consider(&app, Signal::Essai).await;
    Ok(())
}

/// Boucle de fond : lit les signaux toutes les 15 s.
pub(crate) fn spawn(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let mut away_for: u64 = 0; // durée de la dernière absence (0 = présent)
        let mut active_since = now_ts();
        let mut last_day = std::fs::read_to_string(day_path(&app)).unwrap_or_default().trim().to_string();
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(TICK_SECS)).await;
            let now = now_ts();
            let idle = idle_secs();

            // ── mesures (toujours tenues à jour) ──
            if idle >= AWAY_SECS {
                away_for = away_for.max(idle);
                active_since = now;
                continue;
            }
            if idle > 5 * 60 {
                active_since = now; // pause courte : la session repart de zéro
            }
            let back = away_for > 0 && idle <= BACK_IDLE_SECS;
            let (today, hour) = local_day_and_hour();
            let morning = today != last_day && hour >= 5 && idle <= BACK_IDLE_SECS;
            let last_talk = LAST_EXCHANGE.load(Ordering::Relaxed).max(LAST_INITIATIVE.load(Ordering::Relaxed));
            let session = now.saturating_sub(active_since.max(last_talk));

            let signal = if morning {
                Some(Signal::Matin)
            } else if back {
                Some(Signal::Retour { minutes: away_for / 60 })
            } else if session >= SESSION_SECS {
                Some(Signal::LongueSession { minutes: session / 60 })
            } else {
                None
            };
            let Some(signal) = signal else { continue };

            // ── garde-fous ──
            let ignored = IGNORED_STREAK.load(Ordering::Relaxed) as u64;
            let gap = MIN_GAP_SECS + ignored * IGNORED_PENALTY_SECS;
            let recent = {
                let mut r = RECENT.lock().unwrap();
                r.retain(|t| now.saturating_sub(*t) < 3600);
                r.len()
            };
            let prefs = crate::prefs::current(&app);
            let blocked = if !prefs.initiative {
                Some("réglage coupé")
            } else if prefs.move_mode == "concentration" {
                Some("mode concentration")
            } else if pending(now) {
                Some("prise de parole déjà en cours")
            } else if now < SNOOZE_UNTIL.load(Ordering::Relaxed) {
                Some("pas maintenant")
            } else if now.saturating_sub(LAST_EXCHANGE.load(Ordering::Relaxed)) < QUIET_AFTER_EXCHANGE_SECS {
                Some("conversation en cours")
            } else if now.saturating_sub(LAST_INITIATIVE.load(Ordering::Relaxed)) < gap {
                Some("trop tôt après la précédente")
            } else if recent >= MAX_PER_HOUR {
                Some("quota de l'heure atteint")
            } else if foreground_is_fullscreen() {
                Some("plein écran")
            } else {
                None
            };

            // Un retour ne se rattrape pas plus tard : il est consommé tout de
            // suite. Le bonjour du matin, lui, attend la fin d'un plein écran
            // ou le retour du réseau ; une conversation en cours le remplace.
            match signal {
                Signal::Retour { .. } => away_for = 0,
                Signal::LongueSession { .. } => active_since = now, // pas de relance à chaque tick
                Signal::Matin | Signal::Essai => {}
            }
            let is_morning = matches!(signal, Signal::Matin);
            let decided = match blocked {
                Some(why) => {
                    log_event(&app, "initiative", &format!("{} : bloqué ({why})", signal.name()));
                    why != "plein écran" && why != "prise de parole déjà en cours"
                }
                None => consider(&app, signal).await,
            };
            if is_morning && decided {
                last_day = today.clone();
                let _ = std::fs::write(day_path(&app), &today);
                away_for = 0;
            }
        }
    });
}

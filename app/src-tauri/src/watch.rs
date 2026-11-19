//! « Regarde avec moi » : Oz voit en direct UNE fenêtre autorisée.
//!
//! Une image par seconde part dans la conversation vocale (Gemini Live,
//! `realtimeInput.video`), seulement pendant que Julien lui parle : Oz
//! répond sur ce qu'il voit en moins d'une seconde, sans capture à la
//! demande. Le broker est consulté à CHAQUE image : un accès retiré ou un
//! sanctuaire arrête la vue sur-le-champ. Un témoin reste visible sur l'œuf.

use base64::Engine as _;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use tauri::{AppHandle, Emitter};
use windows::Win32::UI::WindowsAndMessaging::{IsIconic, IsWindow};

use crate::journal::{log_event, now_ts};
use crate::windows_api::{friendly_name, WindowHandle};

const FRAME_EVERY: Duration = Duration::from_secs(1);
/// Le micro est muet pendant qu'Oz parle : la conversation compte comme
/// active tant qu'un morceau de micro est arrivé dans ce délai.
const CONVERSING_SECS: u64 = 20;
/// Sans conversation pendant ce temps, la vue s'arrête d'elle-même.
const IDLE_STOP_SECS: u64 = 20 * 60;
const MAX_FAILURES: u32 = 5;

/// Fenêtre regardée (nom lisible) ; None = Oz ne regarde rien.
static WATCHING: Mutex<Option<String>> = Mutex::new(None);
/// Change à chaque démarrage ou arrêt : une boucle périmée s'arrête seule.
static WATCH_ID: AtomicU64 = AtomicU64::new(0);

/// Ce qu'Oz regarde en ce moment, pour son prompt.
pub(crate) fn current() -> Option<String> {
    WATCHING.lock().unwrap().clone()
}

/// Outil `regarder_avec_moi`.
pub(crate) fn tool(app: &AppHandle, action: &str, target: &str) -> String {
    match action {
        "arreter" => stop(app, "à la demande de Julien"),
        _ => start(app, target),
    }
}

fn start(app: &AppHandle, target: &str) -> String {
    let w = match crate::vision::lookable_window(app, target) {
        Ok(w) => w,
        Err(e) => return e,
    };
    let label = friendly_name(&w.process);
    let id = WATCH_ID.fetch_add(1, Ordering::SeqCst) + 1;
    *WATCHING.lock().unwrap() = Some(label.clone());
    let _ = app.emit("oz://watch", serde_json::json!({"active": true, "label": label}));
    log_event(app, "watch", &format!("regarde en direct « {} »", w.title));
    tauri::async_runtime::spawn(watch_loop(app.clone(), w, id));
    format!(
        "Tu vois maintenant {label} en direct (une image par seconde pendant que Julien te parle). \
         Réponds sur ce que tu vois sans utiliser regarder_fenetre pour cette fenêtre."
    )
}

pub(crate) fn stop(app: &AppHandle, why: &str) -> String {
    WATCH_ID.fetch_add(1, Ordering::SeqCst);
    let was = WATCHING.lock().unwrap().take();
    let _ = app.emit("oz://watch", serde_json::json!({"active": false}));
    match was {
        Some(label) => {
            log_event(app, "watch", &format!("arrête de regarder {label} ({why})"));
            format!("Tu ne regardes plus {label}.")
        }
        None => "Tu ne regardais aucune fenêtre.".into(),
    }
}

async fn watch_loop(app: AppHandle, w: WindowHandle, id: u64) {
    let mut last_conversation = now_ts();
    let mut failures = 0u32;
    loop {
        let tick = Instant::now();
        if WATCH_ID.load(Ordering::SeqCst) != id {
            return; // arrêtée ou remplacée
        }
        if !crate::voice::conversing_within(CONVERSING_SECS) {
            if now_ts().saturating_sub(last_conversation) > IDLE_STOP_SECS {
                stop(&app, "plus de conversation depuis 20 min");
                return;
            }
            tokio::time::sleep(FRAME_EVERY).await;
            continue;
        }
        last_conversation = now_ts();

        // 🔐 le broker à chaque image : un accès retiré arrête tout
        if crate::vision::can_look(&app, &w) != Some(true) {
            stop(&app, "accès retiré");
            return;
        }
        // SAFETY : lecture de l'état d'une fenêtre par son handle.
        let (alive, minimized) = unsafe { (IsWindow(Some(w.raw())).as_bool(), IsIconic(w.raw()).as_bool()) };
        if !alive {
            stop(&app, "fenêtre fermée");
            return;
        }
        if !minimized {
            let shot = w.clone();
            // capture hors du fil asynchrone : elle prend jusqu'à 1 s
            let captured = tokio::task::spawn_blocking(move || shot.capture_jpeg())
                .await
                .unwrap_or_else(|e| Err(e.to_string()));
            match captured {
                Ok(jpg) => {
                    failures = 0;
                    let b64 = base64::engine::general_purpose::STANDARD.encode(&jpg);
                    if let Err(e) = crate::voice::send_video_frame(b64).await {
                        log_event(&app, "watch", &format!("image non envoyée : {e}"));
                    }
                }
                Err(e) => {
                    failures += 1;
                    if failures >= MAX_FAILURES {
                        stop(&app, &format!("capture impossible : {e}"));
                        return;
                    }
                }
            }
        }
        // une image par seconde, capture comprise
        if let Some(rest) = FRAME_EVERY.checked_sub(tick.elapsed()) {
            tokio::time::sleep(rest).await;
        }
    }
}

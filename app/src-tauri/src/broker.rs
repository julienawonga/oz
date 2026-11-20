//! Broker de permissions + routeur d'intentions + actions du harnais.
//! Les permissions s'appliquent DANS LE CODE, pas dans le prompt :
//! un LLM ne peut jamais capturer ou fermer une fenêtre sanctuaire.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::journal::log_event;
use crate::windows_api::{clean_title, enumerate_windows, friendly_name, WindowHandle};

#[derive(Serialize, Deserialize, Clone)]
pub struct Permissions(HashMap<String, String>); // clé: "process::titre" → niveau

pub struct BrokerState(pub Mutex<Permissions>);

impl Permissions {
    /// Niveau effectif d'une fenêtre : règle exacte, sinon règle de
    /// l'application entière (`processus::*`). Un sanctuaire, à l'un ou
    /// l'autre niveau, l'emporte toujours.
    pub(crate) fn level_of(&self, process: &str, title: &str) -> String {
        let exact = self.0.get(&format!("{process}::{title}"));
        let app = self.0.get(&format!("{process}::*"));
        if exact.map(String::as_str) == Some("sanctuary") || app.map(String::as_str) == Some("sanctuary") {
            return "sanctuary".into();
        }
        exact.or(app).cloned().unwrap_or_else(|| "none".into())
    }

    pub(crate) fn level_for(&self, w: &WindowHandle) -> String {
        self.level_of(&w.process, &w.title)
    }
}

/// Demande d'accès : le frontend affiche un badge discret sur l'œuf
/// (au lieu d'un long message dans la bulle). `need` = "look" | "act".
pub(crate) fn request_access(app: &AppHandle, w: &WindowHandle, need: &str) {
    let _ = app.emit(
        "oz://access-request",
        serde_json::json!({
            "app": friendly_name(&w.process),
            "process": w.process,
            "title": clean_title(&w.title),
            "need": need,
        }),
    );
}

pub(crate) fn perms_path(app: &AppHandle) -> std::path::PathBuf {
    app.path()
        .app_data_dir()
        .expect("app_data_dir indisponible")
        .join("permissions.json")
}

pub(crate) fn load_permissions(app: &AppHandle) -> Permissions {
    let path = perms_path(app);
    fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or(Permissions(HashMap::new()))
}

pub(crate) fn save_permissions(app: &AppHandle, perms: &Permissions) -> Result<(), String> {
    let path = perms_path(app);
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    fs::write(&path, serde_json::to_string_pretty(perms).unwrap())
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub(crate) fn get_permissions(state: State<'_, BrokerState>) -> HashMap<String, String> {
    state.0.lock().unwrap().0.clone()
}

#[tauri::command]
pub(crate) fn set_permission(
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

// ═══════════════════ Actions sur les fenêtres ═══════════════════
// Appelées par les outils de Gemini (tools.rs), toujours après
// vérification des permissions.

pub(crate) fn execute_action(broker: &BrokerState, app: &AppHandle, action: &str, target: &str) -> String {
    let windows = enumerate_windows();
    match action {
        "list" => {
            // On ne révèle JAMAIS les sanctuaires, même à l'utilisateur
            let perms = broker.0.lock().unwrap();
            let mut seen = std::collections::HashSet::new();
            let mut names: Vec<String> = Vec::new();
            for w in windows {
                if perms.level_for(&w) == "sanctuary" {
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
            let q = target.trim().to_lowercase();
            if q.is_empty() {
                // une cible vide correspondrait à TOUTES les fenêtres
                return "Quelle fenêtre ? Donne-moi son nom.".into();
            }
            let found = windows.into_iter().find(|w| {
                w.title.to_lowercase().contains(&q) || w.process.to_lowercase().contains(&q)
            });
            let Some(w) = found else {
                return format!("Je ne trouve pas de fenêtre « {} »", target);
            };
            let level = broker.0.lock().unwrap().level_for(&w);
            if level == "sanctuary" {
                // un sanctuaire n'existe pas pour Oz : même réponse que « introuvable »
                return format!("Je ne trouve pas de fenêtre « {} »", target);
            }
            if level != "act" && level != "trust" {
                request_access(app, &w, "act");
                return format!("J'ai besoin de ton accord pour agir sur {}.", friendly_name(&w.process));
            }
            if action == "close" {
                w.close();
                log_event(app, "action", &format!("fermé « {} »", w.title));
                format!("Voilà, « {} » est fermé ✅", w.title)
            } else {
                w.focus();
                log_event(app, "action", &format!("mis au premier plan : « {} »", w.title));
                format!("« {} » est au premier plan", w.title)
            }
        }
        _ => "Action inconnue".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn perms(rules: &[(&str, &str)]) -> Permissions {
        Permissions(rules.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect())
    }

    // ── niveau effectif ──

    #[test]
    fn no_rule_means_none() {
        assert_eq!(perms(&[]).level_of("chrome.exe", "Docs"), "none");
    }

    #[test]
    fn exact_rule_applies() {
        let p = perms(&[("chrome.exe::Docs", "look")]);
        assert_eq!(p.level_of("chrome.exe", "Docs"), "look");
        assert_eq!(p.level_of("chrome.exe", "Autre"), "none");
    }

    #[test]
    fn app_rule_covers_every_window_of_the_app() {
        let p = perms(&[("chrome.exe::*", "act")]);
        assert_eq!(p.level_of("chrome.exe", "N'importe quoi"), "act");
        assert_eq!(p.level_of("code.exe", "N'importe quoi"), "none");
    }

    #[test]
    fn exact_rule_beats_app_rule_unless_sanctuary() {
        let p = perms(&[("chrome.exe::*", "act"), ("chrome.exe::Docs", "look")]);
        assert_eq!(p.level_of("chrome.exe", "Docs"), "look");
    }

    #[test]
    fn sanctuary_on_window_wins_over_app_rule() {
        let p = perms(&[("chrome.exe::*", "trust"), ("chrome.exe::Banque", "sanctuary")]);
        assert_eq!(p.level_of("chrome.exe", "Banque"), "sanctuary");
        assert_eq!(p.level_of("chrome.exe", "Docs"), "trust");
    }

    #[test]
    fn sanctuary_on_app_wins_over_window_rule() {
        let p = perms(&[("keepass.exe::*", "sanctuary"), ("keepass.exe::Base", "act")]);
        assert_eq!(p.level_of("keepass.exe", "Base"), "sanctuary");
    }
}

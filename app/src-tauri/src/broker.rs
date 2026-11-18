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

// ═══════════════════ Router d'intentions déterministe ═══════════════════
// Le petit modèle local (3B) est peu fiable pour le tool-calling JSON.
// Les actions sont donc détectées par mots-clés ICI, et exécutées par
// le broker avec vérification des permissions. Le LLM = pure conversation.

/// Mots génériques qui désignent « une fenêtre » sans nommer l'application.
const STOPWORDS: [&str; 5] = ["fenêtre", "fenetre", "programme", "application", "app"];

pub(crate) fn detect_action(msg: &str) -> Option<(String, String)> {
    let m = msg.to_lowercase();

    // Lister
    for trigger in [
        "quelles app", "quelle app", "apps ouvertes", "applications ouvertes",
        "fenêtres ouvertes", "fenetres ouvertes", "liste des fenêtres",
        "liste des fenetres", "qui est ouvert", "ce qui est ouvert",
        "liste les fenêtres", "liste les fenetres", "liste les applications",
        "liste les apps", "quelles fenêtres", "quelles fenetres",
        "lister les fenêtres", "lister les fenetres",
    ] {
        if m.contains(trigger) {
            return Some(("list".into(), String::new()));
        }
    }

    // Fermer : "ferme X", "fermer la fenêtre X", "quitte X"
    for verb in ["ferme", "fermer", "quitte", "close"] {
        if let Some(t) =
            extract_target(&m, verb, &STOPWORDS)
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
            extract_target(&focus_msg, verb, &STOPWORDS)
        {
            return Some(("focus".into(), t));
        }
    }

    None
}

/// Position du verbe dans le message, en MOT ENTIER uniquement :
/// « ferme » ne doit pas se déclencher dans « la ferme », « fermer » ou
/// « referme » ; « active » pas dans « désactive » ; « affiche » pas dans
/// « affiche-moi » (le trait d'union lie le mot suivant).
fn find_word(msg: &str, verb: &str) -> Option<usize> {
    let is_word = |c: char| c.is_alphanumeric() || c == '-' || c == '_';
    msg.match_indices(verb).map(|(i, _)| i).find(|&i| {
        let before_ok = msg[..i].chars().next_back().map_or(true, |c| !is_word(c) && c != '\'');
        let after_ok = msg[i + verb.len()..].chars().next().map_or(true, |c| !is_word(c));
        // précédé d'un article, c'est un nom : « la ferme », « une active »
        let prev = msg[..i].split_whitespace().next_back().unwrap_or("");
        let is_noun = ["la", "le", "une", "un", "ma", "ta", "sa", "cette", "notre", "votre", "leur"]
            .contains(&prev);
        before_ok && after_ok && !is_noun
    })
}

/// Formules de politesse et ponctuation en fin de phrase, pas des cibles.
const TRAILING: [&str; 7] = ["stp", "svp", "s'il te plaît", "s'il te plait", "merci", "please", "pour moi"];

/// Extrait la cible après un verbe, en retirant les déterminants.
pub(crate) fn extract_target(msg: &str, verb: &str, stopwords: &[&str]) -> Option<String> {
    let idx = find_word(msg, verb)?;
    let mut after = msg[idx + verb.len()..]
        .trim()
        .trim_end_matches(['.', '!', '?', ',', ' '])
        .to_string();
    // « ferme chrome stp » -> « chrome »
    loop {
        let before = after.len();
        for t in TRAILING {
            if let Some(s) = after.strip_suffix(t) {
                after = s.trim_end_matches(['.', '!', '?', ',', ' ']).to_string();
            }
        }
        if after.len() == before {
            break;
        }
    }
    // « l'application » -> « l application » pour traiter chaque mot séparément
    let after = after.replace("l'", "l ");
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

pub(crate) fn execute_action(broker: &BrokerState, app: &AppHandle, action: &str, target: &str) -> String {
    let windows = enumerate_windows();
    match action {
        "list" => {
            // On ne révèle JAMAIS les sanctuaires, même à Julien
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
            let q = target.to_lowercase();
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
                format!("« {} » est au premier plan 🎯", w.title)
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

    // ── routeur d'intentions ──

    #[test]
    fn detects_list() {
        assert_eq!(detect_action("Quelles apps sont ouvertes ?"), Some(("list".into(), String::new())));
        assert_eq!(detect_action("liste des fenêtres stp"), Some(("list".into(), String::new())));
    }

    #[test]
    fn detects_close() {
        assert_eq!(detect_action("ferme chrome"), Some(("close".into(), "chrome".into())));
        assert_eq!(detect_action("Quitte spotify"), Some(("close".into(), "spotify".into())));
    }

    #[test]
    fn detects_focus() {
        assert_eq!(detect_action("mets chrome au premier plan"), Some(("focus".into(), "chrome".into())));
        assert_eq!(detect_action("affiche le bloc-notes"), Some(("focus".into(), "bloc-notes".into())));
    }

    #[test]
    fn plain_conversation_is_not_an_action() {
        assert_eq!(detect_action("bonjour, comment vas-tu ?"), None);
        assert_eq!(detect_action("raconte-moi une blague"), None);
    }

    #[test]
    fn extract_target_strips_determiners() {
        assert_eq!(extract_target("ferme le chrome", "ferme", &[]), Some("chrome".into()));
        assert_eq!(extract_target("ferme mon vlc", "ferme", &[]), Some("vlc".into()));
    }

    #[test]
    fn generic_nouns_are_not_the_target() {
        assert_eq!(detect_action("ferme la fenêtre notepad"), Some(("close".into(), "notepad".into())));
        assert_eq!(detect_action("ferme l'application spotify"), Some(("close".into(), "spotify".into())));
        assert_eq!(detect_action("mets le programme vlc au premier plan"), Some(("focus".into(), "vlc".into())));
    }

    #[test]
    fn verbs_match_whole_words_only() {
        // conversation ordinaire : aucun verbe d'action en mot entier
        assert_eq!(detect_action("j'ai visité la ferme de mon oncle"), None);
        assert_eq!(detect_action("tu me permets une question ?"), None);
        assert_eq!(detect_action("comment je désactive les notifications"), None);
        assert_eq!(detect_action("affiche-moi la météo"), None);
        // infinitifs : la cible ne doit pas garder un bout du verbe (« r chrome »)
        assert_eq!(detect_action("fermer chrome"), Some(("close".into(), "chrome".into())));
        assert_eq!(detect_action("peux-tu afficher spotify"), Some(("focus".into(), "spotify".into())));
    }

    #[test]
    fn list_accepts_common_phrasings() {
        for msg in ["liste les fenêtres", "tu peux lister les fenetres ?", "quelles fenêtres sont là", "liste les applications"] {
            assert_eq!(detect_action(msg), Some(("list".into(), String::new())), "{msg}");
        }
    }

    #[test]
    fn politeness_is_not_part_of_the_target() {
        assert_eq!(detect_action("ferme chrome stp"), Some(("close".into(), "chrome".into())));
        assert_eq!(detect_action("Ferme Spotify s'il te plaît !"), Some(("close".into(), "spotify".into())));
        assert_eq!(detect_action("mets vlc au premier plan, merci"), Some(("focus".into(), "vlc".into())));
    }

    #[test]
    fn extract_target_needs_a_target() {
        assert_eq!(extract_target("ferme", "ferme", &[]), None);
        assert_eq!(extract_target("ferme la", "ferme", &[]), None);
        assert_eq!(extract_target("bonjour", "ferme", &[]), None);
    }
}

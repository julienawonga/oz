//! Réglages de comportement d'Oz (onglet Réglages du panneau) :
//! transcription, prise de parole spontanée, langue, raccourci du micro.

use serde::{Deserialize, Serialize};
use std::fs;
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_global_shortcut::GlobalShortcutExt;

use crate::journal::log_event;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Prefs {
    /// La bulle affiche ce que disent Julien et Oz.
    #[serde(default = "yes")]
    pub show_transcript: bool,
    /// Oz peut engager la conversation de lui-même.
    #[serde(default = "yes")]
    pub initiative: bool,
    /// Langue parlée et écrite par Oz (code BCP-47, voir LANGUAGES).
    #[serde(default = "default_language")]
    pub language: String,
    /// Raccourci global qui ouvre le micro (vide = aucun).
    #[serde(default = "default_shortcut")]
    pub mic_shortcut: String,
    /// libre (se balade), fixe (ne bouge plus seul), bord (ancré au bord
    /// de l'écran), concentration (petit, discret, dans un coin, muet).
    #[serde(default = "default_move_mode")]
    pub move_mode: String,
    /// Derrière toutes les fenêtres (sinon au-dessus).
    #[serde(default)]
    pub behind: bool,
    /// Couleur de l'œuf (#rrggbb ; vide = bleu d'origine).
    #[serde(default)]
    pub egg_color: String,
    /// lisse, points, rayures, etoiles, taches.
    #[serde(default = "default_texture")]
    pub egg_texture: String,
}

pub(crate) const MOVE_MODES: &[&str] = &["libre", "fixe", "bord", "concentration"];
pub(crate) const TEXTURES: &[&str] = &["lisse", "points", "rayures", "etoiles", "taches"];

fn default_move_mode() -> String {
    "libre".into()
}
fn default_texture() -> String {
    "lisse".into()
}

fn yes() -> bool {
    true
}
fn default_language() -> String {
    "fr-FR".into()
}
fn default_shortcut() -> String {
    "Ctrl+Shift+Space".into()
}

impl Default for Prefs {
    fn default() -> Self {
        Self {
            show_transcript: true,
            initiative: true,
            language: default_language(),
            mic_shortcut: default_shortcut(),
            move_mode: default_move_mode(),
            behind: false,
            egg_color: String::new(),
            egg_texture: default_texture(),
        }
    }
}

/// Langues proposées : code, nom (tel qu'écrit dans les consignes du modèle).
pub(crate) const LANGUAGES: &[(&str, &str)] = &[
    ("fr-FR", "français"),
    ("en-US", "anglais"),
    ("es-ES", "espagnol"),
    ("de-DE", "allemand"),
    ("it-IT", "italien"),
    ("pt-BR", "portugais"),
    ("nl-NL", "néerlandais"),
];

pub struct PrefsState(pub Mutex<Prefs>);

fn prefs_path(app: &AppHandle) -> std::path::PathBuf {
    app.path().app_data_dir().expect("app_data_dir").join("prefs.json")
}

pub(crate) fn load_prefs(app: &AppHandle) -> Prefs {
    fs::read_to_string(prefs_path(app))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub(crate) fn current(app: &AppHandle) -> Prefs {
    app.state::<PrefsState>().0.lock().unwrap().clone()
}

/// Code de langue choisi (« fr-FR »).
pub(crate) fn language_code(app: &AppHandle) -> String {
    current(app).language
}

/// Nom de la langue choisie, pour les consignes (« français »).
pub(crate) fn language_name(app: &AppHandle) -> &'static str {
    let code = language_code(app);
    LANGUAGES.iter().find(|(c, _)| *c == code).map(|(_, n)| *n).unwrap_or("français")
}

/// (Ré)enregistre le raccourci global du micro.
pub(crate) fn apply_shortcut(app: &AppHandle, shortcut: &str) -> Result<(), String> {
    let gs = app.global_shortcut();
    let _ = gs.unregister_all();
    let shortcut = shortcut.trim();
    if shortcut.is_empty() {
        return Ok(());
    }
    gs.register(shortcut)
        .map_err(|e| format!("Raccourci « {shortcut} » refusé : {e}"))?;
    log_event(app, "prefs", &format!("raccourci du micro : {shortcut}"));
    Ok(())
}

#[tauri::command]
pub(crate) fn get_prefs(state: State<'_, PrefsState>) -> Prefs {
    state.0.lock().unwrap().clone()
}

#[tauri::command]
pub(crate) async fn set_prefs(app: AppHandle, prefs: Prefs) -> Result<(), String> {
    let old = current(&app);
    if !LANGUAGES.iter().any(|(c, _)| *c == prefs.language) {
        return Err(format!("Langue inconnue : {}", prefs.language));
    }
    if !MOVE_MODES.contains(&prefs.move_mode.as_str()) {
        return Err(format!("Mode de déplacement inconnu : {}", prefs.move_mode));
    }
    if !TEXTURES.contains(&prefs.egg_texture.as_str()) {
        return Err(format!("Texture inconnue : {}", prefs.egg_texture));
    }
    if !prefs.egg_color.is_empty() && !is_hex_color(&prefs.egg_color) {
        return Err(format!("Couleur invalide : {} (attendu #rrggbb)", prefs.egg_color));
    }
    if prefs.behind != old.behind {
        crate::tools::apply_layer(&app, prefs.behind)?;
    }
    if prefs.mic_shortcut != old.mic_shortcut {
        if let Err(e) = apply_shortcut(&app, &prefs.mic_shortcut) {
            let _ = apply_shortcut(&app, &old.mic_shortcut); // on garde l'ancien
            return Err(e);
        }
    }
    let path = prefs_path(&app);
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    fs::write(&path, serde_json::to_string_pretty(&prefs).unwrap()).map_err(|e| e.to_string())?;
    *app.state::<PrefsState>().0.lock().unwrap() = prefs.clone();
    if prefs.language != old.language {
        // la conversation vocale repart avec la nouvelle langue
        crate::voice::reset_talk_session(&app).await;
    }
    let _ = app.emit("oz://prefs-updated", prefs);
    Ok(())
}

pub(crate) fn is_hex_color(c: &str) -> bool {
    c.len() == 7 && c.starts_with('#') && c[1..].chars().all(|x| x.is_ascii_hexdigit())
}

/// Modifie les réglages depuis un outil (même chemin que le panneau).
pub(crate) async fn update(app: &AppHandle, f: impl FnOnce(&mut Prefs)) -> Result<(), String> {
    let mut p = current(app);
    f(&mut p);
    set_prefs(app.clone(), p).await
}

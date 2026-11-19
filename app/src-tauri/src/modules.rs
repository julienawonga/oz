//! Modules installables (à la façon des plugins d'Hermes Agent).
//!
//! Un module est un dossier `modules/<id>/` du dossier de config :
//!
//! ```text
//! module.json        nom, version, description, auteur, outils
//! *.ps1              un script PowerShell par outil
//! skills/<nom>/SKILL.md   skills livrés avec le module (facultatif)
//! ```
//!
//! Installation en deux temps, depuis le panneau : `prepare_module` copie
//! la source (dépôt git ou dossier local) dans `modules/.staging/` et
//! renvoie ce qu'elle contient (outils, code des scripts) ; rien ne tourne.
//! Julien relit puis `install_module` l'installe : le code exact des
//! scripts est alors approuvé. Un module installé devient un toolset
//! (activable dans l'onglet Outils) et ses outils sont déclarés tels quels
//! à Gemini. Si un script change après l'installation, il ne tourne plus
//! sans un nouvel accord.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Component, Path, PathBuf};
use tauri::{AppHandle, Manager};

use crate::journal::log_event;
use crate::toolsets::Toolset;

const MANIFEST: &str = "module.json";
const APPROVED: &str = ".approved.json";
const STAGING: &str = ".staging";

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ModuleTool {
    /// Nom de l'outil vu par Gemini (unique parmi tous les outils).
    pub name: String,
    /// Description pour Gemini : quand et comment s'en servir.
    pub description: String,
    /// Paramètres au format JSON Schema (objet), facultatif.
    #[serde(default)]
    pub parameters: Option<Value>,
    /// Script PowerShell, relatif au dossier du module.
    pub script: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Manifest {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub author: String,
    #[serde(default)]
    pub tools: Vec<ModuleTool>,
}

/// Ce que Julien relit avant d'installer.
#[derive(Serialize)]
pub struct Review {
    manifest: Manifest,
    /// Script -> code.
    scripts: BTreeMap<String, String>,
    skills: Vec<String>,
}

fn modules_dir(app: &AppHandle) -> PathBuf {
    let dir = app.path().app_data_dir().expect("app_data_dir").join("modules");
    let _ = fs::create_dir_all(&dir);
    dir
}

fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 40 && id.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

fn valid_tool_name(n: &str) -> bool {
    !n.is_empty() && n.len() <= 48 && n.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

/// Chemin relatif sans « .. » : un module ne lit rien hors de son dossier.
fn safe_rel(rel: &str) -> bool {
    let p = Path::new(rel);
    !rel.is_empty() && !p.is_absolute() && p.components().all(|c| matches!(c, Component::Normal(_)))
}

fn read_manifest(dir: &Path) -> Result<Manifest, String> {
    let text = fs::read_to_string(dir.join(MANIFEST)).map_err(|_| format!("{MANIFEST} introuvable"))?;
    let m: Manifest = serde_json::from_str(&text).map_err(|e| format!("{MANIFEST} illisible : {e}"))?;
    if !valid_id(&m.id) {
        return Err(format!("id « {} » invalide (minuscules, chiffres, tirets)", m.id));
    }
    if m.tools.is_empty() {
        return Err("le module ne déclare aucun outil".into());
    }
    for t in &m.tools {
        if !valid_tool_name(&t.name) {
            return Err(format!("nom d'outil « {} » invalide (minuscules, chiffres, _)", t.name));
        }
        if !safe_rel(&t.script) || !t.script.ends_with(".ps1") {
            return Err(format!("script « {} » invalide (un .ps1 dans le dossier du module)", t.script));
        }
        if !dir.join(&t.script).is_file() {
            return Err(format!("script « {} » absent", t.script));
        }
        if let Some(p) = &t.parameters {
            if p["type"] != "object" {
                return Err(format!("paramètres de « {} » : un JSON Schema de type object", t.name));
            }
        }
    }
    Ok(m)
}

fn scripts_of(dir: &Path, m: &Manifest) -> BTreeMap<String, String> {
    m.tools
        .iter()
        .map(|t| (t.script.clone(), fs::read_to_string(dir.join(&t.script)).unwrap_or_default()))
        .collect()
}

fn skills_of(dir: &Path) -> Vec<String> {
    fs::read_dir(dir.join("skills"))
        .map(|it| {
            it.flatten()
                .filter(|e| e.path().join("SKILL.md").is_file())
                .map(|e| e.file_name().to_string_lossy().to_string())
                .collect()
        })
        .unwrap_or_default()
}

/// Modules installés (hors préparation), avec leur dossier.
pub(crate) fn installed(app: &AppHandle) -> Vec<(Manifest, PathBuf)> {
    let Ok(entries) = fs::read_dir(modules_dir(app)) else { return Vec::new() };
    let mut out: Vec<(Manifest, PathBuf)> = entries
        .flatten()
        .filter(|e| e.file_name() != STAGING && e.path().is_dir())
        .filter_map(|e| read_manifest(&e.path()).ok().map(|m| (m, e.path())))
        .collect();
    out.sort_by(|a, b| a.0.name.cmp(&b.0.name));
    out
}

/// Un toolset par module installé.
pub(crate) fn toolsets(app: &AppHandle) -> Vec<Toolset> {
    installed(app)
        .into_iter()
        .map(|(m, _)| Toolset {
            id: format!("mod:{}", m.id),
            label: m.name.clone(),
            description: if m.description.is_empty() { format!("Module {} {}", m.name, m.version) } else { m.description.clone() },
            core: false,
            tools: m
                .tools
                .iter()
                .map(|t| {
                    let mut d = json!({"name": t.name, "description": t.description});
                    if let Some(p) = &t.parameters {
                        d["parameters"] = p.clone();
                    }
                    d
                })
                .collect(),
        })
        .collect()
}

/// Exécute un outil de module. None : pas un outil de module.
pub(crate) async fn run(app: &AppHandle, name: &str, args: &Value) -> Option<String> {
    let (m, dir) = installed(app).into_iter().find(|(m, _)| m.tools.iter().any(|t| t.name == name))?;
    let tool = m.tools.iter().find(|t| t.name == name)?.clone();
    let code = fs::read_to_string(dir.join(&tool.script)).unwrap_or_default();
    let approved: BTreeMap<String, String> = fs::read_to_string(dir.join(APPROVED))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default();
    if approved.get(&tool.script) != Some(&code) {
        // script modifié depuis l'installation : accord de Julien
        return Some(crate::kernel::ask_module_run(app, &m.id, &tool.name, &tool.script, &code, args));
    }
    log_event(app, "module", &format!("{} : {}", m.id, tool.name));
    Some(crate::kernel::run_script(app, &code, args, &dir, &tool.name).await)
}

/// Accord donné pour un script modifié : il devient le code approuvé.
pub(crate) fn approve_script(app: &AppHandle, module: &str, script: &str, code: &str) {
    let dir = modules_dir(app).join(module);
    let mut approved: BTreeMap<String, String> = fs::read_to_string(dir.join(APPROVED))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default();
    approved.insert(script.to_string(), code.to_string());
    let _ = fs::write(dir.join(APPROVED), serde_json::to_string_pretty(&approved).unwrap_or_default());
}

/// Dossiers de skills fournis par les modules installés.
pub(crate) fn skill_dirs(app: &AppHandle) -> Vec<(String, PathBuf)> {
    installed(app)
        .into_iter()
        .map(|(m, dir)| (m.id, dir.join("skills")))
        .filter(|(_, d)| d.is_dir())
        .collect()
}

fn copy_dir(from: &Path, to: &Path) -> std::io::Result<()> {
    fs::create_dir_all(to)?;
    for e in fs::read_dir(from)?.flatten() {
        let name = e.file_name();
        if name == ".git" {
            continue;
        }
        let path = e.path();
        if path.is_dir() {
            copy_dir(&path, &to.join(&name))?;
        } else {
            fs::copy(&path, to.join(&name))?;
        }
    }
    Ok(())
}

/// Étape 1 : récupère le module dans la zone de préparation et le décrit.
/// Rien n'est exécuté.
#[tauri::command]
pub(crate) async fn prepare_module(app: AppHandle, source: String) -> Result<Review, String> {
    let source = source.trim().to_string();
    let staging_root = modules_dir(&app).join(STAGING);
    let _ = fs::remove_dir_all(&staging_root);
    fs::create_dir_all(&staging_root).map_err(|e| e.to_string())?;
    let fetched = staging_root.join("source");
    if source.starts_with("https://") || source.starts_with("git@") {
        let out = tokio::time::timeout(
            std::time::Duration::from_secs(90),
            tokio::process::Command::new("git")
                .args(["clone", "--depth", "1", "--", &source])
                .arg(&fetched)
                .kill_on_drop(true)
                .creation_flags(0x0800_0000)
                .output(),
        )
        .await
        .map_err(|_| "git clone trop long (plus de 90 s)".to_string())?
        .map_err(|e| format!("git introuvable ({e})"))?;
        if !out.status.success() {
            return Err(format!("git clone a échoué : {}", String::from_utf8_lossy(&out.stderr).trim()));
        }
    } else {
        let from = Path::new(&source);
        if !from.is_dir() {
            return Err("Donne un lien git (https://…) ou le chemin d'un dossier.".into());
        }
        copy_dir(from, &fetched).map_err(|e| format!("copie impossible : {e}"))?;
    }
    let m = read_manifest(&fetched)?;
    // un nom d'outil ne doit pas en masquer un autre
    let existing: Vec<String> = crate::toolsets::all_tool_names(&app)
        .into_iter()
        .filter(|n| {
            !installed(&app)
                .iter()
                .any(|(im, _)| im.id == m.id && im.tools.iter().any(|t| &t.name == n))
        })
        .collect();
    if let Some(clash) = m.tools.iter().find(|t| existing.contains(&t.name)) {
        return Err(format!("l'outil « {} » existe déjà : renomme-le dans le module", clash.name));
    }
    let dest = staging_root.join(&m.id);
    fs::rename(&fetched, &dest).map_err(|e| e.to_string())?;
    log_event(&app, "module", &format!("préparé : {} {} ({source})", m.id, m.version));
    Ok(Review { scripts: scripts_of(&dest, &m), skills: skills_of(&dest), manifest: m })
}

/// Étape 2 : Julien a relu, le module est installé et ses scripts approuvés.
#[tauri::command]
pub(crate) async fn install_module(app: AppHandle, id: String) -> Result<(), String> {
    if !valid_id(&id) {
        return Err("Module invalide.".into());
    }
    let staged = modules_dir(&app).join(STAGING).join(&id);
    let m = read_manifest(&staged)?;
    let approved = scripts_of(&staged, &m);
    fs::write(staged.join(APPROVED), serde_json::to_string_pretty(&approved).unwrap_or_default())
        .map_err(|e| e.to_string())?;
    let dest = modules_dir(&app).join(&id);
    let _ = fs::remove_dir_all(&dest); // mise à jour : l'ancienne version part
    fs::rename(&staged, &dest).map_err(|e| e.to_string())?;
    let _ = fs::remove_dir_all(modules_dir(&app).join(STAGING));
    log_event(&app, "module", &format!("installé : {} {}", m.id, m.version));
    crate::voice::reset_talk_session(&app).await;
    Ok(())
}

#[tauri::command]
pub(crate) fn cancel_module(app: AppHandle) {
    let _ = fs::remove_dir_all(modules_dir(&app).join(STAGING));
}

#[tauri::command]
pub(crate) async fn uninstall_module(app: AppHandle, id: String) -> Result<(), String> {
    if !valid_id(&id) {
        return Err("Module invalide.".into());
    }
    fs::remove_dir_all(modules_dir(&app).join(&id)).map_err(|e| e.to_string())?;
    log_event(&app, "module", &format!("désinstallé : {id}"));
    crate::voice::reset_talk_session(&app).await;
    Ok(())
}

#[derive(Serialize)]
pub struct ModuleInfo {
    manifest: Manifest,
    skills: Vec<String>,
    /// Scripts modifiés depuis l'installation (accord redemandé).
    changed: Vec<String>,
}

#[tauri::command]
pub(crate) fn get_modules(app: AppHandle) -> Vec<ModuleInfo> {
    installed(&app)
        .into_iter()
        .map(|(m, dir)| {
            let approved: BTreeMap<String, String> = fs::read_to_string(dir.join(APPROVED))
                .ok()
                .and_then(|s| serde_json::from_str(&s).ok())
                .unwrap_or_default();
            let changed = scripts_of(&dir, &m)
                .into_iter()
                .filter(|(s, code)| approved.get(s) != Some(code))
                .map(|(s, _)| s)
                .collect();
            ModuleInfo { skills: skills_of(&dir), changed, manifest: m }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chemins_et_noms() {
        assert!(safe_rel("tools/min.ps1"));
        assert!(!safe_rel("../evil.ps1"));
        assert!(!safe_rel("C:\\Windows\\x.ps1"));
        assert!(valid_id("window-sizes"));
        assert!(!valid_id("Window Sizes"));
        assert!(valid_tool_name("minimize_window"));
        assert!(!valid_tool_name("minimize-window"));
    }

    #[test]
    fn module_exemple_valide() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/modules/window-sizes");
        let m = read_manifest(&dir).expect("le module d'exemple doit être valide");
        assert_eq!(m.id, "window-sizes");
        assert!(m.tools.len() >= 3);
    }
}

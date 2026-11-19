//! Registre des outils, rangés en toolsets (à la façon d'Hermes Agent).
//!
//! Chaque module d'Oz fournit ses toolsets (un groupe d'outils : leurs
//! déclarations pour Gemini) et sait exécuter ses outils. Le registre
//! les rassemble ; Julien active ou désactive chaque toolset dans le
//! panneau (onglet Outils). Gemini ne reçoit que les outils actifs : des
//! consignes plus courtes, et rien d'appelable qui ait été coupé.
//!
//! Ajouter un toolset au code d'Oz : écrire `toolsets()` et `run()` dans
//! le module, puis l'ajouter à `builtin` et à `dispatch` ci-dessous.
//! Les modules installables (modules.rs) s'ajoutent sans toucher au code.

use serde::Serialize;
use serde_json::{json, Value};
use tauri::AppHandle;

use crate::journal::log_event;

/// Un groupe d'outils activable d'un bloc.
pub(crate) struct Toolset {
    pub id: String,
    pub label: String,
    pub description: String,
    /// Toujours actif (sans lui, Oz ne peut plus finir une conversation).
    pub core: bool,
    pub tools: Vec<Value>,
}

/// Toolsets livrés avec Oz, module par module.
fn builtin() -> Vec<Toolset> {
    let mut all = crate::tools::toolsets();
    all.extend(crate::volume::toolsets());
    all.extend(crate::skills::toolsets());
    all.extend(crate::kernel::toolsets());
    all
}

/// Tous les toolsets : ceux d'Oz, puis un par module installé.
fn providers(app: &AppHandle) -> Vec<Toolset> {
    let mut all = builtin();
    all.extend(crate::modules::toolsets(app));
    all
}

/// Tous les noms d'outils (pour refuser un module qui en masquerait un).
pub(crate) fn all_tool_names(app: &AppHandle) -> Vec<String> {
    providers(app)
        .iter()
        .flat_map(|t| t.tools.iter().filter_map(|d| d["name"].as_str().map(String::from)))
        .collect()
}

fn is_enabled(app: &AppHandle, ts: &Toolset) -> bool {
    ts.core || !crate::prefs::current(app).disabled_toolsets.iter().any(|d| *d == ts.id)
}

/// Le toolset est-il actif ? (pour adapter les consignes)
pub(crate) fn enabled(app: &AppHandle, id: &str) -> bool {
    providers(app).iter().find(|t| t.id == id).is_some_and(|t| is_enabled(app, t))
}

/// Déclarations envoyées à Gemini (champ `tools`) : les toolsets actifs.
pub(crate) fn declarations(app: &AppHandle) -> Value {
    let decls: Vec<Value> = providers(app)
        .into_iter()
        .filter(|t| is_enabled(app, t))
        .flat_map(|t| t.tools)
        .collect();
    json!([{ "functionDeclarations": decls }])
}

/// Exécute un outil : refusé s'il appartient à un toolset coupé.
pub(crate) async fn dispatch(app: &AppHandle, name: &str, args: &Value) -> String {
    let owner = providers(app).into_iter().find(|t| t.tools.iter().any(|d| d["name"] == name));
    match owner {
        None => return format!("Outil inconnu : {name}"),
        Some(t) if !is_enabled(app, &t) => {
            log_event(app, "tool", &format!("{name} refusé : toolset « {} » désactivé", t.id));
            return format!("Cet outil est désactivé par Julien (groupe « {} »). Dis-le-lui simplement.", t.label);
        }
        Some(_) => {}
    }
    if let Some(out) = crate::tools::run_builtin(app, name, args).await {
        return out;
    }
    if let Some(out) = crate::volume::run(app, name, args) {
        return out;
    }
    if let Some(out) = crate::skills::run(app, name, args) {
        return out;
    }
    if let Some(out) = crate::kernel::run(app, name, args).await {
        return out;
    }
    if let Some(out) = crate::modules::run(app, name, args).await {
        return out;
    }
    format!("Outil inconnu : {name}")
}

// ── Panneau ──

#[derive(Serialize)]
pub struct ToolsetInfo {
    id: String,
    label: String,
    description: String,
    core: bool,
    enabled: bool,
    tools: Vec<String>,
}

#[tauri::command]
pub(crate) fn get_toolsets(app: AppHandle) -> Vec<ToolsetInfo> {
    providers(&app)
        .into_iter()
        .map(|t| ToolsetInfo {
            enabled: is_enabled(&app, &t),
            id: t.id,
            label: t.label,
            description: t.description,
            core: t.core,
            tools: t.tools.iter().filter_map(|d| d["name"].as_str().map(String::from)).collect(),
        })
        .collect()
}

/// Active ou coupe un toolset. La conversation vocale repart, car Gemini
/// Live fixe ses outils à l'ouverture de la session.
#[tauri::command]
pub(crate) async fn set_toolset_enabled(app: AppHandle, id: String, enabled: bool) -> Result<(), String> {
    let Some(ts) = providers(&app).into_iter().find(|t| t.id == id) else {
        return Err(format!("Toolset inconnu : {id}"));
    };
    if ts.core && !enabled {
        return Err(format!("« {} » ne peut pas être coupé.", ts.label));
    }
    crate::prefs::update(&app, |p| {
        p.disabled_toolsets.retain(|d| d != &id);
        if !enabled {
            p.disabled_toolsets.push(id.clone());
        }
    })
    .await?;
    log_event(&app, "toolset", &format!("{} {}", if enabled { "activé" } else { "coupé" }, ts.label));
    crate::voice::reset_talk_session(&app).await;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn noms_uniques_et_rattaches() {
        let all = builtin();
        let mut names: Vec<String> = all
            .iter()
            .flat_map(|t| t.tools.iter().map(|d| d["name"].as_str().unwrap_or("").to_string()))
            .collect();
        let n = names.len();
        names.sort();
        names.dedup();
        assert_eq!(n, names.len(), "un outil déclaré deux fois");
        assert!(names.iter().all(|s| !s.is_empty()));
        assert!(all.iter().any(|t| t.core), "il faut au moins un toolset de base");
        let mut ids: Vec<&str> = all.iter().map(|t| t.id.as_str()).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), all.len(), "deux toolsets ont le même id");
    }
}

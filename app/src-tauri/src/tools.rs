//! Outils du cerveau : Gemini comprend la demande de Julien, quelle que
//! soit la formulation, et appelle ces outils. Aucun mot-clé.
//!
//! La sécurité ne dépend PAS du modèle : chaque outil qui touche une
//! fenêtre passe par le broker, qui applique les permissions dans le code
//! (sanctuaire invisible, demande d'accord sinon).

use serde_json::{json, Value};
use tauri::{AppHandle, Emitter, Manager};

use crate::brain::BrainState;
use crate::broker::{execute_action, BrokerState};
use crate::journal::log_event;
use crate::zones::{Zone, ZonesState};

/// Déclarations envoyées à Gemini (champ `tools`).
pub(crate) fn declarations() -> Value {
    json!([{ "functionDeclarations": [
        {
            "name": "lister_fenetres",
            "description": "Liste les applications et fenêtres ouvertes sur l'écran de Julien."
        },
        {
            "name": "fermer_fenetre",
            "description": "Ferme une fenêtre ou une application de Julien. À n'utiliser que s'il le demande clairement.",
            "parameters": {"type": "object", "properties": {
                "cible": {"type": "string", "description": "Nom de l'application ou mot du titre de la fenêtre, par exemple chrome, spotify, bloc-notes."}
            }, "required": ["cible"]}
        },
        {
            "name": "mettre_au_premier_plan",
            "description": "Ramène une fenêtre ou une application au premier plan.",
            "parameters": {"type": "object", "properties": {
                "cible": {"type": "string", "description": "Nom de l'application ou mot du titre de la fenêtre."}
            }, "required": ["cible"]}
        },
        {
            "name": "regarder_fenetre",
            "description": "Fait une capture d'une fenêtre et la décrit. Sert dès que Julien demande ce qu'il y a sur son écran, ce que tu vois, ou de l'aide sur ce qu'il fait.",
            "parameters": {"type": "object", "properties": {
                "cible": {"type": "string", "description": "Application ou mot du titre à regarder. Vide pour la fenêtre au premier plan."},
                "question": {"type": "string", "description": "Ce qu'il faut chercher ou expliquer dans la capture."}
            }}
        },
        {
            "name": "se_deplacer",
            "description": "Déplace Oz (toi, le petit œuf) sur l'écran.",
            "parameters": {"type": "object", "properties": {
                "direction": {"type": "string", "enum": ["gauche", "droite", "haut", "bas", "centre", "haut gauche", "haut droite", "bas gauche", "bas droite"]}
            }, "required": ["direction"]}
        },
        {
            "name": "changer_pose",
            "description": "Change la posture d'Oz.",
            "parameters": {"type": "object", "properties": {
                "pose": {"type": "string", "enum": ["assis", "debout"]}
            }, "required": ["pose"]}
        },
        {
            "name": "zone_interdite",
            "description": "Gère les zones de l'écran où Oz n'a pas le droit d'aller. interdire_ici : l'endroit où se trouve Oz devient interdit. autoriser_ici : retire la zone à cet endroit. compter : combien de zones existent.",
            "parameters": {"type": "object", "properties": {
                "operation": {"type": "string", "enum": ["interdire_ici", "autoriser_ici", "compter"]}
            }, "required": ["operation"]}
        }
    ]}])
}

/// Exécute un appel d'outil et renvoie le résultat (texte lu par le modèle).
pub(crate) async fn run(app: &AppHandle, name: &str, args: &Value) -> String {
    log_event(app, "tool", &format!("{name} {args}"));
    let result = dispatch(app, name, args).await;
    // le résultat aussi : sans lui, un outil qui échoue reste invisible
    log_event(app, "tool", &format!("{name} -> {}", result.chars().take(160).collect::<String>()));
    result
}

async fn dispatch(app: &AppHandle, name: &str, args: &Value) -> String {
    let arg = |k: &str| args[k].as_str().unwrap_or("").trim().to_string();
    match name {
        "lister_fenetres" => {
            execute_action(&app.state::<BrokerState>(), app, "list", "")
        }
        "fermer_fenetre" => {
            execute_action(&app.state::<BrokerState>(), app, "close", &arg("cible").to_lowercase())
        }
        "mettre_au_premier_plan" => {
            execute_action(&app.state::<BrokerState>(), app, "focus", &arg("cible").to_lowercase())
        }
        "regarder_fenetre" => {
            let question = match arg("question") {
                q if q.is_empty() => "Décris ce que tu vois sur cette capture d'écran.".to_string(),
                q => q,
            };
            match crate::vision::look_at(
                app.state::<BrainState>(),
                app.state::<BrokerState>(),
                app.clone(),
                arg("cible").to_lowercase(),
                question,
            )
            .await
            {
                Ok(desc) => desc,
                Err(e) => e,
            }
        }
        "se_deplacer" => {
            let _ = app.emit("oz://do", json!({"action": "move", "direction": arg("direction")}));
            "Oz se déplace.".into()
        }
        "changer_pose" => {
            let _ = app.emit("oz://do", json!({"action": "pose", "pose": arg("pose")}));
            "Pose changée.".into()
        }
        "zone_interdite" => zone(app, &arg("operation")),
        other => format!("Outil inconnu : {other}"),
    }
}

/// Zones interdites autour de la position actuelle d'Oz.
fn zone(app: &AppHandle, op: &str) -> String {
    let state = app.state::<ZonesState>();
    let mut zones = state.0.lock().unwrap().clone();
    if op == "compter" {
        return format!("{} zone(s) interdite(s).", zones.len());
    }
    let Some(win) = app.get_webview_window("oz") else {
        return "Fenêtre d'Oz introuvable.".into();
    };
    let (Ok(pos), Ok(size)) = (win.outer_position(), win.outer_size()) else {
        return "Position d'Oz illisible.".into();
    };
    let (x, y, w, h) = (pos.x as f64, pos.y as f64, size.width as f64, size.height as f64);
    let before = zones.len();
    match op {
        "interdire_ici" => zones.push(Zone { x, y, w, h, label: "interdit".into() }),
        "autoriser_ici" => zones.retain(|z| !(x < z.x + z.w && x + w > z.x && y < z.y + z.h && y + h > z.y)),
        _ => return format!("Opération inconnue : {op}"),
    }
    if let Err(e) = crate::zones::set_zones(app.clone(), state, zones.clone()) {
        return format!("Impossible d'enregistrer les zones : {e}");
    }
    match op {
        "interdire_ici" => "Zone interdite ajoutée à l'endroit où se trouve Oz.".into(),
        _ if zones.len() < before => "Zone retirée.".into(),
        _ => "Il n'y avait pas de zone à cet endroit.".into(),
    }
}

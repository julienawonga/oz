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
use crate::toolsets::Toolset;
use crate::zones::{Zone, ZonesState};

/// Toolsets natifs d'Oz (fenêtres, vision, corps, expression,
/// conversation). Le registre est dans `toolsets.rs`.
pub(crate) fn toolsets() -> Vec<Toolset> {
    vec![
        Toolset {
            id: "windows",
            label: "Fenêtres",
            description: "Lister, fermer et ramener tes fenêtres, toujours derrière les permissions.",
            core: false,
            tools: vec![
            json!({
                "name": "list_windows",
                "description": "Liste les applications et fenêtres ouvertes sur l'écran de Julien."
            }),
            json!({
                "name": "close_window",
                "description": "Ferme une fenêtre ou une application de Julien. À n'utiliser que s'il le demande clairement.",
                "parameters": {"type": "object", "properties": {
                    "target": {"type": "string", "description": "Nom de l'application ou mot du titre de la fenêtre, par exemple chrome, spotify, bloc-notes."}
                }, "required": ["target"]}
            }),
            json!({
                "name": "focus_window",
                "description": "Ramène une fenêtre ou une application au premier plan.",
                "parameters": {"type": "object", "properties": {
                    "target": {"type": "string", "description": "Nom de l'application ou mot du titre de la fenêtre."}
                }, "required": ["target"]}
            }),
            ],
        },
        Toolset {
            id: "vision",
            label: "Vision",
            description: "Regarder une fenêtre autorisée, ou la suivre en direct pendant que tu parles.",
            core: false,
            tools: vec![
            json!({
                "name": "look_at_window",
                "description": "Fait une capture d'une fenêtre et la décrit. Sert dès que Julien demande ce qu'il y a sur son écran, ce que tu vois, ou de l'aide sur ce qu'il fait. Inutile pour la fenêtre que tu regardes déjà en direct (watch_with_me) : tu la vois.",
                "parameters": {"type": "object", "properties": {
                    "target": {"type": "string", "description": "Application ou mot du titre à regarder. Vide pour la fenêtre au premier plan."},
                    "question": {"type": "string", "description": "Ce qu'il faut chercher ou expliquer dans la capture."}
                }}
            }),
            json!({
                "name": "watch_with_me",
                "description": "Vue en direct : tu vois une fenêtre en continu pendant que Julien te parle, pour suivre ce qu'il fait et réagir tout de suite. commencer : quand il te demande de regarder avec lui, de suivre ce qu'il fait, de garder un œil sur une fenêtre. arreter : quand il te demande d'arrêter de regarder. Une seule fenêtre à la fois.",
                "parameters": {"type": "object", "properties": {
                    "action": {"type": "string", "enum": ["commencer", "arreter"]},
                    "target": {"type": "string", "description": "Application ou mot du titre. Vide pour la fenêtre que Julien utilise."}
                }, "required": ["action"]}
            }),
            ],
        },
        Toolset {
            id: "body",
            label: "Corps",
            description: "Se déplacer, s'asseoir, faire des figures, lieux nommés, zones interdites, plan et mode de déplacement.",
            core: false,
            tools: vec![
            json!({
                "name": "move",
                "description": "Déplace Oz (toi, le petit œuf) sur l'écran.",
                "parameters": {"type": "object", "properties": {
                    "direction": {"type": "string", "enum": ["gauche", "droite", "haut", "bas", "centre", "haut gauche", "haut droite", "bas gauche", "bas droite"]}
                }, "required": ["direction"]}
            }),
            json!({
                "name": "set_pose",
                "description": "Change la posture d'Oz.",
                "parameters": {"type": "object", "properties": {
                    "pose": {"type": "string", "enum": ["assis", "debout"]}
                }, "required": ["pose"]}
            }),
            json!({
                "name": "do_trick",
                "description": "Oz (toi, le petit œuf) fait une figure pour s'amuser ou faire le spectacle. Trajets sur l'écran : zigzag, cercle, huit, vague, spirale. Sur place : toupie (tourne sur lui-même), salto, demi_tour (se retourne, montre son dos puis revient), saut. danse : un enchaînement de plusieurs figures. Avec « vers », Oz va à cet endroit en faisant la figure (« va en bas à droite en zigzag »). Tu peux appeler l'outil plusieurs fois pour enchaîner.",
                "parameters": {"type": "object", "properties": {
                    "figure": {"type": "string", "enum": ["zigzag", "cercle", "huit", "vague", "spirale", "toupie", "salto", "demi_tour", "saut", "danse"]},
                    "towards": {"type": "string", "description": "Destination facultative : une direction (gauche, droite, haut, bas, centre, haut gauche, haut droite, bas gauche, bas droite) ou le nom d'un lieu enregistré."}
                }, "required": ["figure"]}
            }),
            json!({
                "name": "places",
                "description": "Lieux nommés de l'écran, propres à Oz et Julien (« ta maison », « le travail »). nommer_ici : donne ce nom à l'endroit où se trouve Oz maintenant (si Julien nomme un endroit où tu n'es pas, va d'abord y aller avec move). aller : Oz glisse jusqu'au lieu. lister : les lieux connus. oublier : supprime un lieu.",
                "parameters": {"type": "object", "properties": {
                    "operation": {"type": "string", "enum": ["nommer_ici", "aller", "lister", "oublier"]},
                    "name": {"type": "string", "description": "Nom du lieu, par exemple maison, travail."}
                }, "required": ["operation"]}
            }),
            json!({
                "name": "no_go_zone",
                "description": "Gère les zones de l'écran où Oz n'a pas le droit d'aller. interdire_ici : l'endroit où se trouve Oz devient interdit. autoriser_ici : retire la zone à cet endroit. compter : combien de zones existent.",
                "parameters": {"type": "object", "properties": {
                    "operation": {"type": "string", "enum": ["interdire_ici", "autoriser_ici", "compter"]}
                }, "required": ["operation"]}
            }),
            json!({
                "name": "set_layer",
                "description": "Où Oz se place par rapport aux fenêtres. derriere : Oz passe derrière toutes les fenêtres, sur le bureau (quand Julien te demande de te mettre en arrière-plan, derrière les fenêtres, de ne plus le gêner). devant : Oz repasse au-dessus de toutes les fenêtres.",
                "parameters": {"type": "object", "properties": {
                    "layer": {"type": "string", "enum": ["devant", "derriere"]}
                }, "required": ["layer"]}
            }),
            json!({
                "name": "set_movement_mode",
                "description": "Comment Oz se déplace de lui-même. libre : il se balade sur l'écran. fixe : il reste où il est. bord : il se pose au bord de l'écran le plus proche et y reste. concentration : il se fait petit et discret dans un coin, ne bouge plus et ne prend plus la parole de lui-même (quand Julien veut se concentrer, travailler tranquille).",
                "parameters": {"type": "object", "properties": {
                    "mode": {"type": "string", "enum": ["libre", "fixe", "bord", "concentration"]}
                }, "required": ["mode"]}
            }),
            ],
        },
        Toolset {
            id: "expression",
            label: "Expression",
            description: "Émotions sur son visage et changement d'apparence.",
            core: false,
            tools: vec![
            json!({
                "name": "express_emotion",
                "description": "Montre ce que tu ressens sur ton visage, avec des bulles qui s'envolent de toi. Appelle-le EN MÊME TEMPS que ta réponse quand tu ressens quelque chose de net (tu ris, tu es gêné, fâché pour de faux, fier, attendri, surpris...), pas à chaque phrase.",
                "parameters": {"type": "object", "properties": {
                    "emotion": {"type": "string", "enum": ["joie", "rire", "colere", "tristesse", "surprise", "amour", "douceur", "gene", "reflexion", "ennui", "fierte", "malice"]},
                    "intensity": {"type": "integer", "description": "1 léger, 2 net, 3 fort"}
                }, "required": ["emotion"]}
            }),
            json!({
                "name": "customize_egg",
                "description": "Change l'apparence d'Oz : sa couleur et/ou sa texture, quand Julien le demande. Traduis la couleur demandée en code #rrggbb (rose : #f472b6, vert : #4ade80, violet : #a78bfa, orange : #fb923c, rouge : #f87171, jaune : #facc15...) ; « origine » remet le bleu d'origine.",
                "parameters": {"type": "object", "properties": {
                    "color": {"type": "string", "description": "#rrggbb, ou « origine »"},
                    "texture": {"type": "string", "enum": ["lisse", "points", "rayures", "etoiles", "taches"]}
                }}
            }),
            ],
        },
        Toolset {
            id: "conversation",
            label: "Conversation",
            description: "Finir une conversation, se taire un moment. Toujours actif.",
            core: true,
            tools: vec![
            json!({
                "name": "end_conversation",
                "description": "Julien met fin à la conversation (« tais-toi », « c'est fini », « laisse tomber », « à plus », « merci c'est tout ») : tu dis un mot très bref et tu arrêtes de l'écouter. Il rouvrira la conversation en cliquant sur l'œuf."
            }),
            json!({
                "name": "not_now",
                "description": "Julien ne veut pas être dérangé : Oz ne prendra plus la parole de lui-même pendant un moment, et la conversation en cours s'arrête. À utiliser quand il dit qu'il est occupé, concentré, en réunion, ou te demande de le laisser tranquille un moment.",
                "parameters": {"type": "object", "properties": {
                    "minutes": {"type": "integer", "description": "Durée du silence demandée, 60 par défaut."}
                }}
            }),
            ],
        }
    ]
}

/// Déclarations envoyées à Gemini : les toolsets actifs.
pub(crate) fn declarations(app: &AppHandle) -> Value {
    crate::toolsets::declarations(app)
}

/// Exécute un appel d'outil et renvoie le résultat (texte lu par le modèle).
pub(crate) async fn run(app: &AppHandle, name: &str, args: &Value) -> String {
    log_event(app, "tool", &format!("{name} {args}"));
    let result = guarded(app, name, args).await;
    // le résultat aussi : sans lui, un outil qui échoue reste invisible
    log_event(app, "tool", &format!("{name} -> {}", result.chars().take(160).collect::<String>()));
    result
}

/// Garde-fous communs, puis le registre (toolset actif ?) et l'outil.
async fn guarded(app: &AppHandle, name: &str, args: &Value) -> String {
    // 🔐 après « arrête de regarder », le code bloque tout regard
    let action = args["action"].as_str().unwrap_or("");
    let wants_look = name == "look_at_window" || (name == "watch_with_me" && action != "arreter");
    if wants_look && crate::watch::look_paused() {
        log_event(app, "watch", &format!("regard bloqué pendant la pause : {name}"));
        return crate::kernel::ask_look(app, name, args);
    }
    crate::toolsets::dispatch(app, name, args).await
}

/// Outils natifs. None : pas un outil de ce module.
pub(crate) async fn run_builtin(app: &AppHandle, name: &str, args: &Value) -> Option<String> {
    let arg = |k: &str| args[k].as_str().unwrap_or("").trim().to_string();
    let out: String = match name {
        "list_windows" => {
            execute_action(&app.state::<BrokerState>(), app, "list", "")
        }
        "close_window" => {
            execute_action(&app.state::<BrokerState>(), app, "close", &arg("target").to_lowercase())
        }
        "focus_window" => {
            execute_action(&app.state::<BrokerState>(), app, "focus", &arg("target").to_lowercase())
        }
        "look_at_window" => {
            let question = match arg("question") {
                q if q.is_empty() => "Décris ce que tu vois sur cette capture d'écran.".to_string(),
                q => q,
            };
            match crate::vision::look_at(
                app.state::<BrainState>(),
                app.clone(),
                arg("target").to_lowercase(),
                question,
            )
            .await
            {
                Ok(desc) => desc,
                Err(e) => e,
            }
        }
        "move" => {
            let _ = app.emit("oz://do", json!({"action": "move", "direction": arg("direction")}));
            "Oz se déplace.".into()
        }
        "do_trick" => {
            let vers = arg("towards");
            let mut ev = json!({"action": "figure", "figure": arg("figure")});
            if !vers.is_empty() {
                // un lieu nommé, sinon une direction
                match crate::places::find(app, &vers) {
                    Some(p) => {
                        ev["x"] = json!(p.x);
                        ev["y"] = json!(p.y);
                    }
                    None => ev["direction"] = json!(vers),
                }
            }
            let _ = app.emit("oz://do", ev);
            "Figure lancée.".into()
        }
        "places" => crate::places::tool(app, &arg("operation"), &arg("name")),
        "watch_with_me" => crate::watch::tool(app, &arg("action"), &arg("target").to_lowercase()),
        "set_pose" => {
            let _ = app.emit("oz://do", json!({"action": "pose", "pose": arg("pose")}));
            "Pose changée.".into()
        }
        "express_emotion" => {
            let _ = app.emit("oz://do", json!({
                "action": "emotion",
                "emotion": arg("emotion"),
                "intensity": args["intensity"].as_i64().unwrap_or(2)
            }));
            "Montré.".into()
        }
        "set_layer" => {
            let behind = arg("layer") == "derriere";
            match crate::prefs::update(app, |p| p.behind = behind).await {
                Ok(()) if behind => "Oz est maintenant derrière toutes les fenêtres, sur le bureau (gardé au redémarrage).".into(),
                Ok(()) => "Oz est de nouveau au-dessus de toutes les fenêtres.".into(),
                Err(e) => format!("Impossible de changer de plan ({e})."),
            }
        }
        "set_movement_mode" => {
            let mode = arg("mode");
            match crate::prefs::update(app, |p| p.move_mode = mode.clone()).await {
                Ok(()) => match mode.as_str() {
                    "libre" => "Mode libre : tu te balades à nouveau sur l'écran.".into(),
                    "fixe" => "Mode fixe : tu ne bouges plus de toi-même ; Julien peut toujours te déplacer.".into(),
                    "bord" => "Mode ancré : tu te poses au bord de l'écran le plus proche et tu y restes.".into(),
                    _ => "Mode concentration : tu te fais petit dans un coin, tu ne bouges plus et tu ne prends plus la parole de toi-même. Dis-le en trois mots.".into(),
                },
                Err(e) => format!("Impossible : {e}"),
            }
        }
        "customize_egg" => {
            let couleur = arg("color");
            let texture = arg("texture");
            if !couleur.is_empty() && couleur != "origine" && !crate::prefs::is_hex_color(&couleur) {
                return Some(format!("Couleur « {couleur} » invalide : donne-la en #rrggbb."));
            }
            match crate::prefs::update(app, |p| {
                if couleur == "origine" {
                    p.egg_color.clear();
                } else if !couleur.is_empty() {
                    p.egg_color = couleur.to_lowercase();
                }
                if !texture.is_empty() {
                    p.egg_texture = texture.clone();
                }
            })
            .await
            {
                Ok(()) => "C'est fait, ton apparence a changé.".into(),
                Err(e) => format!("Impossible : {e}"),
            }
        }
        "no_go_zone" => zone(app, &arg("operation")),
        // fin de conversation : la vue en direct s'arrête aussi
        "end_conversation" => {
            let _ = app.emit("oz://do", json!({"action": "end"}));
            stop_watching(app);
            "Conversation terminée, le micro se coupe. Si tu as déjà dit au revoir, ne dis plus rien ; sinon, un seul mot.".into()
        }
        "not_now" => {
            let _ = app.emit("oz://do", json!({"action": "end"}));
            stop_watching(app);
            crate::initiative::snooze(app, args["minutes"].as_u64().unwrap_or(60))
        }
        _ => return None,
    };
    Some(out)
}

/// Place de l'œuf dans sa fenêtre, en pixels logiques (voir #stage et
/// #robot-body dans styles.css : fenêtre 360×720, œuf 92×106 centré,
/// 260 px sous lui).
const EGG_BOX: (f64, f64, f64, f64) = (134.0, 354.0, 92.0, 106.0);

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
    let (Ok(pos), Ok(scale)) = (win.outer_position(), win.scale_factor()) else {
        return "Position d'Oz illisible.".into();
    };
    // la zone couvre l'œuf, pas toute sa fenêtre (qui garde de la place
    // pour la bulle au-dessus et en dessous)
    let (ex, ey, ew, eh) = EGG_BOX;
    let (x, y, w, h) = (pos.x as f64 + ex * scale, pos.y as f64 + ey * scale, ew * scale, eh * scale);
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

/// Arrête « Regarde avec moi » s'il est en cours (fin de conversation :
/// la pause du regard repart aussi de zéro).
fn stop_watching(app: &AppHandle) {
    crate::watch::clear_look_pause();
    if crate::watch::current().is_some() {
        crate::watch::stop(app, "fin de la conversation");
    }
}

/// Devant toutes les fenêtres (par défaut) ou derrière, sur le bureau.
pub(crate) fn apply_layer(app: &AppHandle, behind: bool) -> Result<(), String> {
    let win = app.get_webview_window("oz").ok_or("Fenêtre d'Oz introuvable.")?;
    let done = if behind {
        win.set_always_on_top(false).and_then(|_| win.set_always_on_bottom(true))
    } else {
        win.set_always_on_bottom(false).and_then(|_| win.set_always_on_top(true))
    };
    done.map_err(|e| e.to_string())?;
    log_event(app, "layer", if behind { "derrière les fenêtres" } else { "devant les fenêtres" });
    Ok(())
}

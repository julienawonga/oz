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
    let mut decl = json!([{ "functionDeclarations": [
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
            "description": "Fait une capture d'une fenêtre et la décrit. Sert dès que Julien demande ce qu'il y a sur son écran, ce que tu vois, ou de l'aide sur ce qu'il fait. Inutile pour la fenêtre que tu regardes déjà en direct (regarder_avec_moi) : tu la vois.",
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
            "name": "mode_deplacement",
            "description": "Comment Oz se déplace de lui-même. libre : il se balade sur l'écran. fixe : il reste où il est. bord : il se pose au bord de l'écran le plus proche et y reste. concentration : il se fait petit et discret dans un coin, ne bouge plus et ne prend plus la parole de lui-même (quand Julien veut se concentrer, travailler tranquille).",
            "parameters": {"type": "object", "properties": {
                "mode": {"type": "string", "enum": ["libre", "fixe", "bord", "concentration"]}
            }, "required": ["mode"]}
        },
        {
            "name": "personnaliser_oeuf",
            "description": "Change l'apparence d'Oz : sa couleur et/ou sa texture, quand Julien le demande. Traduis la couleur demandée en code #rrggbb (rose : #f472b6, vert : #4ade80, violet : #a78bfa, orange : #fb923c, rouge : #f87171, jaune : #facc15...) ; « origine » remet le bleu d'origine.",
            "parameters": {"type": "object", "properties": {
                "couleur": {"type": "string", "description": "#rrggbb, ou « origine »"},
                "texture": {"type": "string", "enum": ["lisse", "points", "rayures", "etoiles", "taches"]}
            }}
        },
        {
            "name": "changer_plan",
            "description": "Où Oz se place par rapport aux fenêtres. derriere : Oz passe derrière toutes les fenêtres, sur le bureau (quand Julien te demande de te mettre en arrière-plan, derrière les fenêtres, de ne plus le gêner). devant : Oz repasse au-dessus de toutes les fenêtres.",
            "parameters": {"type": "object", "properties": {
                "plan": {"type": "string", "enum": ["devant", "derriere"]}
            }, "required": ["plan"]}
        },
        {
            "name": "zone_interdite",
            "description": "Gère les zones de l'écran où Oz n'a pas le droit d'aller. interdire_ici : l'endroit où se trouve Oz devient interdit. autoriser_ici : retire la zone à cet endroit. compter : combien de zones existent.",
            "parameters": {"type": "object", "properties": {
                "operation": {"type": "string", "enum": ["interdire_ici", "autoriser_ici", "compter"]}
            }, "required": ["operation"]}
        },
        {
            "name": "faire_une_figure",
            "description": "Oz (toi, le petit œuf) fait une figure pour s'amuser ou faire le spectacle. Trajets sur l'écran : zigzag, cercle, huit, vague, spirale. Sur place : toupie (tourne sur lui-même), salto, demi_tour (se retourne, montre son dos puis revient), saut. danse : un enchaînement de plusieurs figures. Avec « vers », Oz va à cet endroit en faisant la figure (« va en bas à droite en zigzag »). Tu peux appeler l'outil plusieurs fois pour enchaîner.",
            "parameters": {"type": "object", "properties": {
                "figure": {"type": "string", "enum": ["zigzag", "cercle", "huit", "vague", "spirale", "toupie", "salto", "demi_tour", "saut", "danse"]},
                "vers": {"type": "string", "description": "Destination facultative : une direction (gauche, droite, haut, bas, centre, haut gauche, haut droite, bas gauche, bas droite) ou le nom d'un lieu enregistré."}
            }, "required": ["figure"]}
        },
        {
            "name": "lieux",
            "description": "Lieux nommés de l'écran, propres à Oz et Julien (« ta maison », « le travail »). nommer_ici : donne ce nom à l'endroit où se trouve Oz maintenant (si Julien nomme un endroit où tu n'es pas, va d'abord y aller avec se_deplacer). aller : Oz glisse jusqu'au lieu. lister : les lieux connus. oublier : supprime un lieu.",
            "parameters": {"type": "object", "properties": {
                "operation": {"type": "string", "enum": ["nommer_ici", "aller", "lister", "oublier"]},
                "nom": {"type": "string", "description": "Nom du lieu, par exemple maison, travail."}
            }, "required": ["operation"]}
        },
        {
            "name": "regarder_avec_moi",
            "description": "Vue en direct : tu vois une fenêtre en continu pendant que Julien te parle, pour suivre ce qu'il fait et réagir tout de suite. commencer : quand il te demande de regarder avec lui, de suivre ce qu'il fait, de garder un œil sur une fenêtre. arreter : quand il te demande d'arrêter de regarder. Une seule fenêtre à la fois.",
            "parameters": {"type": "object", "properties": {
                "action": {"type": "string", "enum": ["commencer", "arreter"]},
                "cible": {"type": "string", "description": "Application ou mot du titre. Vide pour la fenêtre que Julien utilise."}
            }, "required": ["action"]}
        },
        {
            "name": "lire_skill",
            "description": "Charge les instructions complètes d'un de tes skills (la liste est dans tes consignes). À appeler dès que la situation correspond à la description d'un skill, avant d'agir ou de répondre.",
            "parameters": {"type": "object", "properties": {
                "nom": {"type": "string", "description": "Nom du skill, tel qu'il apparaît dans la liste."}
            }, "required": ["nom"]}
        },
        {
            "name": "volume",
            "description": "Volume du son de l'ordinateur. monter / baisser : de 10 points par défaut, ou de valeur. regler : met le volume à valeur (0 à 100). niveau : dit où en est le volume. couper : coupe le son juste après ta réponse (dis un mot très bref). remettre : remet le son.",
            "parameters": {"type": "object", "properties": {
                "action": {"type": "string", "enum": ["monter", "baisser", "regler", "niveau", "couper", "remettre"]},
                "valeur": {"type": "integer", "description": "Pas en points pour monter/baisser, niveau cible pour regler."}
            }, "required": ["action"]}
        },
        {
            "name": "finir_conversation",
            "description": "Julien met fin à la conversation (« tais-toi », « c'est fini », « laisse tomber », « à plus », « merci c'est tout ») : tu dis un mot très bref et tu arrêtes de l'écouter. Il rouvrira la conversation en cliquant sur l'œuf."
        },
        {
            "name": "pas_maintenant",
            "description": "Julien ne veut pas être dérangé : Oz ne prendra plus la parole de lui-même pendant un moment, et la conversation en cours s'arrête. À utiliser quand il dit qu'il est occupé, concentré, en réunion, ou te demande de le laisser tranquille un moment.",
            "parameters": {"type": "object", "properties": {
                "minutes": {"type": "integer", "description": "Durée du silence demandée, 60 par défaut."}
            }}
        }
    ]}]);
    // noyau : fichiers, PowerShell, outils fabriqués par Oz
    if let Some(list) = decl[0]["functionDeclarations"].as_array_mut() {
        list.extend(crate::kernel::declarations());
    }
    decl
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
        "faire_une_figure" => {
            let vers = arg("vers");
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
        "lieux" => crate::places::tool(app, &arg("operation"), &arg("nom")),
        "regarder_avec_moi" => crate::watch::tool(app, &arg("action"), &arg("cible").to_lowercase()),
        "volume" => crate::volume::tool(app, &arg("action"), args["valeur"].as_i64()),
        "lire_skill" => crate::skills::tool(app, &arg("nom")),
        "lire_fichier" => crate::kernel::lire_fichier(app, &arg("chemin")),
        "ecrire_fichier" => crate::kernel::ecrire_fichier(app, &arg("chemin"), args["contenu"].as_str().unwrap_or("")),
        "modifier_fichier" => crate::kernel::modifier_fichier(
            app,
            &arg("chemin"),
            args["ancien"].as_str().unwrap_or(""),
            args["nouveau"].as_str().unwrap_or(""),
        ),
        "executer" => crate::kernel::executer(app, &arg("commande")).await,
        "creer_outil" => crate::kernel::creer_outil(app, args),
        "utiliser_outil" => {
            let a: Value = serde_json::from_str(&arg("arguments")).unwrap_or(json!({}));
            crate::kernel::utiliser_outil(app, &arg("nom"), &a).await
        }
        "changer_pose" => {
            let _ = app.emit("oz://do", json!({"action": "pose", "pose": arg("pose")}));
            "Pose changée.".into()
        }
        "changer_plan" => {
            let behind = arg("plan") == "derriere";
            match crate::prefs::update(app, |p| p.behind = behind).await {
                Ok(()) if behind => "Oz est maintenant derrière toutes les fenêtres, sur le bureau (gardé au redémarrage).".into(),
                Ok(()) => "Oz est de nouveau au-dessus de toutes les fenêtres.".into(),
                Err(e) => format!("Impossible de changer de plan ({e})."),
            }
        }
        "mode_deplacement" => {
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
        "personnaliser_oeuf" => {
            let couleur = arg("couleur");
            let texture = arg("texture");
            if !couleur.is_empty() && couleur != "origine" && !crate::prefs::is_hex_color(&couleur) {
                return format!("Couleur « {couleur} » invalide : donne-la en #rrggbb.");
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
        "zone_interdite" => zone(app, &arg("operation")),
        // fin de conversation : la vue en direct s'arrête aussi
        "finir_conversation" => {
            let _ = app.emit("oz://do", json!({"action": "end"}));
            stop_watching(app);
            "Conversation terminée : le micro se coupe après ta réponse.".into()
        }
        "pas_maintenant" => {
            let _ = app.emit("oz://do", json!({"action": "end"}));
            stop_watching(app);
            crate::initiative::snooze(app, args["minutes"].as_u64().unwrap_or(60))
        }
        other => format!("Outil inconnu : {other}"),
    }
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

/// Arrête « Regarde avec moi » s'il est en cours.
fn stop_watching(app: &AppHandle) {
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
    log_event(app, "plan", if behind { "derrière les fenêtres" } else { "devant les fenêtres" });
    Ok(())
}

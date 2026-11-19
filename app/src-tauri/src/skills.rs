//! Skills : des savoir-faire écrits en texte, que l'utilisateur ajoute depuis le
//! panneau (et, plus tard, qu'Oz proposera lui-même).
//!
//! Un skill = `skills/<id>/SKILL.md` dans le dossier de config :
//! un en-tête (nom, description, actif) puis les instructions. Oz ne voit
//! que la liste des noms et descriptions ; il charge les instructions avec
//! l'outil `read_skill` quand la situation s'y prête (aucun mot-clé).
//! Du texte seulement : rien n'est exécuté.

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use tauri::{AppHandle, Manager};

use crate::journal::{log_event, now_ts};

/// Garde-fous contre l'enflure (la liste part dans chaque session).
const MAX_IN_PROMPT: usize = 20;
const MAX_DESCRIPTION: usize = 240;
const MAX_INSTRUCTIONS: usize = 8000;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Skill {
    /// Nom du dossier (stable, dérivé du nom à la création).
    #[serde(default)]
    pub id: String,
    pub name: String,
    pub description: String,
    pub instructions: String,
    pub enabled: bool,
    #[serde(default)]
    pub ts: u64,
    /// Proposé par Oz après relecture de ses ratés, en attente de l'utilisateur.
    #[serde(default)]
    pub proposed: bool,
    /// Pourquoi Oz le propose (les ratés constatés).
    #[serde(default)]
    pub reason: String,
    /// Fourni par un module installé (lecture seule) : son id.
    #[serde(default)]
    pub module: String,
}

fn skills_dir(app: &AppHandle) -> PathBuf {
    app.path().app_data_dir().expect("app_data_dir").join("skills")
}

/// « Résumer une réunion ! » -> « resumer-une-reunion ».
fn slug(name: &str) -> String {
    let mut s = String::new();
    for c in name.trim().to_lowercase().chars() {
        let c = match c {
            'à' | 'â' | 'ä' => 'a',
            'é' | 'è' | 'ê' | 'ë' => 'e',
            'î' | 'ï' => 'i',
            'ô' | 'ö' => 'o',
            'ù' | 'û' | 'ü' => 'u',
            'ç' => 'c',
            c => c,
        };
        if c.is_ascii_alphanumeric() {
            s.push(c);
        } else if !s.ends_with('-') && !s.is_empty() {
            s.push('-');
        }
    }
    s.trim_end_matches('-').chars().take(48).collect()
}

/// Lit un SKILL.md : en-tête `---` clé: valeur `---`, puis le corps.
fn parse(id: &str, text: &str, ts: u64) -> Option<Skill> {
    let rest = text.trim_start_matches('\u{feff}').strip_prefix("---")?;
    let (head, body) = rest.split_once("\n---")?;
    let mut skill = Skill {
        id: id.into(),
        name: id.into(),
        description: String::new(),
        instructions: body.trim_start_matches(['\r', '\n']).trim_end().to_string(),
        enabled: true,
        ts,
        proposed: false,
        reason: String::new(),
        module: String::new(),
    };
    for line in head.lines() {
        let Some((k, v)) = line.split_once(':') else { continue };
        let v = v.trim().to_string();
        match k.trim() {
            "name" => skill.name = v,
            "description" => skill.description = v,
            "enabled" => skill.enabled = v != "false",
            "proposed" => skill.proposed = v == "true",
            "reason" => skill.reason = v,
            _ => {}
        }
    }
    Some(skill)
}

fn render(s: &Skill) -> String {
    let one_line = |t: &str| t.replace(['\r', '\n'], " ").trim().to_string();
    let mut head = format!(
        "---\nname: {}\ndescription: {}\nenabled: {}\n",
        one_line(&s.name),
        one_line(&s.description),
        s.enabled
    );
    if s.proposed {
        head.push_str(&format!("proposed: true\nreason: {}\n", one_line(&s.reason)));
    }
    format!("{head}---\n\n{}\n", s.instructions.trim())
}

pub(crate) fn load(app: &AppHandle) -> Vec<Skill> {
    let mut skills = load_dir(&skills_dir(app));
    // skills livrés par les modules installés : en lecture seule
    for (module, dir) in crate::modules::skill_dirs(app) {
        for mut k in load_dir(&dir) {
            k.id = format!("{module}/{}", k.id);
            k.module = module.clone();
            skills.push(k);
        }
    }
    skills.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    skills
}

fn load_dir(dir: &std::path::Path) -> Vec<Skill> {
    let Ok(entries) = fs::read_dir(dir) else { return Vec::new() };
    let mut skills: Vec<Skill> = entries
        .flatten()
        .filter_map(|e| {
            let id = e.file_name().to_string_lossy().to_string();
            let path = e.path().join("SKILL.md");
            let text = fs::read_to_string(&path).ok()?;
            let ts = fs::metadata(&path)
                .and_then(|m| m.modified())
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs())
                .unwrap_or(0);
            parse(&id, &text, ts)
        })
        .collect();
    skills.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    skills
}

/// Bloc ajouté aux consignes : la liste des skills actifs.
pub(crate) fn index_block(app: &AppHandle) -> String {
    if !crate::toolsets::enabled(app, "skills") {
        return String::new();
    }
    let skills: Vec<Skill> = load(app).into_iter().filter(|s| s.enabled && !s.proposed).take(MAX_IN_PROMPT).collect();
    if skills.is_empty() {
        return String::new();
    }
    let mut s = String::from(
        " Tes skills : des procédures écrites que %OWNER% t'a apprises. Un skill te dit COMMENT t'y prendre ; il \
         n'agit pas : pour agir, tu appelles toujours tes outils. Quand la situation correspond à la description d'un \
         skill, ouvre-le avec read_skill AVANT d'agir ou de répondre, puis suis ses étapes sans les réciter :",
    );
    for k in skills {
        s.push_str(&format!(" [{}] {}.", k.name, k.description.trim_end_matches('.')));
    }
    s
}

/// Outil `read_skill`.
pub(crate) fn tool(app: &AppHandle, name: &str) -> String {
    let wanted = slug(name);
    let skills = load(app);
    let found = skills
        .iter()
        .filter(|s| s.enabled && !s.proposed)
        .find(|s| s.id == wanted || slug(&s.name) == wanted)
        .or_else(|| skills.iter().filter(|s| s.enabled && !s.proposed).find(|s| slug(&s.name).contains(&wanted)));
    match found {
        Some(s) => {
            log_event(app, "skill", &format!("lit « {} »", s.name));
            format!("Skill « {} » :\n{}", s.name, s.instructions)
        }
        None => {
            let names: Vec<String> = skills.iter().filter(|s| s.enabled && !s.proposed).map(|s| s.name.clone()).collect();
            format!("Aucun skill actif nommé « {name} ». Skills disponibles : {}.", names.join(", "))
        }
    }
}

#[tauri::command]
pub(crate) fn get_skills(app: AppHandle) -> Vec<Skill> {
    load(&app)
}

/// Crée (id vide) ou met à jour un skill. Renvoie son id.
#[tauri::command]
pub(crate) fn save_skill(app: AppHandle, skill: Skill) -> Result<String, String> {
    let mut skill = skill;
    skill.name = skill.name.trim().to_string();
    skill.description = skill.description.trim().chars().take(MAX_DESCRIPTION).collect();
    skill.instructions = skill.instructions.trim().chars().take(MAX_INSTRUCTIONS).collect();
    if skill.name.is_empty() || skill.description.is_empty() || skill.instructions.is_empty() {
        return Err("Le nom, la description et les instructions sont obligatoires.".into());
    }
    let creating = skill.id.is_empty();
    if creating {
        let base = slug(&skill.name);
        if base.is_empty() {
            return Err("Le nom doit contenir au moins une lettre ou un chiffre.".into());
        }
        let mut id = base.clone();
        let mut n = 2;
        while skills_dir(&app).join(&id).exists() {
            id = format!("{base}-{n}");
            n += 1;
        }
        skill.id = id;
    } else if skill.id.contains(['/', '\\', '.']) {
        return Err("Identifiant de skill invalide.".into());
    }
    let dir = skills_dir(&app).join(&skill.id);
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    skill.ts = now_ts();
    fs::write(dir.join("SKILL.md"), render(&skill)).map_err(|e| e.to_string())?;
    log_event(&app, "skill", &format!("{} « {} »", if creating { "ajouté" } else { "modifié" }, skill.name));
    Ok(skill.id)
}

#[tauri::command]
pub(crate) fn delete_skill(app: AppHandle, id: String) -> Result<(), String> {
    if id.is_empty() || id.contains(['/', '\\', '.']) {
        return Err("Identifiant de skill invalide.".into());
    }
    fs::remove_dir_all(skills_dir(&app).join(&id)).map_err(|e| e.to_string())?;
    log_event(&app, "skill", &format!("supprimé « {id} »"));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slug_accents_et_ponctuation() {
        assert_eq!(slug("Résumer une réunion !"), "resumer-une-reunion");
        assert_eq!(slug("  ça va?  "), "ca-va");
    }

    #[test]
    fn aller_retour_skill_md() {
        let s = Skill {
            id: "x".into(),
            name: "Mon skill".into(),
            description: "Quand %OWNER%\nprépare un mariage".into(),
            instructions: "Étape 1\n\nÉtape 2: ok".into(),
            enabled: false,
            ts: 0,
            proposed: true,
            reason: "deux fois\nsans outil".into(),
            module: String::new(),
        };
        let p = parse("x", &render(&s), 0).unwrap();
        assert_eq!(p.name, "Mon skill");
        assert_eq!(p.description, "Quand %OWNER% prépare un mariage");
        assert_eq!(p.instructions, "Étape 1\n\nÉtape 2: ok");
        assert!(!p.enabled);
        assert!(p.proposed);
        assert_eq!(p.reason, "deux fois sans outil");
    }
}

// ── Oz propose ses propres skills ──
// La seconde boucle de Harness², en version légère : de temps en temps,
// Oz relit son journal (échanges, outils, échecs). Si un même raté revient
// au moins deux fois et qu'un savoir-faire écrit l'éviterait, il rédige
// UN skill, désactivé, que l'utilisateur accepte ou refuse dans le panneau.
// Un raté isolé est du bruit : on ne retient rien.

use std::sync::atomic::{AtomicU32, Ordering};

/// Une relecture toutes les N consolidations de mémoire.
const REFLECT_EVERY: u32 = 3;
static SINCE_REFLECTION: AtomicU32 = AtomicU32::new(0);
const MAX_PROPOSED: usize = 3;

const REFLECTION_PROMPT: &str = "Tu relis le journal d'Oz, un compagnon de bureau vocal (%OWNER% lui parle). \
Cherche un RATÉ QUI REVIENT au moins deux fois : Oz qui dit ne pas pouvoir faire quelque chose, un outil qui échoue, \
%OWNER% qui doit reformuler ou corriger Oz, une même demande mal traitée. \
Si un savoir-faire écrit (des instructions en texte, sans code) éviterait ce raté à l'avenir, rédige UN skill. \
Sinon (raté isolé, simple bruit de transcription, ou problème qu'un texte ne règle pas), n'en propose aucun. \
N'en propose pas un qui recouvre un skill existant. Les instructions s'adressent à Oz, au tutoiement, courtes et concrètes. \
Réponds UNIQUEMENT en JSON : {\"skill\": null} ou \
{\"skill\": {\"name\": \"...\", \"description\": \"quand s'en servir, en une phrase\", \"instructions\": \"...\", \"reason\": \"les ratés constatés, en une phrase\"}}";

/// Appelé après chaque consolidation de mémoire.
pub(crate) fn maybe_reflect(app: &AppHandle) {
    if SINCE_REFLECTION.fetch_add(1, Ordering::SeqCst) + 1 < REFLECT_EVERY {
        return;
    }
    SINCE_REFLECTION.store(0, Ordering::SeqCst);
    let skills = load(app);
    if skills.iter().filter(|s| s.proposed).count() >= MAX_PROPOSED {
        return; // L'utilisateur n'a pas encore trié les propositions
    }
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        match reflect(&app, &skills).await {
            Ok(Some(name)) => log_event(&app, "skill", &format!("proposé par Oz : « {name} »")),
            Ok(None) => log_event(&app, "skill", "relecture : rien à proposer"),
            Err(e) => log_event(&app, "skill", &format!("relecture impossible : {e}")),
        }
    });
}

/// Les dernières lignes utiles du journal (échanges, outils, noyau).
fn recent_journal(app: &AppHandle) -> String {
    let text = fs::read_to_string(crate::journal::journal_path(app)).unwrap_or_default();
    let lines: Vec<String> = text
        .lines()
        .filter_map(|l| serde_json::from_str::<serde_json::Value>(l).ok())
        .filter(|v| matches!(v["kind"].as_str(), Some("conversation" | "tool" | "kernel" | "vision" | "watch")))
        .map(|v| format!("{} : {}", v["kind"].as_str().unwrap_or(""), v["detail"].as_str().unwrap_or("")))
        .collect();
    lines[lines.len().saturating_sub(150)..].join("\n")
}

async fn reflect(app: &AppHandle, skills: &[Skill]) -> Result<Option<String>, String> {
    let cfg = app.state::<crate::brain::BrainState>().config.lock().unwrap().clone();
    if cfg.api_key.is_empty() {
        return Err("pas de clé Gemini".into());
    }
    let model = if cfg.provider == "gemini" { cfg.model.clone() } else { "gemini-3.8-flash".into() };
    let existing: Vec<String> = skills.iter().map(|s| format!("[{}] {}", s.name, s.description)).collect();
    let ctx = format!(
        "Skills existants :\n{}\n\nJournal récent :\n{}",
        if existing.is_empty() { "aucun".into() } else { existing.join("\n") },
        recent_journal(app)
    );
    let url = format!("https://generativelanguage.googleapis.com/v1beta/models/{model}:generateContent");
    let body = serde_json::json!({
        "systemInstruction": {"parts": [{"text": crate::prefs::personalize(app, REFLECTION_PROMPT)}]},
        "contents": [{"role": "user", "parts": [{"text": ctx}]}],
        "generationConfig": {"responseMimeType": "application/json", "temperature": 0.2}
    });
    let resp = crate::brain::http_client()
        .post(&url)
        .header("x-goog-api-key", &cfg.api_key)
        .json(&body)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(crate::brain::http_error("Gemini", resp).await);
    }
    let v: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
    let text = v["candidates"][0]["content"]["parts"]
        .as_array()
        .and_then(|parts| parts.iter().filter(|p| p["thought"].as_bool() != Some(true)).find_map(|p| p["text"].as_str()))
        .ok_or("réponse vide")?;
    let out: serde_json::Value = serde_json::from_str(text).map_err(|_| "réponse illisible".to_string())?;
    let k = &out["skill"];
    if k.is_null() {
        return Ok(None);
    }
    let get = |f: &str| k[f].as_str().unwrap_or("").trim().to_string();
    let skill = Skill {
        id: String::new(),
        name: get("name"),
        description: get("description"),
        instructions: get("instructions"),
        enabled: false,
        ts: 0,
        proposed: true,
        reason: get("reason"),
        module: String::new(),
    };
    let name = skill.name.clone();
    save_skill(app.clone(), skill)?;
    let _ = tauri::Emitter::emit(app, "oz://skills-updated", ());
    Ok(Some(name))
}

// ── Toolset « Skills » ──

pub(crate) fn toolsets() -> Vec<crate::toolsets::Toolset> {
    vec![crate::toolsets::Toolset {
        id: "skills".into(),
        label: "Lecture des skills".into(),
        description: "L'action qui permet à Oz d'ouvrir un skill, une procédure écrite (onglet Skills). Coupé, il ne voit plus aucun skill.".into(),
        core: false,
        tools: vec![serde_json::json!({
            "name": "read_skill",
            "description": "Charge les instructions complètes d'un de tes skills (la liste est dans tes consignes). À appeler dès que la situation correspond à la description d'un skill, avant d'agir ou de répondre.",
            "parameters": {"type": "object", "properties": {
                "name": {"type": "string", "description": "Nom du skill, tel qu'il apparaît dans la liste."}
            }, "required": ["name"]}
        })],
    }]
}

pub(crate) fn run(app: &AppHandle, name: &str, args: &serde_json::Value) -> Option<String> {
    (name == "read_skill").then(|| tool(app, args["name"].as_str().unwrap_or("").trim()))
}

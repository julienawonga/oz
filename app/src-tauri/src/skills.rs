//! Skills : des savoir-faire écrits en texte, que Julien ajoute depuis le
//! panneau (et, plus tard, qu'Oz proposera lui-même).
//!
//! Un skill = `skills/<id>/SKILL.md` dans le dossier de config :
//! un en-tête (nom, description, actif) puis les instructions. Oz ne voit
//! que la liste des noms et descriptions ; il charge les instructions avec
//! l'outil `lire_skill` quand la situation s'y prête (aucun mot-clé).
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
    };
    for line in head.lines() {
        let Some((k, v)) = line.split_once(':') else { continue };
        let v = v.trim().to_string();
        match k.trim() {
            "name" => skill.name = v,
            "description" => skill.description = v,
            "enabled" => skill.enabled = v != "false",
            _ => {}
        }
    }
    Some(skill)
}

fn render(s: &Skill) -> String {
    let one_line = |t: &str| t.replace(['\r', '\n'], " ").trim().to_string();
    format!(
        "---\nname: {}\ndescription: {}\nenabled: {}\n---\n\n{}\n",
        one_line(&s.name),
        one_line(&s.description),
        s.enabled,
        s.instructions.trim()
    )
}

pub(crate) fn load(app: &AppHandle) -> Vec<Skill> {
    let Ok(entries) = fs::read_dir(skills_dir(app)) else { return Vec::new() };
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
    let skills: Vec<Skill> = load(app).into_iter().filter(|s| s.enabled).take(MAX_IN_PROMPT).collect();
    if skills.is_empty() {
        return String::new();
    }
    let mut s = String::from(
        " Tes skills (savoir-faire que Julien t'a appris). Quand la situation correspond à la description d'un skill, \
         charge-le avec l'outil lire_skill AVANT d'agir ou de répondre, puis suis ses instructions sans les réciter :",
    );
    for k in skills {
        s.push_str(&format!(" [{}] {}.", k.name, k.description.trim_end_matches('.')));
    }
    s
}

/// Outil `lire_skill`.
pub(crate) fn tool(app: &AppHandle, name: &str) -> String {
    let wanted = slug(name);
    let skills = load(app);
    let found = skills
        .iter()
        .filter(|s| s.enabled)
        .find(|s| s.id == wanted || slug(&s.name) == wanted)
        .or_else(|| skills.iter().filter(|s| s.enabled).find(|s| slug(&s.name).contains(&wanted)));
    match found {
        Some(s) => {
            log_event(app, "skill", &format!("lit « {} »", s.name));
            format!("Skill « {} » :\n{}", s.name, s.instructions)
        }
        None => {
            let names: Vec<String> = skills.iter().filter(|s| s.enabled).map(|s| s.name.clone()).collect();
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
            description: "Quand Julien\nprépare un mariage".into(),
            instructions: "Étape 1\n\nÉtape 2: ok".into(),
            enabled: false,
            ts: 0,
        };
        let p = parse("x", &render(&s), 0).unwrap();
        assert_eq!(p.name, "Mon skill");
        assert_eq!(p.description, "Quand Julien prépare un mariage");
        assert_eq!(p.instructions, "Étape 1\n\nÉtape 2: ok");
        assert!(!p.enabled);
    }
}

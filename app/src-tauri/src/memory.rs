//! Mémoire multi-niveaux (inspirée de Waku Agent) : souvenirs de fenêtres,
//! faits durables, épisodes, consolidation par lots.

use serde::{Deserialize, Serialize};
use std::fs;
use tauri::{AppHandle, Manager};

use crate::brain::{http_client, BrainState, ChatMessage};
use crate::journal::{log_event, now_ts};

#[derive(Serialize, Deserialize, Clone)]
pub struct Memory {
    pub key: String, // "process::titre" de la fenêtre
    pub note: String,
    pub ts: u64,
}

pub(crate) fn memories_path(app: &AppHandle) -> std::path::PathBuf {
    app.path()
        .app_data_dir()
        .expect("app_data_dir")
        .join("memories.json")
}

pub(crate) fn load_memories(app: &AppHandle) -> Vec<Memory> {
    fs::read_to_string(memories_path(app))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub(crate) fn save_memories(app: &AppHandle, memories: &[Memory]) -> Result<(), String> {
    let path = memories_path(app);
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    fs::write(&path, serde_json::to_string_pretty(memories).unwrap())
        .map_err(|e| e.to_string())
}

/// Ajoute/rafraîchit un souvenir : un souvenir par fenêtre, remplacé à chaque vision
/// (le lifecycle learn/merge/prune complet arrive avec le polissage de l'album).
pub(crate) fn remember(app: &AppHandle, key: String, note: String) {
    let mut memories = load_memories(app);
    memories.retain(|m| m.key != key); // remplace l'ancien souvenir de ce lieu
    memories.push(Memory { key, note, ts: now_ts() });
    let _ = save_memories(app, &memories);
}

#[tauri::command]
pub(crate) fn add_memory(app: AppHandle, key: String, note: String) -> Result<(), String> {
    remember(&app, key, note);
    Ok(())
}

#[tauri::command]
pub(crate) fn get_memories(app: AppHandle) -> Vec<Memory> {
    load_memories(&app)
}

#[tauri::command]
pub(crate) fn clear_memory(app: AppHandle, key: String) -> Result<(), String> {
    let mut memories = load_memories(&app);
    memories.retain(|m| m.key != key);
    save_memories(&app, &memories)
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Fact {
    pub subject: String,
    pub content: String,
    pub ts: u64,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Episode {
    pub summary: String,
    pub ts: u64,
}

pub(crate) fn facts_path(app: &AppHandle) -> std::path::PathBuf {
    app.path().app_data_dir().expect("app_data_dir").join("semantic.json")
}
pub(crate) fn episodes_path(app: &AppHandle) -> std::path::PathBuf {
    app.path().app_data_dir().expect("app_data_dir").join("episodic.json")
}

pub(crate) fn load_facts(app: &AppHandle) -> Vec<Fact> {
    fs::read_to_string(facts_path(app))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub(crate) fn load_episodes(app: &AppHandle) -> Vec<Episode> {
    fs::read_to_string(episodes_path(app))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

/// Filtre anti-bruit (Waku : is_ops_state) — l'état opérationnel change
/// trop vite pour être une mémoire.
pub(crate) fn is_ops_state(content: &str) -> bool {
    let c = content.to_lowercase();
    ["solde", "crédit", "credit", "token", "rate limit", "429", "402",
     "insuffisant", "balance", "erreur d'outil", "mémoire contient", "mémoire ne"]
        .iter()
        .any(|kw| c.contains(kw))
}

/// Dédoublonnage sémantique : un fait dont le contenu est déjà couvert
/// (contient / est contenu) n'entre pas (Waku : "restates").
pub(crate) fn is_duplicate(facts: &[Fact], content: &str) -> bool {
    let c = content.to_lowercase();
    facts.iter().any(|f| {
        let e = f.content.to_lowercase();
        (e.len() > 15 && c.contains(&e)) || (c.len() > 15 && e.contains(&c))
    })
}

pub(crate) const CONSOLIDATION_EVERY: usize = 6; // Waku : "only consolidate after N new chats"

pub(crate) const CONSOLIDATION_PROMPT: &str = "Tu distilles la conversation récente d'un compagnon de bureau en mémoire long terme. \
Extrais : 1) les faits durables sur %OWNER% (préférences, projets, personnes, décisions) — un fait = une phrase courte qui nomme son sujet ; \
2) un épisode : une phrase résumant ce qui s'est passé dans cette conversation. \
NE RETIENS JAMAIS l'état opérationnel de l'assistant (soldes, crédits, erreurs d'outils, limites, tokens), \
ni le contenu des réponses de l'assistant, ni ce que la mémoire contient ou non. \
N'extrais que ce que %OWNER% a dit ou décidé lui-même : ce qu'Oz a lu sur un écran ou dans la conversation de quelqu'un d'autre n'est pas un fait sur %OWNER%, sauf s'il le confirme. \
Tu reçois aussi les faits déjà connus, numérotés. Si un fait nouveau contredit, corrige ou met à jour un fait connu, mets son numéro dans \"replaces\" : l'ancien sera effacé. Ne répète pas un fait déjà connu. \
Réponds UNIQUEMENT en JSON : \
{\"facts\":[{\"subject\":\"...\",\"content\":\"...\",\"replaces\":[]}],\"episode\":\"...\"}";

/// Consolidation : distille les N derniers échanges en faits + épisode.
/// Lancée en tâche de fond — ne bloque jamais une réponse.
pub(crate) fn spawn_consolidation(app: AppHandle, exchanges: Vec<ChatMessage>) {
    tauri::async_runtime::spawn(async move {
        let cfg = {
            let brain = app.state::<BrainState>();
            brain.config.lock().unwrap().clone()
        };
        // les faits connus, numérotés : le modèle signale ceux qu'un fait
        // nouveau contredit (sinon deux versions contradictoires s'empilaient)
        let mut facts = load_facts(&app);
        let known: Vec<String> = facts
            .iter()
            .enumerate()
            .skip(facts.len().saturating_sub(80))
            .map(|(i, f)| format!("{i}. {}", f.content))
            .collect();
        let convo = format!(
            "Faits connus :\n{}\n\nConversation récente :\n{}",
            if known.is_empty() { "aucun".into() } else { known.join("\n") },
            serde_json::to_string(&exchanges).unwrap_or_default()
        );
        let prompt = crate::prefs::personalize(&app, CONSOLIDATION_PROMPT);
        let text = match cfg.provider.as_str() {
            "gemini" => consolidate_gemini(&cfg, &prompt, &convo).await,
            _ => consolidate_ollama(&cfg, &prompt, &convo).await,
        };
        let text = match text {
            Ok(t) => t,
            Err(e) => {
                log_event(&app, "memory", &format!("consolidation impossible : {e}"));
                return;
            }
        };
        let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) else {
            log_event(&app, "memory", "consolidation : réponse illisible");
            return;
        };

        // Faits : contradictions retirées, filtres Waku, puis écriture
        let mut kept = 0;
        let mut replaced: Vec<usize> = Vec::new();
        if let Some(list) = v["facts"].as_array() {
            for f in list {
                if let Some(r) = f["replaces"].as_array() {
                    replaced.extend(r.iter().filter_map(|n| n.as_u64()).map(|n| n as usize).filter(|n| *n < facts.len()));
                }
            }
        }
        if !replaced.is_empty() {
            replaced.sort_unstable();
            replaced.dedup();
            for i in replaced.iter().rev() {
                facts.remove(*i);
            }
        }
        if let Some(list) = v["facts"].as_array() {
            for f in list {
                let (Some(subject), Some(content)) =
                    (f["subject"].as_str(), f["content"].as_str())
                else { continue };
                if is_ops_state(content) || is_duplicate(&facts, content) {
                    continue;
                }
                facts.push(Fact {
                    subject: subject.chars().take(60).collect(),
                    content: content.chars().take(200).collect(),
                    ts: now_ts(),
                });
                kept += 1;
            }
        }
        if kept > 0 || !replaced.is_empty() {
            if facts.len() > 200 { facts = facts.split_off(facts.len() - 200); } // vieillissement
            let _ = fs::write(facts_path(&app), serde_json::to_string_pretty(&facts).unwrap());
        }

        // Épisode
        if let Some(ep) = v["episode"].as_str() {
            if !ep.is_empty() && !is_ops_state(ep) {
                let mut episodes = load_episodes(&app);
                episodes.push(Episode { summary: ep.chars().take(200).collect(), ts: now_ts() });
                if episodes.len() > 100 { episodes = episodes.split_off(episodes.len() - 100); }
                let _ = fs::write(episodes_path(&app), serde_json::to_string_pretty(&episodes).unwrap());
            }
        }
        log_event(&app, "memory", &format!("consolidation : {} fait(s) retenu(s), {} remplacé(s)", kept, replaced.len()));
        crate::skills::maybe_reflect(&app);
    });
}

async fn consolidate_ollama(cfg: &crate::brain::BrainConfig, prompt: &str, convo: &str) -> Result<String, String> {
    let body = serde_json::json!({
        "model": cfg.model,
        "messages": [{"role": "system", "content": prompt},
                     {"role": "user", "content": convo}],
        "stream": false,
        "think": false,
        "format": "json"
    });
    let resp = http_client()
        .post(format!("{}/api/chat", cfg.url))
        .json(&body)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    let json = resp.json::<serde_json::Value>().await.map_err(|e| e.to_string())?;
    json["message"]["content"].as_str().map(str::to_string).ok_or_else(|| "réponse vide".into())
}

async fn consolidate_gemini(cfg: &crate::brain::BrainConfig, prompt: &str, convo: &str) -> Result<String, String> {
    let url = format!(
        "https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent",
        cfg.model
    );
    let body = serde_json::json!({
        "systemInstruction": {"parts": [{"text": prompt}]},
        "contents": [{"role": "user", "parts": [{"text": convo}]}],
        "generationConfig": {"responseMimeType": "application/json", "temperature": 0.2}
    });
    let resp = http_client()
        .post(&url)
        .header("x-goog-api-key", &cfg.api_key)
        .json(&body)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(crate::brain::http_error("Gemini", resp).await);
    }
    let json = resp.json::<serde_json::Value>().await.map_err(|e| e.to_string())?;
    json["candidates"][0]["content"]["parts"]
        .as_array()
        .and_then(|parts| parts.iter().filter(|p| p["thought"].as_bool() != Some(true)).find_map(|p| p["text"].as_str()))
        .map(str::to_string)
        .ok_or_else(|| "réponse vide".into())
}

#[tauri::command]
pub(crate) fn get_facts(app: AppHandle) -> Vec<Fact> {
    load_facts(&app)
}

#[tauri::command]
pub(crate) fn forget_fact(app: AppHandle, ts: u64, content: String) -> Result<(), String> {
    // les faits d'une même consolidation partagent leur ts : le contenu
    // (unique grâce au dédoublonnage) désigne le bon
    let mut facts = load_facts(&app);
    facts.retain(|f| !(f.ts == ts && f.content == content));
    fs::write(facts_path(&app), serde_json::to_string_pretty(&facts).unwrap())
        .map_err(|e| e.to_string())
}

/// Injection : "ce qu'Oz sait déjà" — derniers faits + souvenirs spatiaux
/// pertinents (Waku : le bloc "company brain").
pub(crate) fn build_memory_block(app: &AppHandle, user_message: &str) -> String {
    let mut block = String::new();
    let facts = load_facts(app);
    if !facts.is_empty() {
        block.push_str("\n\nCe que tu sais déjà de %OWNER% :\n");
        for f in facts.iter().rev().take(8) {
            block.push_str(&format!("- {}\n", f.content));
        }
    }
    // Album spatial : les souvenirs dont le lieu est évoqué dans le message
    let memories = load_memories(app);
    let mentioned: Vec<&Memory> = memories
        .iter()
        .filter(|m| {
            m.key.split([':', ' ', '.', '-'])
                .filter(|w| w.len() > 3)
                .any(|w| user_message.to_lowercase().contains(&w.to_lowercase()))
        })
        .take(3)
        .collect();
    if !mentioned.is_empty() {
        block.push_str("\nSouvenirs de lieux déjà regardés :\n");
        for m in mentioned {
            block.push_str(&format!("- [{}] {}\n", m.key.split("::").next().unwrap_or(""), m.note));
        }
    }
    block
}

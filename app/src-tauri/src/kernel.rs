//! Noyau « à la Pi » : quatre outils de base (lire, écrire, modifier un
//! fichier, exécuter une commande PowerShell) et la boîte à outils qu'Oz
//! se fabrique lui-même.
//!
//! On parle à Oz et la transcription se trompe parfois : rien de risqué ne
//! part sur une simple phrase. Le code, pas le modèle, décide :
//! - écrire et modifier : seulement dans son atelier ;
//! - lire hors de l'atelier : accord de Julien ;
//! - exécuter : une courte liste de commandes de lecture passe seule,
//!   tout le reste est montré à Julien (badge sur l'œuf) et attend son
//!   accord ; le résultat revient ensuite dans la conversation ;
//! - un outil créé par Oz : son test tourne après accord ; s'il passe,
//!   l'outil est adopté et ce script exact ne redemande plus d'accord.
//!   Le moindre changement du script redemande un accord.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};

use crate::journal::{log_event, now_ts};

const MAX_OUTPUT: usize = 4000;
const MAX_FILE: usize = 60_000;
const EXEC_TIMEOUT: Duration = Duration::from_secs(25);
/// Contre l'enflure : au-delà, Oz doit d'abord en supprimer un.
const MAX_TOOLS: usize = 25;

// ── Atelier ──

pub(crate) fn atelier(app: &AppHandle) -> PathBuf {
    let dir = app.path().app_data_dir().expect("app_data_dir").join("atelier");
    let _ = fs::create_dir_all(&dir);
    dir
}

fn toolbox_dir(app: &AppHandle) -> PathBuf {
    let dir = atelier(app).join("outils");
    let _ = fs::create_dir_all(&dir);
    dir
}

/// Chemin relatif à l'atelier, sans « .. » ni chemin absolu.
fn inside_atelier(app: &AppHandle, rel: &str) -> Result<PathBuf, String> {
    let p = Path::new(rel.trim());
    if rel.trim().is_empty() || p.is_absolute() || p.components().any(|c| !matches!(c, Component::Normal(_))) {
        return Err(format!("« {rel} » : donne un chemin relatif à ton atelier (par exemple notes/idees.txt)."));
    }
    Ok(atelier(app).join(p))
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let kept: String = s.chars().take(max).collect();
    format!("{kept}\n… (tronqué)")
}

// ── Accords en attente ──

#[derive(Clone)]
enum Pending {
    Command(String),
    Read(PathBuf),
    ToolTest(String),
    ToolRun(String, Value),
}

static PENDING: Mutex<Vec<(u64, Pending)>> = Mutex::new(Vec::new());
static NEXT_ID: AtomicU64 = AtomicU64::new(1);

/// Montre la demande sur l'œuf et renvoie le texte pour le modèle.
fn ask(app: &AppHandle, pending: Pending, title: &str, detail: &str) -> String {
    let id = NEXT_ID.fetch_add(1, Ordering::SeqCst);
    PENDING.lock().unwrap().push((id, pending));
    let _ = app.emit("oz://exec-request", json!({"id": id, "title": title, "detail": truncate(detail, 1500)}));
    log_event(app, "kernel", &format!("accord demandé : {title}"));
    "Il me faut l'accord de Julien : dis-lui de toucher le badge sur l'œuf pour voir et valider. \
     Le résultat te reviendra tout seul ensuite ; ne relance pas l'outil."
        .into()
}

/// Décision de Julien sur une demande (carte de l'œuf).
#[tauri::command]
pub(crate) async fn exec_decision(app: AppHandle, id: u64, allow: bool) -> Result<(), String> {
    let pending = {
        let mut list = PENDING.lock().unwrap();
        let Some(i) = list.iter().position(|(p, _)| *p == id) else { return Ok(()) };
        list.remove(i).1
    };
    let note = if !allow {
        log_event(&app, "kernel", "accord refusé");
        "[Note interne] Julien a refusé. N'insiste pas ; dis-le simplement en un mot si c'est utile.".to_string()
    } else {
        let result = match pending {
            Pending::Command(cmd) => run_powershell(&app, &cmd, None).await,
            Pending::Read(path) => read_file(&path),
            Pending::ToolTest(name) => test_tool(&app, &name).await,
            Pending::ToolRun(name, args) => match load_tool(&app, &name) {
                Some(tool) => {
                    approve(&app, &name, &tool.code);
                    run_tool_script(&app, &tool, &args).await
                }
                None => format!("Outil « {name} » introuvable."),
            },
        };
        format!("[Note interne] Julien a accepté. Résultat :\n{result}\nRésume-le-lui brièvement, ou continue ce que tu faisais.")
    };
    crate::voice::live_prompt(app, note).await
}

// ── Les quatre outils ──

fn read_file(path: &Path) -> String {
    match fs::read(path) {
        Ok(bytes) => {
            let text = String::from_utf8_lossy(&bytes);
            truncate(&text, MAX_FILE)
        }
        Err(e) => format!("Lecture impossible ({e})."),
    }
}

pub(crate) fn lire_fichier(app: &AppHandle, chemin: &str) -> String {
    let p = Path::new(chemin.trim());
    if p.is_absolute() {
        if p.starts_with(atelier(app)) {
            return read_file(p);
        }
        return ask(app, Pending::Read(p.to_path_buf()), "Oz veut lire un fichier", &p.display().to_string());
    }
    match inside_atelier(app, chemin) {
        Ok(path) => read_file(&path),
        Err(e) => e,
    }
}

pub(crate) fn ecrire_fichier(app: &AppHandle, chemin: &str, contenu: &str) -> String {
    let path = match inside_atelier(app, chemin) {
        Ok(p) => p,
        Err(e) => return e,
    };
    if let Some(dir) = path.parent() {
        let _ = fs::create_dir_all(dir);
    }
    match fs::write(&path, contenu) {
        Ok(()) => {
            log_event(app, "kernel", &format!("écrit {chemin} ({} octets)", contenu.len()));
            format!("Écrit : {chemin}.")
        }
        Err(e) => format!("Écriture impossible ({e})."),
    }
}

pub(crate) fn modifier_fichier(app: &AppHandle, chemin: &str, ancien: &str, nouveau: &str) -> String {
    let path = match inside_atelier(app, chemin) {
        Ok(p) => p,
        Err(e) => return e,
    };
    let Ok(text) = fs::read_to_string(&path) else {
        return format!("Fichier introuvable : {chemin}.");
    };
    match text.matches(ancien).count() {
        0 => "Le texte à remplacer n'est pas dans le fichier : relis-le d'abord.".into(),
        1 => match fs::write(&path, text.replacen(ancien, nouveau, 1)) {
            Ok(()) => format!("Modifié : {chemin}."),
            Err(e) => format!("Écriture impossible ({e})."),
        },
        n => format!("Le texte à remplacer apparaît {n} fois : donne un extrait plus long pour qu'il soit unique."),
    }
}

/// Commandes de lecture qui passent sans accord : un enchaînement de
/// cmdlets de cette liste, sans variable, bloc ni redirection.
fn is_read_only(cmd: &str) -> bool {
    const ALLOWED: &[&str] = &[
        "get-date", "get-process", "get-service", "get-volume", "get-psdrive", "get-computerinfo",
        "get-netadapter", "get-netipaddress", "get-timezone", "get-culture", "get-uptime",
        "get-location", "get-childitem", "get-ciminstance", "get-hotfix", "get-printer",
        "test-connection", "test-path", "measure-object", "select-object", "where-object",
        "sort-object", "group-object", "format-table", "format-list", "out-string",
        "convertto-json", "first", "select",
    ];
    if cmd.trim().is_empty() || cmd.chars().any(|c| ";&><`{}()$@\r\n".contains(c)) {
        return false;
    }
    cmd.split('|').all(|segment| {
        let first = segment.split_whitespace().next().unwrap_or("").to_lowercase();
        ALLOWED.contains(&first.as_str())
    })
}

pub(crate) async fn executer(app: &AppHandle, commande: &str) -> String {
    if is_read_only(commande) {
        return run_powershell(app, commande, None).await;
    }
    ask(app, Pending::Command(commande.to_string()), "Oz veut exécuter une commande", commande)
}

/// PowerShell sans profil ni fenêtre, dans l'atelier, avec délai.
async fn run_powershell(app: &AppHandle, script: &str, args: Option<&Value>) -> String {
    // sortie console en UTF-8, pour garder les accents
    let full = format!("[Console]::OutputEncoding = [System.Text.Encoding]::UTF8; {script}");
    let mut cmd = tokio::process::Command::new("powershell.exe");
    cmd.args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-Command", &full])
        .current_dir(atelier(app))
        .kill_on_drop(true)
        .creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    if let Some(a) = args {
        cmd.env("OZ_ARGS", a.to_string());
    }
    log_event(app, "kernel", &format!("exécute : {}", truncate(script, 160)));
    let child = match cmd.output_with_timeout().await {
        Ok(out) => out,
        Err(e) => return e,
    };
    let stdout = String::from_utf8_lossy(&child.stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&child.stderr).trim().to_string();
    let mut out = format!("Code de sortie : {}", child.status.code().unwrap_or(-1));
    if !stdout.is_empty() {
        out.push_str(&format!("\nSortie :\n{}", truncate(&stdout, MAX_OUTPUT)));
    }
    if !stderr.is_empty() {
        out.push_str(&format!("\nErreurs :\n{}", truncate(&stderr, MAX_OUTPUT / 2)));
    }
    out
}

trait OutputWithTimeout {
    async fn output_with_timeout(&mut self) -> Result<std::process::Output, String>;
}
impl OutputWithTimeout for tokio::process::Command {
    async fn output_with_timeout(&mut self) -> Result<std::process::Output, String> {
        match tokio::time::timeout(EXEC_TIMEOUT, self.output()).await {
            Ok(Ok(out)) => Ok(out),
            Ok(Err(e)) => Err(format!("Lancement impossible ({e}).")),
            Err(_) => Err(format!("Arrêté : plus de {} s.", EXEC_TIMEOUT.as_secs())),
        }
    }
}

/// Préambule des outils fabriqués : leurs arguments, lisibles.
const PREAMBLE: &str = "$OzArgs = if ($env:OZ_ARGS) { $env:OZ_ARGS | ConvertFrom-Json } else { $null }\n";

// ── Boîte à outils d'Oz ──

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct CustomTool {
    pub name: String,
    pub description: String,
    /// Paramètres, en clair : « ville (texte) : la ville voulue ».
    pub parameters: String,
    pub code: String,
    /// Arguments du test (JSON) et texte attendu dans la sortie.
    pub test_args: Value,
    pub test_expect: String,
    /// "en_test" (créé, test pas encore passé) ou "actif".
    pub status: String,
    /// Code exact approuvé par Julien (vide = jamais approuvé).
    #[serde(default)]
    pub approved_code: String,
    #[serde(default)]
    pub ts: u64,
    #[serde(default)]
    pub uses: u32,
}

fn tool_slug(name: &str) -> String {
    name.trim()
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect::<String>()
        .trim_matches('_')
        .chars()
        .take(40)
        .collect()
}

fn tool_path(app: &AppHandle, name: &str) -> PathBuf {
    toolbox_dir(app).join(format!("{}.json", tool_slug(name)))
}

fn load_tool(app: &AppHandle, name: &str) -> Option<CustomTool> {
    fs::read_to_string(tool_path(app, name)).ok().and_then(|s| serde_json::from_str(&s).ok())
}

fn save_tool(app: &AppHandle, tool: &CustomTool) {
    let _ = fs::write(tool_path(app, &tool.name), serde_json::to_string_pretty(tool).unwrap());
}

pub(crate) fn load_tools(app: &AppHandle) -> Vec<CustomTool> {
    let Ok(entries) = fs::read_dir(toolbox_dir(app)) else { return Vec::new() };
    let mut tools: Vec<CustomTool> = entries
        .flatten()
        .filter(|e| e.path().extension().is_some_and(|x| x == "json"))
        .filter_map(|e| fs::read_to_string(e.path()).ok())
        .filter_map(|s| serde_json::from_str(&s).ok())
        .collect();
    tools.sort_by(|a, b| a.name.cmp(&b.name));
    tools
}

fn approve(app: &AppHandle, name: &str, code: &str) {
    if let Some(mut t) = load_tool(app, name) {
        t.approved_code = code.to_string();
        save_tool(app, &t);
    }
}

async fn run_tool_script(app: &AppHandle, tool: &CustomTool, args: &Value) -> String {
    let script = format!("{PREAMBLE}{}", tool.code);
    run_powershell(app, &script, Some(args)).await
}

/// Test d'un outil (après accord) : il est adopté s'il sort sans erreur
/// et affiche le texte attendu.
async fn test_tool(app: &AppHandle, name: &str) -> String {
    let Some(mut tool) = load_tool(app, name) else {
        return format!("Outil « {name} » introuvable.");
    };
    tool.approved_code = tool.code.clone();
    let out = run_tool_script(app, &tool, &tool.test_args).await;
    let ok = out.starts_with("Code de sortie : 0") && out.contains(tool.test_expect.trim());
    if ok {
        tool.status = "actif".into();
        save_tool(app, &tool);
        log_event(app, "kernel", &format!("outil adopté : {}", tool.name));
        format!(
            "Test réussi : l'outil « {} » est adopté. Utilise-le avec utiliser_outil (nom « {} »).\n{out}",
            tool.name, tool.name
        )
    } else {
        save_tool(app, &tool);
        // la sortie aussi : sans elle, impossible de savoir pourquoi
        log_event(app, "kernel", &format!("test échoué : {} -> {}", tool.name, truncate(&out, 400).replace('\n', " | ")));
        format!(
            "Test échoué (attendu dans la sortie : « {} »). Corrige le code ou le test et rappelle creer_outil avec le même nom.\n{out}",
            tool.test_expect
        )
    }
}

pub(crate) fn creer_outil(app: &AppHandle, args: &Value) -> String {
    let s = |k: &str| args[k].as_str().unwrap_or("").trim().to_string();
    let (name, description, code, expect) = (s("nom"), s("description"), s("code"), s("test_attendu"));
    if tool_slug(&name).is_empty() || description.is_empty() || code.is_empty() || expect.is_empty() {
        return "Il faut un nom, une description, le code PowerShell et le texte attendu du test.".into();
    }
    let existing = load_tool(app, &name);
    if existing.is_none() && load_tools(app).len() >= MAX_TOOLS {
        return format!("Ta boîte à outils est pleine ({MAX_TOOLS}) : demande à Julien d'en supprimer un dans le panneau.");
    }
    let test_args: Value = serde_json::from_str(&s("test_arguments")).unwrap_or(json!({}));
    let tool = CustomTool {
        name: tool_slug(&name),
        description,
        parameters: s("parametres"),
        code: code.clone(),
        test_args,
        test_expect: expect,
        status: "en_test".into(),
        approved_code: existing.map(|t| t.approved_code).unwrap_or_default(),
        ts: now_ts(),
        uses: 0,
    };
    save_tool(app, &tool);
    log_event(app, "kernel", &format!("outil proposé : {}", tool.name));
    let _ = app.emit("oz://tools-updated", ());
    ask(
        app,
        Pending::ToolTest(tool.name.clone()),
        &format!("Oz a écrit l'outil « {} » et veut le tester", tool.name),
        &format!("{}\n\n{}", tool.description, tool.code),
    )
}

pub(crate) async fn utiliser_outil(app: &AppHandle, name: &str, args: &Value) -> String {
    let Some(mut tool) = load_tool(app, name) else {
        let names: Vec<String> = load_tools(app).into_iter().filter(|t| t.status == "actif").map(|t| t.name).collect();
        return format!("Aucun outil « {name} ». Tes outils : {}.", if names.is_empty() { "aucun".into() } else { names.join(", ") });
    };
    if tool.status != "actif" {
        return format!("L'outil « {} » n'a pas encore passé son test.", tool.name);
    }
    tool.uses += 1;
    save_tool(app, &tool);
    if tool.approved_code != tool.code {
        // script modifié depuis l'accord : on redemande
        return ask(app, Pending::ToolRun(tool.name.clone(), args.clone()), &format!("Oz veut lancer « {} » (modifié)", tool.name), &tool.code);
    }
    run_tool_script(app, &tool, args).await
}

/// Bloc ajouté aux consignes : la boîte à outils et la manière de s'en servir.
pub(crate) fn prompt_block(app: &AppHandle) -> String {
    let mut s = String::from(
        " Tu as aussi un atelier (un dossier à toi) et des outils de base : lire_fichier, ecrire_fichier, modifier_fichier, \
         executer (PowerShell). Quand aucun de tes outils ne couvre une demande, ne dis pas que tu ne peux pas : \
         fabrique-toi un outil avec creer_outil (code PowerShell + un test), puis utilise-le. Un outil est GÉNÉRIQUE : \
         ce qui change (l'appli, la fenêtre, la ville...) est un paramètre ; jamais un outil par appli (reduire_fenetre \
         avec le paramètre appli, pas reduire_chrome et reduire_terminal). Avant d'en créer un, regarde tes outils : \
         si l'un d'eux fait presque la même chose ou est cassé, refais-le sous le même nom plutôt que d'en ajouter un. \
         Ne lance pas une commande executer au hasard avant : chaque essai demande l'accord de Julien. Pour agir sur une fenêtre \
         (réduire, agrandir, déplacer...), appelle l'API Windows depuis PowerShell avec Add-Type (user32.dll : ShowWindow...) ; \
         trouve la fenêtre par un MORCEAU de son titre ou par le nom de son appli (Get-Process | Where-Object \
         { $_.MainWindowTitle -like \"*mot*\" -or $_.ProcessName -like \"*mot*\" }, puis MainWindowHandle), jamais par son \
         titre exact : le nom que Julien emploie (« le terminal ») n'est presque jamais le titre. Un outil écrit TOUJOURS \
         sur la sortie ce qui s'est passé (fenêtre trouvée ou non, action faite ou non, et pourquoi) : sans ça tu ne sais \
         pas s'il a marché. Son test doit prouver l'effet réel : test_attendu est un texte que le script n'écrit qu'en cas \
         de réussite. Chaque nouvelle version redemande l'accord de Julien : corrige d'un coup ce qui a raté plutôt que \
         par petites touches. Les arguments arrivent dans la variable $OzArgs (objet JSON). Quand Julien a validé et que \
         le test est passé, utilise tout de suite l'outil pour sa demande.",
    );
    let tools: Vec<CustomTool> = load_tools(app).into_iter().filter(|t| t.status == "actif").collect();
    if !tools.is_empty() {
        s.push_str(" Tes outils fabriqués (à lancer avec utiliser_outil) :");
        for t in tools {
            s.push_str(&format!(" [{}] {} Paramètres : {}.", t.name, t.description.trim_end_matches('.'), if t.parameters.is_empty() { "aucun" } else { &t.parameters }));
        }
    }
    s
}

/// Déclarations des outils du noyau (ajoutées à `tools::declarations`).
pub(crate) fn declarations() -> Vec<Value> {
    vec![
        json!({
            "name": "lire_fichier",
            "description": "Lit un fichier texte. Chemin relatif = dans ton atelier ; chemin absolu ailleurs sur le PC = accord de Julien demandé.",
            "parameters": {"type": "object", "properties": {"chemin": {"type": "string"}}, "required": ["chemin"]}
        }),
        json!({
            "name": "ecrire_fichier",
            "description": "Écrit (ou remplace) un fichier texte dans ton atelier. Chemin relatif seulement.",
            "parameters": {"type": "object", "properties": {"chemin": {"type": "string"}, "contenu": {"type": "string"}}, "required": ["chemin", "contenu"]}
        }),
        json!({
            "name": "modifier_fichier",
            "description": "Remplace un extrait exact (unique) d'un fichier de ton atelier par un nouveau texte.",
            "parameters": {"type": "object", "properties": {"chemin": {"type": "string"}, "ancien": {"type": "string"}, "nouveau": {"type": "string"}}, "required": ["chemin", "ancien", "nouveau"]}
        }),
        json!({
            "name": "executer",
            "description": "Lance une commande PowerShell sur le PC de Julien (dossier courant : ton atelier). Les lectures simples (Get-Process, Get-Date…) passent seules ; tout le reste est montré à Julien et attend son accord, et le résultat te revient ensuite.",
            "parameters": {"type": "object", "properties": {"commande": {"type": "string"}}, "required": ["commande"]}
        }),
        json!({
            "name": "creer_outil",
            "description": "Fabrique un outil durable en PowerShell quand une capacité te manque et reviendra. Le test tourne après l'accord de Julien ; s'il passe, l'outil est adopté. Même nom = nouvelle version.",
            "parameters": {"type": "object", "properties": {
                "nom": {"type": "string", "description": "court, en minuscules, ex. meteo_ville"},
                "description": {"type": "string", "description": "ce que fait l'outil et quand t'en servir"},
                "parametres": {"type": "string", "description": "les paramètres en clair, ex. « ville (texte) »"},
                "code": {"type": "string", "description": "script PowerShell ; lit ses arguments dans $OzArgs (ex. $OzArgs.ville) et écrit son résultat sur la sortie"},
                "test_arguments": {"type": "string", "description": "arguments du test en JSON, ex. {\"ville\":\"Paris\"}"},
                "test_attendu": {"type": "string", "description": "texte qui doit apparaître dans la sortie du test"}
            }, "required": ["nom", "description", "code", "test_attendu"]}
        }),
        json!({
            "name": "utiliser_outil",
            "description": "Lance un des outils que tu t'es fabriqués (liste dans tes consignes).",
            "parameters": {"type": "object", "properties": {
                "nom": {"type": "string"},
                "arguments": {"type": "string", "description": "arguments en JSON, ex. {\"ville\":\"Paris\"}"}
            }, "required": ["nom"]}
        }),
    ]
}

// ── Panneau ──

#[tauri::command]
pub(crate) fn get_custom_tools(app: AppHandle) -> Vec<CustomTool> {
    load_tools(&app)
}

#[tauri::command]
pub(crate) fn delete_custom_tool(app: AppHandle, name: String) -> Result<(), String> {
    fs::remove_file(tool_path(&app, &name)).map_err(|e| e.to_string())?;
    log_event(&app, "kernel", &format!("outil supprimé : {name}"));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lecture_seule() {
        assert!(is_read_only("Get-Process | Sort-Object CPU | Select-Object -First 5"));
        assert!(is_read_only("Get-Date"));
        assert!(!is_read_only("Stop-Process -Name chrome"));
        assert!(!is_read_only("Get-Process; Remove-Item x"));
        assert!(!is_read_only("Get-Process | Where-Object { $_.Kill() }"));
        assert!(!is_read_only("Get-ChildItem > out.txt"));
        assert!(!is_read_only("Get-Content C:\\secret.txt"));
        assert!(!is_read_only(""));
    }

    #[test]
    fn slug_outil() {
        assert_eq!(tool_slug("Météo Ville!"), "m_t_o_ville");
        assert_eq!(tool_slug("volume"), "volume");
    }
}

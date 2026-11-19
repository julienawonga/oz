//! Volume du système (sortie audio par défaut), via l'API audio de Windows.
//!
//! Oz parle par les haut-parleurs : couper le son le ferait taire au
//! milieu de sa phrase. La coupure est donc confiée à l'œuf, qui attend
//! la fin de sa réponse puis appelle `mute_system`.

use serde_json::json;
use tauri::{AppHandle, Emitter};
use windows::Win32::Media::Audio::Endpoints::IAudioEndpointVolume;
use windows::Win32::Media::Audio::{eConsole, eRender, IMMDeviceEnumerator, MMDeviceEnumerator};
use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CLSCTX_ALL, COINIT_MULTITHREADED};

use crate::journal::log_event;

/// Exécute `f` sur le point de sortie par défaut, dans un fil dédié
/// (initialisation COM propre, sans toucher aux fils de Tauri).
fn with_endpoint<T: Send + 'static>(
    f: impl FnOnce(&IAudioEndpointVolume) -> windows::core::Result<T> + Send + 'static,
) -> Result<T, String> {
    std::thread::spawn(move || unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        let enumerator: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
        let device = enumerator.GetDefaultAudioEndpoint(eRender, eConsole)?;
        let vol: IAudioEndpointVolume = device.Activate(CLSCTX_ALL, None)?;
        f(&vol)
    })
    .join()
    .map_err(|_| "le fil audio a planté".to_string())?
    .map_err(|e| format!("volume inaccessible ({e})"))
}

/// Niveau (0-100) et état muet.
fn read() -> Result<(u32, bool), String> {
    with_endpoint(|v| unsafe {
        let level = v.GetMasterVolumeLevelScalar()?;
        let muted = v.GetMute()?.as_bool();
        Ok(((level * 100.0).round() as u32, muted))
    })
}

/// Règle le niveau (0-100) ; un réglage au-dessus de 0 remet le son.
fn set(level: i64) -> Result<u32, String> {
    let level = level.clamp(0, 100) as u32;
    with_endpoint(move |v| unsafe {
        v.SetMasterVolumeLevelScalar(level as f32 / 100.0, std::ptr::null())?;
        if level > 0 {
            v.SetMute(false, std::ptr::null())?;
        }
        Ok(level)
    })
}

fn set_mute(mute: bool) -> Result<(), String> {
    with_endpoint(move |v| unsafe { v.SetMute(mute, std::ptr::null()) })
}

/// Outil `volume`. `valeur` : pas pour monter/baisser (10 par défaut),
/// niveau cible pour regler.
pub(crate) fn tool(app: &AppHandle, action: &str, valeur: Option<i64>) -> String {
    let result = match action {
        "niveau" => read().map(|(l, m)| {
            if m { format!("Le son est coupé (niveau {l} %).") } else { format!("Le volume est à {l} %.") }
        }),
        "monter" | "baisser" => read().and_then(|(l, _)| {
            let step = valeur.unwrap_or(10).abs();
            let target = if action == "monter" { l as i64 + step } else { l as i64 - step };
            set(target).map(|n| format!("Volume à {n} %."))
        }),
        "regler" => match valeur {
            Some(v) => set(v).map(|n| format!("Volume à {n} %.")),
            None => Err("précise le niveau voulu (0 à 100)".into()),
        },
        "couper" => {
            let _ = app.emit("oz://do", json!({"action": "mute"}));
            Ok("Le son sera coupé juste après ta réponse : fais-la très courte.".into())
        }
        "remettre" => set_mute(false).and_then(|_| read()).map(|(l, _)| format!("Son remis, volume à {l} %.")),
        other => Err(format!("action inconnue : {other}")),
    };
    result.unwrap_or_else(|e| format!("Impossible : {e}"))
}

/// Appelé par l'œuf une fois la réponse d'Oz terminée.
#[tauri::command]
pub(crate) fn mute_system(app: AppHandle) -> Result<(), String> {
    set_mute(true)?;
    log_event(&app, "volume", "son coupé");
    Ok(())
}

// ── Baisse automatique des autres applis pendant une conversation ──
// Comme Teams ou Discord : la vidéo qui tourne dans Chrome couvrait la
// voix de Julien (le micro la prenait pour lui). Chaque appli est réglée
// à part (sessions audio Windows) ; Oz et ses processus WebView2, qui
// portent sa voix, ne sont pas touchés.

use std::collections::HashSet;
use std::sync::Mutex;
use windows::core::Interface;
use windows::Win32::Foundation::CloseHandle;
use windows::Win32::Media::Audio::{IAudioSessionControl2, IAudioSessionManager2, ISimpleAudioVolume};
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS,
};

/// Part du volume d'origine gardée pendant la conversation : rien. Même à
/// 6 %, une vidéo couvrait encore la voix de Julien ; le son revient après.
const DUCK_RATIO: f32 = 0.0;
/// Volumes d'origine des sessions baissées : (pid, volume).
static DUCKED: Mutex<Vec<(u32, f32)>> = Mutex::new(Vec::new());

/// Oz et tous ses descendants (WebView2 : rendu, audio…).
fn own_processes() -> HashSet<u32> {
    let mut parents: Vec<(u32, u32)> = Vec::new();
    unsafe {
        if let Ok(snap) = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) {
            let mut e = PROCESSENTRY32W { dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32, ..Default::default() };
            if Process32FirstW(snap, &mut e).is_ok() {
                loop {
                    parents.push((e.th32ProcessID, e.th32ParentProcessID));
                    if Process32NextW(snap, &mut e).is_err() {
                        break;
                    }
                }
            }
            let _ = CloseHandle(snap);
        }
    }
    let mut own: HashSet<u32> = HashSet::from([std::process::id()]);
    loop {
        let before = own.len();
        for (pid, parent) in &parents {
            if own.contains(parent) {
                own.insert(*pid);
            }
        }
        if own.len() == before {
            return own;
        }
    }
}

/// Applique `f` à chaque session audio (pid, volume) de la sortie par défaut.
fn for_each_session(
    f: impl Fn(u32, &ISimpleAudioVolume) -> windows::core::Result<()> + Send + 'static,
) -> Result<(), String> {
    std::thread::spawn(move || unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        let enumerator: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
        let device = enumerator.GetDefaultAudioEndpoint(eRender, eConsole)?;
        let manager: IAudioSessionManager2 = device.Activate(CLSCTX_ALL, None)?;
        let sessions = manager.GetSessionEnumerator()?;
        for i in 0..sessions.GetCount()? {
            let Ok(control) = sessions.GetSession(i) else { continue };
            let (Ok(c2), Ok(vol)) = (control.cast::<IAudioSessionControl2>(), control.cast::<ISimpleAudioVolume>()) else {
                continue;
            };
            let pid = c2.GetProcessId().unwrap_or(0);
            if pid != 0 {
                let _ = f(pid, &vol);
            }
        }
        Ok(())
    })
    .join()
    .map_err(|_| "le fil audio a planté".to_string())?
    .map_err(|e: windows::core::Error| format!("sessions audio inaccessibles ({e})"))
}

/// Volumes d'origine aussi sur le disque : si Oz est arrêté de force en
/// pleine conversation (plantage, arrêt du dev), les applis restaient
/// muettes. Au démarrage suivant, il les remet.
static DUCK_FILE: Mutex<Option<std::path::PathBuf>> = Mutex::new(None);

fn persist(saved: &[(u32, f32)]) {
    if let Some(path) = DUCK_FILE.lock().unwrap().clone() {
        if saved.is_empty() {
            let _ = std::fs::remove_file(path);
        } else {
            let _ = std::fs::write(path, serde_json::to_string(saved).unwrap_or_default());
        }
    }
}

fn duck() -> Result<usize, String> {
    if !DUCKED.lock().unwrap().is_empty() {
        return Ok(0); // déjà baissé
    }
    let own = own_processes();
    let saved = std::sync::Arc::new(Mutex::new(Vec::new()));
    let out = saved.clone();
    for_each_session(move |pid, vol| unsafe {
        if own.contains(&pid) {
            return Ok(());
        }
        let level = vol.GetMasterVolume()?;
        if level > 0.0 {
            vol.SetMasterVolume(level * DUCK_RATIO, std::ptr::null())?;
            out.lock().unwrap().push((pid, level));
        }
        Ok(())
    })?;
    let saved = std::mem::take(&mut *saved.lock().unwrap());
    let n = saved.len();
    persist(&saved);
    *DUCKED.lock().unwrap() = saved;
    Ok(n)
}

fn restore() -> Result<usize, String> {
    let saved = std::mem::take(&mut *DUCKED.lock().unwrap());
    if saved.is_empty() {
        return Ok(0);
    }
    let n = saved.len();
    for_each_session(move |pid, vol| unsafe {
        if let Some((_, level)) = saved.iter().find(|(p, _)| *p == pid) {
            // seulement si l'appli est encore au niveau où Oz l'a mise :
            // si Julien l'a remontée entre-temps, on n'y touche pas
            if vol.GetMasterVolume()? <= level * DUCK_RATIO + 0.01 {
                vol.SetMasterVolume(*level, std::ptr::null())?;
            }
        }
        Ok(())
    })?;
    persist(&[]);
    Ok(n)
}

/// Appelé par l'œuf : `on` à l'ouverture du micro, `!on` à sa fermeture.
#[tauri::command]
pub(crate) fn duck_others(app: AppHandle, on: bool) -> Result<(), String> {
    let n = if on { duck()? } else { restore()? };
    if n > 0 {
        log_event(&app, "volume", &format!("{} {n} appli(s)", if on { "coupe le son de" } else { "remet le son de" }));
    }
    Ok(())
}

/// Au démarrage : remet le son laissé coupé par un arrêt forcé.
pub(crate) fn recover_at_startup(app: &AppHandle) {
    let path = tauri::Manager::path(app).app_data_dir().expect("app_data_dir").join("ducked.json");
    let left: Vec<(u32, f32)> = std::fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default();
    *DUCK_FILE.lock().unwrap() = Some(path);
    if left.is_empty() {
        return;
    }
    *DUCKED.lock().unwrap() = left;
    match restore() {
        Ok(n) => log_event(app, "volume", &format!("son remis après un arrêt en pleine conversation ({n} appli(s))")),
        Err(e) => log_event(app, "volume", &format!("son à remettre à la main : {e}")),
    }
}

/// À la fermeture d'Oz : ne jamais laisser les autres applis coupées.
pub(crate) fn restore_on_exit() {
    let _ = restore();
}

// ── Toolset « Son » ──

pub(crate) fn toolsets() -> Vec<crate::toolsets::Toolset> {
    vec![crate::toolsets::Toolset {
        id: "sound",
        label: "Son",
        description: "Régler le volume de l'ordinateur à la voix.",
        core: false,
        tools: vec![serde_json::json!({
            "name": "volume",
            "description": "Volume du son de l'ordinateur. monter / baisser : de 10 points par défaut, ou de valeur. regler : met le volume à valeur (0 à 100). niveau : dit où en est le volume. couper : coupe le son juste après ta réponse (dis un mot très bref). remettre : remet le son.",
            "parameters": {"type": "object", "properties": {
                "action": {"type": "string", "enum": ["monter", "baisser", "regler", "niveau", "couper", "remettre"]},
                "value": {"type": "integer", "description": "Pas en points pour monter/baisser, niveau cible pour regler."}
            }, "required": ["action"]}
        })],
    }]
}

pub(crate) fn run(app: &AppHandle, name: &str, args: &serde_json::Value) -> Option<String> {
    (name == "volume").then(|| tool(app, args["action"].as_str().unwrap_or("").trim(), args["value"].as_i64()))
}

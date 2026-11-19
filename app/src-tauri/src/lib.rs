// ── Oz · backend Tauri ──────────────────────────────────────────────
// Le cœur du contrat de confiance : les permissions s'appliquent DANS LE
// CODE (module `broker`), jamais dans un prompt.
//
//  windows_api : Win32 (fenêtres, capture, actions)     cursor  : curseur global
//  broker      : permissions, routeur d'intentions      journal : quittances
//  brain       : LLM (Ollama / Gemini), chat streaming  memory  : faits, épisodes, album
//  vision      : capture + description                  zones   : zones interdites
//  voice       : Gemini Live, micro, TTS, whisper
//  initiative  : Oz engage la conversation             prefs   : réglages de comportement
//  watch       : « Regarde avec moi » (vue en direct)   places  : lieux nommés

mod brain;
mod broker;
mod cursor;
mod initiative;
mod journal;
mod kernel;
mod memory;
mod places;
mod prefs;
mod skills;
mod tools;
mod vision;
mod voice;
mod volume;
mod watch;
pub mod windows_api;
mod zones;

use std::sync::Mutex;
use tauri::Manager;

/// Le projet s'appelait Nid : on reprend ses données (mémoire, permissions, config)
/// dans le nouveau dossier, une seule fois, sans rien supprimer de l'ancien.
fn migrate_legacy_data(app: &tauri::AppHandle) {
    fn copy_dir(from: &std::path::Path, to: &std::path::Path) -> std::io::Result<()> {
        std::fs::create_dir_all(to)?;
        for entry in std::fs::read_dir(from)? {
            let entry = entry?;
            let target = to.join(entry.file_name());
            if entry.file_type()?.is_dir() {
                copy_dir(&entry.path(), &target)?;
            } else {
                std::fs::copy(entry.path(), target)?;
            }
        }
        Ok(())
    }
    let Ok(new_dir) = app.path().app_data_dir() else { return };
    let Some(parent) = new_dir.parent() else { return };
    let old_dir = parent.join("com.julienawonga.nid");
    if old_dir.is_dir() && !new_dir.exists() {
        let _ = copy_dir(&old_dir, &new_dir);
    }
}

// ═══════════════════════ Démarrage ═══════════════════════

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // TLS : installe explicitement le provider crypto (requis par rustls
    // quand plusieurs crates l'exigent — le WebSocket Gemini Live en a besoin)
    let _ = rustls::crypto::ring::default_provider().install_default();

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        // raccourci global du micro (réglable) : l'œuf l'ouvre comme un clic
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, _shortcut, event| {
                    if event.state() == tauri_plugin_global_shortcut::ShortcutState::Pressed {
                        let _ = tauri::Emitter::emit(app, "oz://shortcut-mic", ());
                    }
                })
                .build(),
        )
        .setup(|app| {
            migrate_legacy_data(app.handle());
            let perms = broker::load_permissions(app.handle());
            app.manage(broker::BrokerState(Mutex::new(perms)));
            let cfg = brain::load_brain_config(app.handle());
            app.manage(brain::BrainState {
                config: Mutex::new(cfg),
                history: Mutex::new(Vec::new()),
                since_consolidation: Mutex::new(0),
            });
            app.manage(zones::ZonesState(Mutex::new(zones::load_zones(app.handle()))));
            let prefs = prefs::load_prefs(app.handle());
            if let Err(e) = prefs::apply_shortcut(app.handle(), &prefs.mic_shortcut) {
                journal::log_event(app.handle(), "prefs", &e);
            }
            let behind = prefs.behind;
            app.manage(prefs::PrefsState(Mutex::new(prefs)));
            if behind {
                let _ = tools::apply_layer(app.handle(), true);
            }
            initiative::spawn(app.handle().clone());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            windows_api::list_windows,
            broker::get_permissions,
            broker::set_permission,
            cursor::cursor_in_window,
            brain::chat,
            brain::flush_memory,
            brain::get_brain_config,
            brain::set_brain_config,
            voice::transcribe,
            voice::speak,
            voice::stop_voice,
            vision::look_at,
            zones::get_zones,
            zones::set_zones,
            journal::get_journal,
            memory::add_memory,
            memory::get_memories,
            memory::clear_memory,
            memory::get_facts,
            memory::forget_fact,
            volume::mute_system,
            volume::duck_others,
            skills::get_skills,
            kernel::exec_decision,
            kernel::get_custom_tools,
            kernel::delete_custom_tool,
            skills::save_skill,
            skills::delete_skill,
            voice::voice_feed,
            voice::voice_end,
            voice::mic_chunk,
            voice::mic_end,
            voice::live_prompt,
            prefs::get_prefs,
            prefs::set_prefs,
            initiative::initiative_result,
            initiative::initiative_try
        ])
        .build(tauri::generate_context!())
        .expect("error while running tauri application")
        .run(|_, event| {
            // ne jamais laisser les autres applis baissées après Oz
            if let tauri::RunEvent::Exit = event {
                volume::restore_on_exit();
            }
        });
}

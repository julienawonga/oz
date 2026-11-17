// ── Nid · backend Tauri ──────────────────────────────────────────────
// Le cœur du contrat de confiance : les permissions s'appliquent DANS LE
// CODE (module `broker`), jamais dans un prompt.
//
//  windows_api : Win32 (fenêtres, capture, actions)     cursor  : curseur global
//  broker      : permissions, routeur d'intentions      journal : quittances
//  brain       : LLM (Ollama / Gemini), chat streaming  memory  : faits, épisodes, album
//  vision      : capture + description                  zones   : zones interdites
//  voice       : Gemini Live, micro, TTS, whisper

mod brain;
mod broker;
mod cursor;
mod journal;
mod memory;
mod vision;
mod voice;
pub mod windows_api;
mod zones;

use std::sync::Mutex;
use tauri::Manager;

// ═══════════════════════ Démarrage ═══════════════════════

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // TLS : installe explicitement le provider crypto (requis par rustls
    // quand plusieurs crates l'exigent — le WebSocket Gemini Live en a besoin)
    let _ = rustls::crypto::ring::default_provider().install_default();

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let perms = broker::load_permissions(app.handle());
            app.manage(broker::BrokerState(Mutex::new(perms)));
            let cfg = brain::load_brain_config(app.handle());
            app.manage(brain::BrainState {
                config: Mutex::new(cfg),
                history: Mutex::new(Vec::new()),
                since_consolidation: Mutex::new(0),
            });
            app.manage(zones::ZonesState(Mutex::new(zones::load_zones(app.handle()))));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            windows_api::list_windows,
            broker::get_permissions,
            broker::set_permission,
            cursor::get_cursor_pos,
            brain::chat,
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
            voice::voice_feed,
            voice::voice_end,
            voice::mic_chunk,
            voice::mic_end
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

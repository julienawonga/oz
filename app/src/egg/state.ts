import { invoke } from "@tauri-apps/api/core";

// ── État partagé de l'œuf ────────────────────────────────────────────
// Un petit objet mutable commun à tous les modules : évite les imports
// circulaires (chacun lit/écrit ici au lieu de s'importer mutuellement).

export const state = {
  /** Nid réfléchit (requête chat / vision en cours). */
  thinking: false,
  /** La voix de Nid joue : sert de garde anti-écho pour le micro. */
  nidSpeaking: false,
  /** Le micro streame vers Gemini Live. */
  micRunning: false,
  /** Glisser-déposer de la fenêtre en cours. */
  dragging: false,
  /** Transcription de ce que dit Julien (accumulée par tour de parole). */
  userTurnText: "",
  /** Dernière phrase de Julien : rejouée après l'accord d'une permission. */
  lastUserIntent: "",
  /** Backend vocal gemini-live (lu depuis la config cerveau). */
  voiceLiveMode: false,
};

// mode voix gemini-live (lecture de la config cerveau)
void invoke("get_brain_config")
  .then((cfg) => {
    state.voiceLiveMode = String((cfg as { audio_model?: string }).audio_model ?? "").startsWith("gemini-live");
  })
  .catch(() => {});

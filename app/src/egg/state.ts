import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

// ── État partagé de l'œuf ────────────────────────────────────────────
// Un petit objet mutable commun à tous les modules : évite les imports
// circulaires (chacun lit/écrit ici au lieu de s'importer mutuellement).

export const state = {
  /** Oz réfléchit (requête chat / vision en cours). */
  thinking: false,
  /** La voix d'Oz joue : sert de garde anti-écho pour le micro. */
  ozSpeaking: false,
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
  /**
   * Live n'a le droit de parler que pour LIRE une réponse du cerveau.
   * Tant que `voiceOwned` est faux, l'audio spontané du modèle Live est jeté :
   * c'est le cerveau (broker, mémoire, actions) qui répond, jamais Live seul.
   */
  voiceOwned: false,
  /** voice_end a été envoyé : le prochain turn-complete clôt NOTRE réponse. */
  voiceClosed: false,
  /** Réglage : la bulle affiche la transcription de la conversation. */
  showTranscript: true,
  /** Oz appelle Julien : le micro ne doit pas se couper sur le silence. */
  holdMic: false,
  /** Une figure (zigzag, toupie...) est en cours : pas de balade autonome. */
  performing: false,
  /** libre, fixe, bord, concentration (réglage). */
  moveMode: "libre",
};

interface EggPrefs { show_transcript: boolean; move_mode: string; egg_color: string; egg_texture: string }
const listeners: Array<(p: EggPrefs, first: boolean) => void> = [];
let lastPrefs: EggPrefs | null = null;
/** Appelé à chaque changement de réglages (et une fois au démarrage). */
export function onPrefs(f: (p: EggPrefs, first: boolean) => void): void {
  listeners.push(f);
  if (lastPrefs) f(lastPrefs, true);
}

/** Couleur et texture de la coquille (variables CSS de styles.css). */
function applyAppearance(p: EggPrefs): void {
  const body = document.getElementById("robot-body");
  if (!body) return;
  if (p.egg_color) body.style.setProperty("--egg", p.egg_color);
  else body.style.removeProperty("--egg");
  body.className = body.className.replace(/\btexture-\S+/g, "").trim();
  if (p.egg_texture && p.egg_texture !== "lisse") body.classList.add(`texture-${p.egg_texture}`);
}

// réglages de comportement (onglet Réglages du panneau)
function refreshPrefs(): void {
  void invoke<EggPrefs>("get_prefs")
    .then((p) => {
      const first = lastPrefs === null;
      state.showTranscript = p.show_transcript;
      state.moveMode = p.move_mode || "libre";
      applyAppearance(p);
      lastPrefs = p;
      listeners.forEach((f) => f(p, first));
    })
    .catch(() => {});
}
refreshPrefs();
void listen("oz://prefs-updated", refreshPrefs);

// mode voix gemini-live (lecture de la config cerveau, relue à chaque modification)
function refreshBrainConfig(): void {
  void invoke("get_brain_config")
    .then((cfg) => {
      state.voiceLiveMode = String((cfg as { audio_model?: string }).audio_model ?? "").startsWith("gemini-live");
    })
    .catch(() => {});
}
refreshBrainConfig();
void listen("oz://brain-updated", refreshBrainConfig);

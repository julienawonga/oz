import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { closeAccessCard } from "./egg/access";
import { stopVoice } from "./egg/audio";
import "./egg/converse";
import { cancelInitiative, onEggClick } from "./egg/initiative";
import "./egg/mic";
import "./egg/motion";
import { bubble } from "./egg/ui";
import "./egg/window";

// ── Oz · le petit œuf (2D) ─────────────────────────────────────────
// Fenêtre transparente, toujours au premier plan, draggable.
// Point d'entrée : les modules de src/egg/ s'enregistrent à l'import.
//   state    état partagé            ui        DOM + bulle
//   window   drag / clic / click-through  initiative  Oz engage la conversation
//   audio    lecture vocale          mic       micro → Gemini Live
//   motion   poses / zones / balade  intents   commandes locales
//   converse chat streaming + voix   access    demandes d'accès

// Échap : coupe la parole, masque la bulle, ferme la carte
window.addEventListener("keydown", (e) => {
  if (e.key === "Escape") {
    stopVoice();
    void invoke("stop_voice").catch(() => {});
    bubble.classList.add("hidden");
    closeAccessCard();
    cancelInitiative();
  }
});


// Raccourci global (réglable dans le panneau) : même effet qu'un clic sur l'œuf
void listen("oz://shortcut-mic", () => onEggClick());

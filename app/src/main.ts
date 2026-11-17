import { invoke } from "@tauri-apps/api/core";
import { closeAccessCard } from "./egg/access";
import { stopVoice } from "./egg/audio";
import { hideChatBar } from "./egg/chatbar";
import "./egg/converse";
import "./egg/mic";
import "./egg/motion";
import { state } from "./egg/state";
import { bubble, say } from "./egg/ui";
import "./egg/window";

// ── Nid · le petit œuf (2D) ─────────────────────────────────────────
// Fenêtre transparente, toujours au premier plan, draggable.
// Point d'entrée : les modules de src/egg/ s'enregistrent à l'import.
//   state    état partagé            ui        DOM + bulle
//   window   drag / click-through    chatbar   saisie à la demande
//   audio    lecture vocale          mic       micro → Gemini Live
//   motion   poses / zones / balade  intents   commandes locales
//   converse chat streaming + voix   access    demandes d'accès

// Échap : coupe la parole, masque la bulle, ferme la carte et la barre
window.addEventListener("keydown", (e) => {
  if (e.key === "Escape") {
    stopVoice();
    void invoke("stop_voice").catch(() => {});
    bubble.classList.add("hidden");
    closeAccessCard();
    if (!state.micRunning) hideChatBar();
  }
});

setTimeout(() => say("Salut Julien, je suis Nid ! 🪺"), 800);

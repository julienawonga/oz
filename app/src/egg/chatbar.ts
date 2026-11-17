import { state } from "./state";
import { chatBar, chatInput } from "./ui";

// ── Barre de saisie : n'apparaît que sur demande ─────────────────────
let barIdleTimer = 0;

export function showChatBar(): void {
  chatBar.classList.remove("hidden");
  chatInput.focus();
  armBarIdle();
}

export function hideChatBar(): void {
  clearTimeout(barIdleTimer);
  chatBar.classList.add("hidden");
  chatInput.blur();
}

export function toggleChatBar(): void {
  if (chatBar.classList.contains("hidden")) showChatBar();
  else hideChatBar();
}

/** Se range seule après 8 s d'inactivité (sauf micro actif ou texte en cours). */
function armBarIdle(): void {
  clearTimeout(barIdleTimer);
  barIdleTimer = window.setTimeout(() => {
    if (state.micRunning || state.thinking || chatInput.value.trim()) return armBarIdle();
    hideChatBar();
  }, 8000);
}
chatInput.addEventListener("input", armBarIdle);
chatInput.addEventListener("focus", armBarIdle);

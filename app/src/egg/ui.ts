import { state } from "./state";

// ── Éléments du DOM + bulle de dialogue ──────────────────────────────

export const stage = document.getElementById("stage")!;
export const robot = document.getElementById("robot")!;
export const bubble = document.getElementById("speech-bubble")!;
export const chatBar = document.getElementById("chat-bar")!;
export const chatInput = document.getElementById("chat-input") as HTMLInputElement;
export const micBtn = document.getElementById("chat-mic")!;
export const accessBadge = document.getElementById("access-badge")!;
export const accessCard = document.getElementById("access-card")!;
export const eyeLeft = document.getElementById("eye-left")!;
export const eyeRight = document.getElementById("eye-right")!;

// Un SEUL minuteur : un nouveau message annule la disparition du précédent
// (avant, un ancien setTimeout pouvait masquer la bulle toute neuve).
let bubbleTimer = 0;

/** La bulle reste courte : les deux dernières phrases, 160 caractères max.
 *  Le texte complet est conservé dans le panneau d'administration. */
export function shortText(text: string): string {
  const t = text.trim();
  if (t.length <= 160) return t;
  const sentences = t.match(/[^.!?…\n]+[.!?…]*\s*/g) ?? [t];
  let out = sentences.slice(-2).join("").trim();
  if (out.length > 160) out = "…" + out.slice(-157).trimStart();
  return out;
}

export function showBubble(
  text: string,
  opts: { ttl?: number; sticky?: boolean; typing?: boolean } = {}
): void {
  clearTimeout(bubbleTimer);
  bubble.classList.toggle("typing", !!opts.typing);
  bubble.textContent = shortText(text);
  bubble.classList.remove("hidden");
  bubble.scrollTop = bubble.scrollHeight; // suit le texte qui défile
  if (opts.ttl) hideBubbleLater(opts.ttl);
  else if (opts.sticky) hideBubbleLater(6000 + text.length * 40); // filet de sécurité
}

export function hideBubbleLater(ms: number): void {
  clearTimeout(bubbleTimer);
  bubbleTimer = window.setTimeout(() => {
    // ne masque pas la bulle tant que Nid parle encore
    if (state.nidSpeaking || state.thinking) return hideBubbleLater(1200);
    bubble.classList.add("hidden");
    bubble.classList.remove("typing");
  }, ms);
}

export function say(text: string, duration = 4000): void {
  showBubble(text, { ttl: duration });
  // petite animation de bouche pendant le message (hors voix réelle)
  robot.classList.add("talking");
  window.setTimeout(() => {
    if (!state.nidSpeaking) robot.classList.remove("talking");
  }, Math.min(duration, 2500));
}

/** L'état "réfléchit" : bulle à points animés + barre de chat verrouillée. */
export function setThinking(on: boolean): void {
  state.thinking = on;
  chatBar.classList.toggle("busy", on);
  chatInput.disabled = on;
  if (on) {
    showBubble("", { typing: true });
  }
}

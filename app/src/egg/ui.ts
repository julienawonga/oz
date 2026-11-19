import { currentMonitor, getCurrentWindow } from "@tauri-apps/api/window";
import { state } from "./state";

// ── Éléments du DOM + bulle de dialogue ──────────────────────────────

export const stage = document.getElementById("stage")!;
export const robot = document.getElementById("robot")!;
export const bubble = document.getElementById("speech-bubble")!;
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
  void placeBubble();
  if (opts.ttl) hideBubbleLater(opts.ttl);
  else if (opts.sticky) hideBubbleLater(6000 + text.length * 40); // filet de sécurité
}

export function hideBubbleLater(ms: number): void {
  clearTimeout(bubbleTimer);
  bubbleTimer = window.setTimeout(() => {
    // ne masque pas la bulle tant qu'Oz parle encore
    if (state.ozSpeaking || state.thinking) return hideBubbleLater(1200);
    bubble.classList.add("hidden");
    bubble.classList.remove("typing");
  }, ms);
}

export function say(text: string, duration = 4000): void {
  showBubble(text, { ttl: duration });
  // petite animation de bouche pendant le message (hors voix réelle)
  robot.classList.add("talking");
  window.setTimeout(() => {
    if (!state.ozSpeaking) robot.classList.remove("talking");
  }, Math.min(duration, 2500));
}

/** Transcription de la conversation vocale : seulement si le réglage l'autorise. */
export function showTranscript(text: string, opts: { ttl?: number; sticky?: boolean } = {}): void {
  if (state.showTranscript) showBubble(text, opts);
}

// ── Bulle au-dessus de l'œuf, ou en dessous quand le haut de l'écran
// la couperait (l'œuf peut monter jusqu'au bord). Vérifié au plus toutes
// les 400 ms : la bulle se met à jour à chaque mot transcrit.
let lastPlaced = 0;
export async function placeBubble(force = false): Promise<void> {
  const now = performance.now();
  if (!force && now - lastPlaced < 400) return;
  lastPlaced = now;
  try {
    const [pos, mon] = await Promise.all([getCurrentWindow().outerPosition(), currentMonitor()]);
    const scale = mon?.scaleFactor ?? 1;
    const monY = mon?.position?.y ?? 0;
    const top = pos.y + (robot.getBoundingClientRect().top - 16 - bubble.offsetHeight) * scale;
    const below = top < monY + 4;
    bubble.classList.toggle("below", below);
    accessCard.classList.toggle("below", below);
  } catch {
    /* la bulle garde sa place */
  }
}

/** L'état "réfléchit" : bulle à points animés. */
export function setThinking(on: boolean): void {
  state.thinking = on;
  if (on) {
    showBubble("", { typing: true });
  }
}

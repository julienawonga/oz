import { robot } from "./ui";

// ── Émotions : le visage change et des bulles s'envolent ─────────────
// Gemini décide de l'émotion (outil express_emotion, aucun mot-clé) ;
// l'œuf la montre quelques secondes puis reprend son air habituel.

const EMOJI: Record<string, string[]> = {
  joie: ["😄", "✨"],
  rire: ["😂", "🤣"],
  colere: ["😠", "💢"],
  tristesse: ["💧", "😢"],
  surprise: ["😮", "❗"],
  amour: ["❤️", "💕"],
  douceur: ["🌸", "🌼"],
  gene: ["😳", "💦"],
  reflexion: ["❓", "🤔"],
  ennui: ["💤", "😪"],
  fierte: ["✨", "😎"],
  malice: ["😏", "😈"],
};
const MOODS = Object.keys(EMOJI);

const emotes = document.createElement("div");
emotes.id = "emotes";
emotes.setAttribute("aria-hidden", "true");
robot.appendChild(emotes);

let moodTimer = 0;

export function showEmotion(emotion: string, intensity = 2): void {
  const mood = MOODS.includes(emotion) ? emotion : "joie";
  const level = Math.max(1, Math.min(3, Math.round(intensity)));
  for (const m of MOODS) robot.classList.remove(`mood-${m}`);
  // relancer l'animation même si c'est la même émotion
  void robot.offsetWidth;
  robot.classList.add(`mood-${mood}`);
  clearTimeout(moodTimer);
  moodTimer = window.setTimeout(() => robot.classList.remove(`mood-${mood}`), 3000 + level * 1200);

  // des bulles d'émoji qui montent, plus nombreuses si l'émotion est forte
  const icons = EMOJI[mood];
  const count = level * 2;
  for (let i = 0; i < count; i++) {
    window.setTimeout(() => spawnBubble(icons[i % icons.length]), i * 260);
  }
}

function spawnBubble(icon: string): void {
  const b = document.createElement("span");
  b.className = "emote";
  b.textContent = icon;
  b.style.setProperty("--dx", `${Math.round((Math.random() - 0.5) * 70)}px`);
  b.style.setProperty("--rot", `${Math.round((Math.random() - 0.5) * 40)}deg`);
  b.style.left = `${40 + Math.random() * 20}%`;
  b.style.fontSize = `${16 + Math.random() * 10}px`;
  emotes.appendChild(b);
  b.addEventListener("animationend", () => b.remove());
}

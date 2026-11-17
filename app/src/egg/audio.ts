import { listen } from "@tauri-apps/api/event";
import { state } from "./state";
import { robot, say, showBubble } from "./ui";

// ── Lecture vocale streaming (Web Audio, chunks PCM 24 kHz) ──────────
let audioCtx: AudioContext | null = null;
function ensureAudio(): AudioContext {
  if (!audioCtx) audioCtx = new AudioContext();
  if (audioCtx.state === "suspended") void audioCtx.resume();
  return audioCtx;
}
// débloque l'autoplay dès la première interaction
window.addEventListener("pointerdown", () => ensureAudio());
window.addEventListener("keydown", () => ensureAudio());

let nextAudioTime = 0;
let nidSpeakTimer = 0;
const liveSources = new Set<AudioBufferSourceNode>();

/** Coupe net la voix en cours (interruption / Échap). */
export function stopVoice(): void {
  for (const src of liveSources) {
    try { src.stop(); } catch { /* déjà terminé */ }
  }
  liveSources.clear();
  nextAudioTime = 0;
  setSpeaking(false);
}

/** Un seul endroit pilote "Nid parle" : bouche + garde anti-écho. */
function setSpeaking(on: boolean): void {
  state.nidSpeaking = on;
  robot.classList.toggle("talking", on);
}

void listen<string>("nid://voice-reset", () => (nextAudioTime = 0));
void listen<string>("nid://voice-audio", (e) => playPcmChunk(e.payload));
void listen<string>("nid://voice-interrupted", () => stopVoice());
void listen<string>("nid://voice-error", (e) => {
  stopVoice();
  say("🎙️ " + String(e.payload).slice(0, 110), 8000);
});
void listen<string>("nid://turn-complete", () => {
  state.userTurnText = "";
  // l'audio déjà planifié finit de jouer ; la garde se relâche avec lui
});
void listen<string>("nid://user-text", (e) => {
  state.userTurnText += e.payload;
  showBubble("🧑 " + state.userTurnText.trim(), { sticky: true });
});
void listen<string>("nid://nid-text", (e) => {
  showBubble(e.payload, { sticky: true });
});

function playPcmChunk(b64: string): void {
  const ctx = ensureAudio();
  const bin = atob(b64);
  const i16 = new Int16Array(bin.length / 2);
  for (let i = 0; i < i16.length; i++) {
    i16[i] = (bin.charCodeAt(2 * i) | (bin.charCodeAt(2 * i + 1) << 8)) << 16 >> 16;
  }
  const buf = ctx.createBuffer(1, i16.length, 24000);
  const ch = buf.getChannelData(0);
  for (let i = 0; i < i16.length; i++) ch[i] = i16[i] / 32768;
  const src = ctx.createBufferSource();
  src.buffer = buf;
  src.connect(ctx.destination);
  const now = ctx.currentTime;
  if (nextAudioTime < now + 0.05) nextAudioTime = now + 0.05;
  src.start(nextAudioTime);
  liveSources.add(src);
  src.onended = () => liveSources.delete(src);
  nextAudioTime += buf.duration;
  // "parle" jusqu'à la fin RÉELLE de l'audio planifié (+ petite marge anti-écho)
  setSpeaking(true);
  clearTimeout(nidSpeakTimer);
  nidSpeakTimer = window.setTimeout(
    () => setSpeaking(false),
    Math.max(0, (nextAudioTime - ctx.currentTime) * 1000) + 350
  );
}

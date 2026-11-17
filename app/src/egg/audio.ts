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
let ozSpeakTimer = 0;
const liveSources = new Set<AudioBufferSourceNode>();

/** Vide la file audio sans toucher à la propriété de la voix. */
function stopAudioQueue(): void {
  for (const src of liveSources) {
    try { src.stop(); } catch { /* déjà terminé */ }
  }
  liveSources.clear();
  nextAudioTime = 0;
  setSpeaking(false);
}

/** Coupe net la voix en cours (Échap, nouvelle question, erreur). */
export function stopVoice(): void {
  stopAudioQueue();
  releaseVoice();
}

function releaseVoice(): void {
  clearTimeout(ownerTimer);
  state.voiceOwned = false;
  state.voiceClosed = false;
}

let ownerTimer = 0;

/** Appelé par converse.ts quand le cerveau va parler par la voix Live. */
export function claimVoice(): void {
  state.voiceOwned = true;
  state.voiceClosed = false;
}

/** voice_end envoyé : le prochain turn-complete est le nôtre.
 *  Filet de sécurité : la propriété expire seule si le serveur se tait. */
export function markVoiceClosed(): void {
  state.voiceClosed = true;
  clearTimeout(ownerTimer);
  ownerTimer = window.setTimeout(releaseVoice, 60000);
}

/** Envoie la parole transcrite de Julien au cerveau (une seule fois par tour). */
function routeUserSpeech(): void {
  const text = state.userTurnText.trim();
  if (state.routedThisTurn || !text) return;
  state.routedThisTurn = true;
  state.userTurnText = "";
  state.onUserSpeech?.(text);
}

/** Un seul endroit pilote "Oz parle" : bouche + garde anti-écho. */
function setSpeaking(on: boolean): void {
  state.ozSpeaking = on;
  robot.classList.toggle("talking", on);
}

void listen<string>("oz://voice-reset", () => (nextAudioTime = 0));
void listen<string>("oz://voice-audio", (e) => {
  if (state.voiceOwned) {
    playPcmChunk(e.payload);
  } else {
    // Live commence à répondre de lui-même : sa réponse est jetée, et ce
    // premier son signale que Julien a fini de parler -> on route vers le cerveau
    routeUserSpeech();
  }
});
// Le serveur coupe son tour (nouvelle entrée) : on vide la file, on garde la parole
void listen<string>("oz://voice-interrupted", () => stopAudioQueue());
void listen<string>("oz://voice-error", (e) => {
  stopVoice();
  say("🎙️ " + String(e.payload).slice(0, 110), 8000);
});
void listen<string>("oz://turn-complete", () => {
  if (state.voiceOwned) {
    // avant voice_end, un turn-complete appartient à la réponse jetée de Live
    if (state.voiceClosed) releaseVoice();
    return;
  }
  // tour de Live sans son : si la parole n'a pas encore été routée, on le fait ici
  routeUserSpeech();
  state.routedThisTurn = false; // le tour est fini, le prochain peut être routé
});
void listen<string>("oz://user-text", (e) => {
  if (state.routedThisTurn) state.routedThisTurn = false; // nouvelle phrase de Julien
  state.userTurnText += e.payload;
  showBubble("« " + state.userTurnText.trim() + " »", { sticky: true });
});
void listen<string>("oz://oz-text", (e) => {
  if (state.voiceOwned) showBubble(e.payload, { sticky: true });
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
  clearTimeout(ozSpeakTimer);
  ozSpeakTimer = window.setTimeout(
    () => setSpeaking(false),
    Math.max(0, (nextAudioTime - ctx.currentTime) * 1000) + 350
  );
}

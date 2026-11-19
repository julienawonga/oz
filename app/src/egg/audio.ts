import { emit, listen } from "@tauri-apps/api/event";
import { state } from "./state";
import { robot, say, showTranscript } from "./ui";

// ── Lecture vocale streaming (Web Audio, chunks PCM 24 kHz) ──────────
let audioCtx: AudioContext | null = null;
/** Toute la voix d'Oz passe par ici : on mesure ce qui sort des enceintes. */
let outBus: GainNode | null = null;
let outAnalyser: AnalyserNode | null = null;
const outSamples = new Float32Array(1024);
function ensureAudio(): AudioContext {
  if (!audioCtx) {
    audioCtx = new AudioContext();
    outBus = audioCtx.createGain();
    outAnalyser = audioCtx.createAnalyser();
    outAnalyser.fftSize = 1024;
    outBus.connect(outAnalyser);
    outAnalyser.connect(audioCtx.destination);
  }
  if (audioCtx.state === "suspended") void audioCtx.resume();
  return audioCtx;
}

/** Énergie (RMS) de la voix d'Oz en ce moment : sert à reconnaître, dans
 *  le micro, son propre écho de la voix de Julien. */
export function outputLevel(): number {
  if (!outAnalyser) return 0;
  outAnalyser.getFloatTimeDomainData(outSamples);
  let e = 0;
  for (let i = 0; i < outSamples.length; i++) e += outSamples[i] * outSamples[i];
  return Math.sqrt(e / outSamples.length);
}

/** Julien parle par-dessus Oz : on coupe sa voix tout de suite (le serveur
 *  coupera aussi son tour en entendant Julien). */
export function bargeIn(): void {
  stopAudioQueue();
  talkSilenced = true;
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
  talkSilenced = true; // la suite de la réponse vocale en cours ne sera pas jouée
}

// Réponse vocale coupée (Échap) : on jette la fin de son audio, jusqu'à
// ce que Julien reprenne la parole.
let talkSilenced = false;

/** Oz prend l'initiative : sa prochaine réponse doit être jouée,
 *  même si la précédente a été coupée. */
export function unsilenceTalk(): void {
  talkSilenced = false;
}

export function releaseVoice(): void {
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

/** Consigne donnée à Live : lire le texte du cerveau, pas y répondre. */
export const READ_ALOUD = "Lis à voix haute, mot pour mot, sans rien ajouter ni commenter, ce texte : ";

/** Un seul endroit pilote "Oz parle" : bouche + garde anti-écho. */
function setSpeaking(on: boolean): void {
  state.ozSpeaking = on;
  robot.classList.toggle("talking", on);
}

void listen<string>("oz://voice-reset", () => (nextAudioTime = 0));
// Session « parole » : tout son audio est la voix d'Oz. On ne le joue que
// tant que la réponse nous appartient (Échap / nouvelle question la coupent).
void listen<string>("oz://voice-audio", (e) => {
  if (state.voiceOwned) playPcmChunk(e.payload);
});
// Le serveur coupe son tour (nouvelle entrée) : on vide la file, on garde la parole
void listen<string>("oz://voice-interrupted", () => stopAudioQueue());
void listen<string>("oz://voice-error", (e) => {
  stopVoice();
  say("🎙️ " + String(e.payload).slice(0, 110), 8000);
});
// fin de la lecture : la voix est libre pour la prochaine réponse
void listen<string>("oz://voice-turn-complete", () => {
  if (state.voiceClosed) releaseVoice();
});
// ── Session « conversation » : Julien parle, Oz répond directement ──
// Transcription de Julien (affichée pendant qu'il parle)
void listen<string>("oz://voice-user-text", (e) => {
  talkSilenced = false; // Julien reprend la parole : la prochaine réponse sera jouée
  ozTalkText = "";
  state.userTurnText += e.payload;
  showTranscript("« " + state.userTurnText.trim() + " »", { sticky: true });
});
// Voix d'Oz, jouée au fil de l'eau
void listen<string>("oz://voice-talk-audio", (e) => {
  if (!talkSilenced) playPcmChunk(e.payload);
});
// Ce que dit Oz, affiché dans la bulle en même temps
let ozTalkText = "";
void listen<string>("oz://voice-talk-text", (e) => {
  if (talkSilenced) return;
  ozTalkText += e.payload;
  showTranscript(ozTalkText.trim(), { sticky: true });
});
// Fin d'un échange : bulle finale et miroir dans le panneau
void listen<{ user: string; oz: string }>("oz://voice-talk-turn", (e) => {
  const { user, oz } = e.payload;
  state.userTurnText = "";
  ozTalkText = "";
  if (user) {
    state.lastUserIntent = user; // rejouée si une permission est accordée ensuite
    void emit("oz://chat-line", { role: "user", text: user });
  }
  void emit("oz://chat-line", { role: "oz", text: oz });
  if (!talkSilenced) showTranscript(oz, { ttl: Math.min(14000, 3000 + oz.length * 60) });
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
  src.connect(outBus ?? ctx.destination);
  const now = ctx.currentTime;
  if (nextAudioTime < now + 0.05) nextAudioTime = now + 0.05;
  src.start(nextAudioTime);
  liveSources.add(src);
  src.onended = () => liveSources.delete(src);
  nextAudioTime += buf.duration;
  // "parle" jusqu'à la fin RÉELLE de l'audio : planifié + latence de sortie
  // (le son sort des enceintes après currentTime : 20 ms en filaire, jusqu'à
  // 300 ms en Bluetooth ; sans elle, le micro captait la fin de sa phrase)
  // + une marge pour l'écho de la pièce.
  setSpeaking(true);
  clearTimeout(ozSpeakTimer);
  const latencyMs = ((ctx.outputLatency || 0) + (ctx.baseLatency || 0)) * 1000;
  ozSpeakTimer = window.setTimeout(
    () => setSpeaking(false),
    Math.max(0, (nextAudioTime - ctx.currentTime) * 1000) + latencyMs + 300
  );
}

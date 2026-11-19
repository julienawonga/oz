import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { bargeIn, outputLevel, stopVoice } from "./audio";
import { state } from "./state";
import { robot, say } from "./ui";

// ── Micro STREAMING : ta voix part en continu vers Gemini Live ───────
let micStream: MediaStream | null = null;
let micProcessor: ScriptProcessorNode | null = null;
let micAudioCtx: AudioContext | null = null;

// ── Arrêt automatique : le micro ne reste jamais ouvert « par oubli » ──
const SILENCE_AFTER_SPEECH_MS = 8000; // après une phrase
const SILENCE_BEFORE_SPEECH_MS = 15000; // rien dit depuis l'ouverture
const VOICE_RMS = 0.015; // énergie minimale pour compter comme « de la voix »
let lastActivity = 0;

// ── Interrompre Oz à la voix ──
// Pendant qu'Oz parle, le micro entend son écho. On apprend en continu
// le rapport écho / voix d'Oz (le micro ÷ ce qui sort des enceintes) ;
// quand le micro dépasse nettement l'écho attendu pendant ~250 ms, c'est
// Julien qui parle : Oz se tait et les derniers morceaux, gardés en
// réserve, partent pour que le début de sa phrase ne soit pas perdu.
const BARGE_FRAMES = 3; // tampons de ~85 ms consécutifs
const BARGE_FACTOR = 3; // au-dessus de 3x l'écho attendu
let echoGain = 0.3;
let loudFrames = 0;
let backlog: string[] = [];
let bargeEnabled = true;
function refreshBargePref(): void {
  void invoke<{ barge_in?: boolean }>("get_prefs")
    .then((p) => (bargeEnabled = p.barge_in !== false))
    .catch(() => {});
}
refreshBargePref();
void listen("oz://prefs-updated", refreshBargePref);
let micStarting = false; // getUserMedia en cours : pas de double ouverture
let heardSomething = false;

function noteActivity(): void {
  lastActivity = performance.now();
  heardSomething = true;
}
// la transcription renvoyée par le serveur compte aussi comme activité
void listen<string>("oz://voice-user-text", noteActivity);

/** Clic sur l'œuf : il t'écoute ; s'il parlait, tu lui coupes la parole ;
 *  s'il écoutait déjà en silence, il arrête. */
export function toggleListening(): void {
  if (state.ozSpeaking) {
    stopVoice(); // le micro, déjà ouvert, reprend dès qu'Oz se tait
    if (!state.micRunning) void startMicStream();
    return;
  }
  if (state.micRunning) void stopMicStream();
  else void startMicStream();
}

function b64FromInt16(i16: Int16Array): string {
  const bytes = new Uint8Array(i16.buffer);
  let bin = "";
  const CHUNK = 0x8000;
  for (let i = 0; i < bytes.length; i += CHUNK) {
    bin += String.fromCharCode(...bytes.subarray(i, i + CHUNK));
  }
  return btoa(bin);
}

export async function startMicStream(): Promise<void> {
  if (state.micRunning || micStarting) return;
  micStarting = true;
  try {
    micStream = await navigator.mediaDevices.getUserMedia({
      audio: { channelCount: 1, echoCancellation: true, noiseSuppression: true },
    });
    micAudioCtx = new AudioContext();
    const srcNode = micAudioCtx.createMediaStreamSource(micStream);
    micProcessor = micAudioCtx.createScriptProcessor(4096, 1, 1);
    // le processeur doit être connecté à destination, via un gain muet (anti-écho)
    const mute = micAudioCtx.createGain();
    mute.gain.value = 0;
    srcNode.connect(micProcessor);
    micProcessor.connect(mute);
    mute.connect(micAudioCtx.destination);

    let micErr: string | null = null;
    micProcessor.onaudioprocess = (ev) => {
      if (!state.micRunning) return;
      const f32 = ev.inputBuffer.getChannelData(0);
      let energy = 0;
      for (let i = 0; i < f32.length; i++) energy += f32[i] * f32[i];
      const rms = Math.sqrt(energy / f32.length);
      // Oz parle : son écho n'est jamais envoyé, sauf si Julien parle par-dessus
      if (state.ozSpeaking) {
        lastActivity = performance.now(); // le silence ne compte qu'une fois Oz tu
        if (!bargeEnabled) return;
        const out = outputLevel();
        const expected = echoGain * out;
        const threshold = Math.max(VOICE_RMS * 2.5, expected * BARGE_FACTOR);
        if (rms > threshold) {
          loudFrames++;
        } else {
          loudFrames = 0;
          // pas de voix de Julien : on affine le rapport écho / voix d'Oz
          if (out > 0.005) echoGain = Math.min(2, Math.max(0.02, 0.9 * echoGain + 0.1 * (rms / out)));
        }
        backlog.push(encode(f32));
        if (backlog.length > BARGE_FRAMES + 2) backlog.shift();
        if (loudFrames >= BARGE_FRAMES) {
          loudFrames = 0;
          bargeIn();
          noteActivity();
          const pending = backlog;
          backlog = [];
          for (const data of pending) {
            void invoke("mic_chunk", { data }).catch(() => {});
          }
        }
        return;
      }
      loudFrames = 0;
      backlog = [];
      if (rms > VOICE_RMS) noteActivity();
      void invoke("mic_chunk", { data: encode(f32) }).catch((err) => {
        micErr = String(err).slice(0, 100);
      });
    };
    /** Micro (48 kHz flottant) -> PCM 16 bits 16 kHz en base64. */
    const encode = (f32: Float32Array): string => {
      const ratio = micAudioCtx!.sampleRate / 16000;
      const outLen = Math.floor(f32.length / ratio);
      if (outLen < 1) return "";
      const i16 = new Int16Array(outLen);
      // moyenne des échantillons de chaque fenêtre = filtre passe-bas simple
      // (la décimation brute créait de l'aliasing → transcriptions moins fiables)
      for (let i = 0; i < outLen; i++) {
        const from = Math.floor(i * ratio);
        const to = Math.max(from + 1, Math.floor((i + 1) * ratio));
        let sum = 0;
        for (let j = from; j < to && j < f32.length; j++) sum += f32[j];
        const s = (sum / (to - from)) * 32767;
        i16[i] = Math.max(-32768, Math.min(32767, Math.round(s)));
      }
      return b64FromInt16(i16);
    };
    state.micRunning = true;
    micStarting = false;
    void invoke("duck_others", { on: true }).catch(() => {}); // la vidéo ne couvre plus ta voix
    lastActivity = performance.now();
    heardSomething = false;
    robot.classList.add("listening"); // halo sur l'œuf : il t'écoute
    state.userTurnText = "";
    if (!state.holdMic) stopVoice(); // si Oz parlait, on lui coupe la parole : c'est ton tour
    const errWatch = setInterval(() => {
      if (micErr) {
        say("🎙️ " + micErr, 8000);
        clearInterval(errWatch);
        void stopMicStream();
      }
      if (!state.micRunning) {
        clearInterval(errWatch);
        return;
      }
      const limit = heardSomething ? SILENCE_AFTER_SPEECH_MS : SILENCE_BEFORE_SPEECH_MS;
      if (!state.thinking && !state.holdMic && performance.now() - lastActivity > limit) {
        clearInterval(errWatch);
        void stopMicStream();
        say("Je n'entends plus rien, j'arrête d'écouter.", 3500);
      }
    }, 500);
  } catch (err) {
    console.error("micro:", err);
    micStarting = false;
    say("Je n'ai pas accès au micro 🙈 — autorise-le dans Windows (Confidentialité › Microphone)", 6000);
  }
}

export async function stopMicStream(): Promise<void> {
  state.micRunning = false;
  robot.classList.remove("listening");
  micProcessor?.disconnect();
  micStream?.getTracks().forEach((t) => t.stop());
  micStream = null;
  void micAudioCtx?.close();
  micAudioCtx = null;
  await invoke("mic_end").catch(() => {}); // fin du flux → Gemini répond
  // les autres applis remontent après la réponse d'Oz, pas pendant
  void restoreOthersAfterReply();
}

/** Remet le son des autres applis une fois qu'Oz a fini de parler (sauf
 *  si une nouvelle écoute a commencé entre-temps). */
async function restoreOthersAfterReply(): Promise<void> {
  const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));
  for (let waited = 0; waited < 4000 && !state.ozSpeaking; waited += 100) await sleep(100);
  for (let waited = 0; waited < 30_000 && state.ozSpeaking; waited += 100) await sleep(100);
  if (state.micRunning) return;
  await invoke("duck_others", { on: false }).catch(() => {});
  // conversation finie : ce qui s'est dit entre en mémoire tout de suite
  await invoke("flush_memory").catch(() => {});
}

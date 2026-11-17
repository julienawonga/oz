import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { stopVoice } from "./audio";
import { state } from "./state";
import { micBtn, robot, say, showBubble } from "./ui";

// ── Micro STREAMING : ta voix part en continu vers Gemini Live ───────
let micStream: MediaStream | null = null;
let micProcessor: ScriptProcessorNode | null = null;
let micAudioCtx: AudioContext | null = null;

// ── Arrêt automatique : le micro ne reste jamais ouvert « par oubli » ──
const SILENCE_AFTER_SPEECH_MS = 8000; // après une phrase
const SILENCE_BEFORE_SPEECH_MS = 15000; // rien dit depuis l'ouverture
const VOICE_RMS = 0.015; // énergie minimale pour compter comme « de la voix »
let lastActivity = 0;
let heardSomething = false;

function noteActivity(): void {
  lastActivity = performance.now();
  heardSomething = true;
}
// la transcription renvoyée par le serveur compte aussi comme activité
void listen<string>("nid://user-text", noteActivity);

micBtn.addEventListener("click", (e) => {
  e.stopPropagation();
  if (state.micRunning) void stopMicStream();
  else void startMicStream();
});

// la session Live s'est fermée : on remet l'interface au propre,
// elle se rouvrira toute seule au prochain message / à la prochaine parole
void listen<string>("nid://voice-disconnected", () => {
  stopVoice();
  if (state.micRunning) {
    void stopMicStream();
    say("La connexion vocale s'est coupée — relance le micro 🎙️", 5000);
  }
});

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
  try {
    micBtn.classList.add("connecting");
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
      // si Nid parle, on coupe le micro (anti-écho de sa propre voix)
      if (state.nidSpeaking) {
        lastActivity = performance.now(); // le silence ne compte qu'une fois Nid tu
        return;
      }
      const f32 = ev.inputBuffer.getChannelData(0);
      let energy = 0;
      for (let i = 0; i < f32.length; i++) energy += f32[i] * f32[i];
      if (Math.sqrt(energy / f32.length) > VOICE_RMS) noteActivity();
      const ratio = micAudioCtx!.sampleRate / 16000;
      const outLen = Math.floor(f32.length / ratio);
      if (outLen < 1) return;
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
      void invoke("mic_chunk", { data: b64FromInt16(i16) }).catch((err) => {
        micErr = String(err).slice(0, 100);
      });
    };
    state.micRunning = true;
    lastActivity = performance.now();
    heardSomething = false;
    robot.classList.add("listening"); // halo visible sur l'œuf, même barre rangée
    micBtn.classList.remove("connecting");
    micBtn.classList.add("recording");
    micBtn.title = "Arrêter l'écoute";
    state.userTurnText = "";
    stopVoice(); // si Nid parlait, on lui coupe la parole : c'est ton tour
    showBubble("Je t'écoute... 🎙️");
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
      if (!state.thinking && performance.now() - lastActivity > limit) {
        clearInterval(errWatch);
        void stopMicStream();
        say("Je n'entends plus rien, j'arrête d'écouter.", 3500);
      }
    }, 500);
  } catch (err) {
    console.error("micro:", err);
    micBtn.classList.remove("connecting");
    say("Je n'ai pas accès au micro 🙈 — autorise-le dans Windows (Confidentialité › Microphone)", 6000);
  }
}

export async function stopMicStream(): Promise<void> {
  state.micRunning = false;
  robot.classList.remove("listening");
  micBtn.title = "Parler";
  micBtn.classList.remove("recording");
  micProcessor?.disconnect();
  micStream?.getTracks().forEach((t) => t.stop());
  micStream = null;
  void micAudioCtx?.close();
  micAudioCtx = null;
  await invoke("mic_end").catch(() => {}); // fin du flux → Gemini répond
}

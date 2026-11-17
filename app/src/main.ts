import { getCurrentWindow, currentMonitor, PhysicalPosition } from "@tauri-apps/api/window";
import { invoke, Channel } from "@tauri-apps/api/core";
// mode voix gemini-live (lecture de la config cerveau)
let voiceLiveMode = false;
void invoke("get_brain_config")
  .then((cfg) => {
    voiceLiveMode = String((cfg as { audio_model?: string }).audio_model ?? "").startsWith("gemini-live");
  })
  .catch(() => {});
import { listen, emit } from "@tauri-apps/api/event";

// ── Nid · le petit œuf (2D) ─────────────────────────────────────────
// Fenêtre transparente, toujours au premier plan, draggable.
// Toutes les fonctions du harnais sont actives : chat streaming,
// voix, permissions, actions, autonomie. Le rendu 3D VRM reviendra
// plus tard avec un modèle définitif.

const stage = document.getElementById("stage")!;
const robot = document.getElementById("robot")!;
const bubble = document.getElementById("speech-bubble")!;
const chatBar = document.getElementById("chat-bar")!;
const chatInput = document.getElementById("chat-input") as HTMLInputElement;
const micBtn = document.getElementById("chat-mic")!;
const accessBadge = document.getElementById("access-badge")!;
const accessCard = document.getElementById("access-card")!;
const eyeLeft = document.getElementById("eye-left")!;
const eyeRight = document.getElementById("eye-right")!;

// ── 0. Drag manuel DPI-aware ─────────────────────────────────────────
let dragging = false;
let dragStart = { wx: 0, wy: 0, sx: 0, sy: 0, scale: 1 };

stage.addEventListener("pointerdown", async (e) => {
  if (e.button !== 0) return;
  e.preventDefault();
  const win = getCurrentWindow();
  const [pos, mon] = await Promise.all([win.outerPosition(), currentMonitor()]);
  dragStart = {
    wx: pos.x,
    wy: pos.y,
    sx: e.screenX,
    sy: e.screenY,
    scale: mon?.scaleFactor ?? 1,
  };
  dragging = true;
  downAt = { t: performance.now(), x: e.screenX, y: e.screenY };
});

let downAt = { t: 0, x: 0, y: 0 };

window.addEventListener("pointermove", (e) => {
  if (!dragging) return;
  const dx = Math.round((e.screenX - dragStart.sx) * dragStart.scale);
  const dy = Math.round((e.screenY - dragStart.sy) * dragStart.scale);
  getCurrentWindow().setPosition(
    new PhysicalPosition(dragStart.wx + dx, dragStart.wy + dy)
  );
});

window.addEventListener("pointerup", (e) => {
  if (!dragging) return;
  dragging = false;
  // simple clic (peu de mouvement, bref) -> ouvre / ferme la barre de saisie
  const moved = Math.hypot(e.screenX - downAt.x, e.screenY - downAt.y);
  if (moved < 5 && performance.now() - downAt.t < 350) {
    toggleChatBar();
    return;
  }
  // 🚫 Déposé dans une zone interdite ? Il s'échappe tout seul
  void (async () => {
    const win = getCurrentWindow();
    const [pos, size] = await Promise.all([win.outerPosition(), win.outerSize()]);
    const bad = overlapsZone(pos.x, pos.y, size.width, size.height);
    if (bad) {
      say("🚫 C'est ma zone interdite, je ne reste pas là !", 4000);
      await glideTo(); // pas de cible → reste sur place MAIS cherche la place libre
      void glideTo(pos.x + size.width + 20, pos.y); // puis dégage horizontalement
    }
  })();
});

function moveWindowTo(x: number, y: number): void {
  getCurrentWindow().setPosition(new PhysicalPosition(Math.round(x), Math.round(y)));
}

// ── 0bis. Click-through : la zone active épouse l'œuf ────────────────
let cursorOverRobot = true;

async function hitTestLoop(): Promise<void> {
  if (dragging) return;
  try {
    const win = getCurrentWindow();
    const [cursor, outer, mon] = await Promise.all([
      invoke<{ x: number; y: number }>("get_cursor_pos"),
      win.outerPosition(),
      currentMonitor(),
    ]);
    const scale = mon?.scaleFactor ?? 1;
    const lx = (cursor.x - outer.x) / scale;
    const ly = (cursor.y - outer.y) / scale;

    // Zone active : l'œuf + la bulle + la barre de chat
    const overUi = [robot, bubble, chatBar, accessCard].some((el) => {
      // un élément masqué ne doit pas bloquer les clics derrière lui
      if (el.classList.contains("hidden")) return false;
      const r = el.getBoundingClientRect();
      return lx >= r.left && lx <= r.right && ly >= r.top && ly <= r.bottom;
    });
    if (overUi !== cursorOverRobot) {
      cursorOverRobot = overUi;
      await win.setIgnoreCursorEvents(!overUi);
    }

    // Les pupilles suivent le curseur GLOBAL
    updatePupils(lx, ly);
  } catch {
    /* on réessaie au prochain tick */
  }
}
setInterval(hitTestLoop, 60);

// Pupilles qui suivent le curseur
function updatePupils(lx: number, ly: number): void {
  const r = robot.getBoundingClientRect();
  const cx = r.left + r.width / 2;
  const cy = r.top + r.height / 2;
  const dx = Math.max(-1, Math.min(1, (lx - cx) / 300));
  const dy = Math.max(-1, Math.min(1, (ly - cy) / 300));
  for (const eye of [eyeLeft, eyeRight]) {
    const pupil = eye.querySelector(".pupil") as HTMLElement | null;
    if (pupil) pupil.style.transform = `translate(${dx * 4}px, ${dy * 4}px)`;
  }
}

// ── Barre de saisie : n'apparaît que sur demande ─────────────────────
let barIdleTimer = 0;

function showChatBar(): void {
  chatBar.classList.remove("hidden");
  chatInput.focus();
  armBarIdle();
}

function hideChatBar(): void {
  clearTimeout(barIdleTimer);
  chatBar.classList.add("hidden");
  chatInput.blur();
}

function toggleChatBar(): void {
  if (chatBar.classList.contains("hidden")) showChatBar();
  else hideChatBar();
}

/** Se range seule après 8 s d'inactivité (sauf micro actif ou texte en cours). */
function armBarIdle(): void {
  clearTimeout(barIdleTimer);
  barIdleTimer = window.setTimeout(() => {
    if (micRunning || thinking || chatInput.value.trim()) return armBarIdle();
    hideChatBar();
  }, 8000);
}
chatInput.addEventListener("input", armBarIdle);
chatInput.addEventListener("focus", armBarIdle);

// ── 0ter. Lecture vocale streaming (Web Audio, chunks PCM 24 kHz) ────
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
let nidSpeaking = false; // true pendant que la voix de Nid joue
let nidSpeakTimer = 0;
const liveSources = new Set<AudioBufferSourceNode>();
let userTurnText = ""; // transcription de ce que dit Julien (accumulée par tour)

/** Coupe net la voix en cours (interruption / Échap). */
function stopVoice(): void {
  for (const src of liveSources) {
    try { src.stop(); } catch { /* déjà terminé */ }
  }
  liveSources.clear();
  nextAudioTime = 0;
  setSpeaking(false);
}

/** Un seul endroit pilote "Nid parle" : bouche + garde anti-écho. */
function setSpeaking(on: boolean): void {
  nidSpeaking = on;
  robot.classList.toggle("talking", on);
}

void listen<string>("nid://voice-reset", () => (nextAudioTime = 0));
void listen<string>("nid://voice-audio", (e) => playPcmChunk(e.payload));
void listen<string>("nid://voice-interrupted", () => stopVoice());
void listen<string>("nid://voice-error", (e) => {
  stopVoice();
  say("🎙️ " + String(e.payload).slice(0, 110), 8000);
});
void listen<string>("nid://voice-disconnected", () => {
  // la session Live s'est fermée : on remet l'interface au propre,
  // elle se rouvrira toute seule au prochain message / à la prochaine parole
  stopVoice();
  if (micRunning) {
    void stopMicStream();
    say("La connexion vocale s'est coupée — relance le micro 🎙️", 5000);
  }
});
void listen<string>("nid://turn-complete", () => {
  userTurnText = "";
  // l'audio déjà planifié finit de jouer ; la garde se relâche avec lui
});
void listen<string>("nid://user-text", (e) => {
  userTurnText += e.payload;
  showBubble("🧑 " + userTurnText.trim(), { sticky: true });
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

// ── 1. Clignement des yeux ───────────────────────────────────────────
function scheduleBlink(): void {
  const delay = 1800 + Math.random() * 3200;
  setTimeout(() => {
    eyeLeft.classList.add("blink");
    eyeRight.classList.add("blink");
    setTimeout(() => {
      eyeLeft.classList.remove("blink");
      eyeRight.classList.remove("blink");
      scheduleBlink();
    }, 140);
  }, delay);
}
scheduleBlink();

// ── 2. Bulle de dialogue ─────────────────────────────────────────────
// Un SEUL minuteur : un nouveau message annule la disparition du précédent
// (avant, un ancien setTimeout pouvait masquer la bulle toute neuve).
let bubbleTimer = 0;

/** La bulle reste courte : les deux dernières phrases, 160 caractères max.
 *  Le texte complet est conservé dans le panneau d'administration. */
function shortText(text: string): string {
  const t = text.trim();
  if (t.length <= 160) return t;
  const sentences = t.match(/[^.!?…\n]+[.!?…]*\s*/g) ?? [t];
  let out = sentences.slice(-2).join("").trim();
  if (out.length > 160) out = "…" + out.slice(-157).trimStart();
  return out;
}

function showBubble(text: string, opts: { ttl?: number; sticky?: boolean; typing?: boolean } = {}): void {
  clearTimeout(bubbleTimer);
  bubble.classList.toggle("typing", !!opts.typing);
  bubble.textContent = shortText(text);
  bubble.classList.remove("hidden");
  bubble.scrollTop = bubble.scrollHeight; // suit le texte qui défile
  if (opts.ttl) hideBubbleLater(opts.ttl);
  else if (opts.sticky) hideBubbleLater(6000 + text.length * 40); // filet de sécurité
}

function hideBubbleLater(ms: number): void {
  clearTimeout(bubbleTimer);
  bubbleTimer = window.setTimeout(() => {
    // ne masque pas la bulle tant que Nid parle encore
    if (nidSpeaking || thinking) return hideBubbleLater(1200);
    bubble.classList.add("hidden");
    bubble.classList.remove("typing");
  }, ms);
}

function say(text: string, duration = 4000): void {
  showBubble(text, { ttl: duration });
  // petite animation de bouche pendant le message (hors voix réelle)
  robot.classList.add("talking");
  window.setTimeout(() => {
    if (!nidSpeaking) robot.classList.remove("talking");
  }, Math.min(duration, 2500));
}

/** L'état "réfléchit" : bulle à points animés + barre de chat verrouillée. */
function setThinking(on: boolean): void {
  thinking = on;
  chatBar.classList.toggle("busy", on);
  chatInput.disabled = on;
  if (on) {
    showBubble("", { typing: true });
  }
}

// Échap : coupe la parole et masque la bulle
window.addEventListener("keydown", (e) => {
  if (e.key === "Escape") {
    stopVoice();
    void invoke("stop_voice").catch(() => {});
    bubble.classList.add("hidden");
    closeAccessCard();
    if (!micRunning) hideChatBar();
  }
});

setTimeout(() => say("Salut Julien, je suis Nid ! 🪺"), 800);

// ── 4. Chat cérébral (streaming) ─────────────────────────────────────
let thinking = false;

async function sendMessage(): Promise<void> {
  const text = chatInput.value.trim();
  if (!text || thinking) return;
  chatInput.value = "";
  hideChatBar();
  await converse(text);
}

chatInput.addEventListener("keydown", (e) => {
  if (e.key === "Enter") void sendMessage();
});
document.getElementById("chat-send")!.addEventListener("click", (e) => {
  e.stopPropagation();
  void sendMessage();
});
chatBar.addEventListener("pointerdown", (e) => e.stopPropagation());

async function converse(text: string): Promise<void> {
  lastUserIntent = text;
  // L'ancienne « maison » vit désormais dans le panneau d'administration
  if (/(maison|ton home|ouvre ta maison|ferme ta maison)/.test(text.toLowerCase())) {
    say("Ma mémoire et mon activité sont dans le panneau d'administration.", 4500);
    return;
  }

  // 🗺️ Gestion des zones interdites (locales, persistées)
  const lowerCmd = text.toLowerCase();
  if (/(zone interdite|interdis cette zone|interdit ici|zone defensif|zone de défense)/.test(lowerCmd)) {
    const win = getCurrentWindow();
    const [pos, size] = await Promise.all([win.outerPosition(), win.outerSize()]);
    zones.push({ x: pos.x, y: pos.y, w: size.width, h: size.height, label: "interdit" });
    await invoke("set_zones", { zones });
    say("Compris, je ne mettrai plus mes pattes ici 🚫", 4500);
    return;
  }
  if (/(autorise cette zone|supprime la zone|permis ici|enlève la zone|enleve la zone)/.test(lowerCmd)) {
    const win = getCurrentWindow();
    const [pos, size] = await Promise.all([win.outerPosition(), win.outerSize()]);
    const before = zones.length;
    zones = zones.filter((z) => {
      // garde les zones qui ne chevauchent PAS la position actuelle
      return !(pos.x < z.x + z.w && pos.x + size.width > z.x && pos.y < z.y + z.h && pos.y + size.height > z.y);
    });
    await invoke("set_zones", { zones });
    say(before !== zones.length ? "D'accord, je peux revenir ici 😌" : "Il n'y a pas de zone interdite ici 🤔", 4500);
    return;
  }
  if (/(montre les zones|mes zones|combien de zones)/.test(lowerCmd)) {
    say(zones.length === 0 ? "Aucune zone interdite pour l'instant 🌍" : `J'ai ${zones.length} zone(s) interdite(s) en mémoire 🚫`, 4500);
    return;
  }

  // 👁️ Intention de vision : "regarde mon éditeur", "que vois-tu ?"...
  const lowerFirst = text.toLowerCase();
  const looks = /\b(regarde|regarder|que vois|qu'est[- ]ce que tu vois|décris|decrit)\b/.test(lowerFirst)
    && !/\b(ferme|couche|allonge)\b/.test(lowerFirst); // pas de conflit avec les actions
  if (looks) {
    setThinking(true);
    try {
      // cible : ce qui suit "regarde" (sinon fenêtre au premier plan)
      let target = "";
      const m = lowerFirst.match(/(?:regarde|regarder|décris|decrit)\s+(?:la fenêtre\s+|le programme\s+|l'application\s+)?(?:mon\s+|ma\s+|le\s+|la\s+|les\s+)?(.+)/);
      if (m && m[1]) {
        target = m[1].replace(/\b(écran|ecran|fenêtre|fenetre)\b/g, "").trim();
      }
      if (/\b(écran|ecran)\b/.test(lowerFirst) && !target) target = ""; // tout l'écran → premier plan

      showBubble("Je regarde... 👁️");
      const desc = await invoke<string>("look_at", {
        target,
        question: "Décris ce que tu vois sur cette capture d'écran.",
      });
      say(desc.slice(0, 200), Math.min(14000, 4000 + desc.length * 50));
      void invoke("speak", { text: desc.slice(0, 300) }).catch(() => {});
    } catch (err) {
      say(String(err).slice(0, 120), 7000);
    } finally {
      setThinking(false);
    }
    return;
  }

  // Commandes de déplacement : "va à gauche", "monte", "va au milieu"...
  if (await handleMoveCommand(text)) return;

  // Commandes de pose (locales, instantanées)
  const poseCmd = matchPoseCommand(text);
  if (poseCmd) {
    const [p, reply] = poseCmd;
    applyPose(p);
    say(reply, 4000);
    return;
  }

  stopVoice(); // une nouvelle question coupe la réponse précédente
  void emit("nid://chat-line", { role: "user", text }); // miroir dans le panneau
  setThinking(true);
  try {
    const onChunk = new Channel<string>();
    let acc = "";
    let fed = 0;
    // phrase complète = ponctuation forte suivie d'un espace/fin, ou saut de ligne
    const SENTENCE = /^[\s\S]*?(?:[.!?…]+(?=\s|$)|\n)/;
    onChunk.onmessage = (tok) => {
      acc += tok;
      showBubble(acc, { typing: false });
      if (voiceLiveMode) {
        for (;;) {
          const m = acc.slice(fed).match(SENTENCE);
          if (!m) break;
          fed += m[0].length;
          if (m[0].trim().length > 2) void invoke("voice_feed", { text: m[0] }).catch(() => {});
        }
      }
    };
    const reply = await invoke<string>("chat", { userMessage: text, onChunk });
    if (!reply.trim()) {
      say("Hmm, je n'ai rien trouvé à répondre 🤔", 4000);
      return;
    }
    void emit("nid://chat-line", { role: "nid", text: reply });
    showBubble(reply, { ttl: Math.min(14000, 3000 + reply.length * 60) });
    // gemini-live : on envoie le reste de la phrase puis on CLÔT le tour,
    // sinon le modèle n'ouvre jamais la bouche (voice_end déclenche la parole)
    if (voiceLiveMode) {
      const tail = acc.slice(fed).trim();
      if (tail) await invoke("voice_feed", { text: tail }).catch(() => {});
      void invoke("voice_end").catch((err) => say("🎙️ " + String(err).slice(0, 100), 6000));
    } else {
      invoke("speak", { text: reply }).catch(() => {});
    }
  } catch (err) {
    say(friendlyError(err), 7000);
  } finally {
    setThinking(false);
  }
}

/** Erreurs techniques → phrase courte et utile dans la bulle. */
function friendlyError(err: unknown): string {
  const msg = String(err);
  if (/injoignable|connexion|timeout/i.test(msg)) return "Je n'arrive pas à joindre mon cerveau 🧠 — vérifie la connexion ou le frigo 🍽️";
  if (/clé|api|401|403/i.test(msg)) return "Ma clé API ne passe pas 🔑 — regarde dans le frigo 🍽️";
  if (/429/.test(msg)) return "Trop de questions d'un coup 😵 — redonne-moi une seconde";
  return msg.slice(0, 140);
}

// ── 5. Voix : micro → whisper → cerveau → TTS ────────────────────────
// ── Micro STREAMING : ta voix part en continu vers Gemini Live ──────
let micStream: MediaStream | null = null;
let micProcessor: ScriptProcessorNode | null = null;
let micAudioCtx: AudioContext | null = null;
let micRunning = false;

micBtn.addEventListener("click", (e) => {
  e.stopPropagation();
  if (micRunning) void stopMicStream();
  else void startMicStream();
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

async function startMicStream(): Promise<void> {
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
      if (!micRunning) return;
      // si Nid parle, on coupe le micro (anti-écho de sa propre voix)
      if (nidSpeaking) return;
      const f32 = ev.inputBuffer.getChannelData(0);
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
    micRunning = true;
    micBtn.classList.remove("connecting");
    micBtn.classList.add("recording");
    micBtn.title = "Arrêter l'écoute";
    userTurnText = "";
    stopVoice(); // si Nid parlait, on lui coupe la parole : c'est ton tour
    showBubble("Je t'écoute... 🎙️");
    const errWatch = setInterval(() => {
      if (micErr) {
        say("🎙️ " + micErr, 8000);
        clearInterval(errWatch);
        void stopMicStream();
      }
      if (!micRunning) clearInterval(errWatch);
    }, 500);
  } catch (err) {
    console.error("micro:", err);
    micBtn.classList.remove("connecting");
    say("Je n'ai pas accès au micro 🙈 — autorise-le dans Windows (Confidentialité › Microphone)", 6000);
  }
}

async function stopMicStream(): Promise<void> {
  micRunning = false;
  micBtn.title = "Parler";
  micBtn.classList.remove("recording");
  micProcessor?.disconnect();
  micStream?.getTracks().forEach((t) => t.stop());
  micStream = null;
  void micAudioCtx?.close();
  micAudioCtx = null;
  await invoke("mic_end").catch(() => {}); // fin du flux → Gemini répond
}

// ── 6. Poses & autonomie (version œuf) ───────────────────────────────
type PoseName = "idle" | "walk" | "sit";
let pose: PoseName = "idle";
let poseUntil = 0;
interface Stroll {
  dir: number;
  start: number;
  end: number;
  targetX?: number; // cible explicite (commande "va à gauche"...)
  targetY?: number;
}
let stroll: Stroll | null = null;
let moveState = { x: 0, y: 0, scale: 1, maxX: 0 };

// ── Zones interdites (rectangles en pixels physiques) ────────────────
interface Zone { x: number; y: number; w: number; h: number; label: string; }
let zones: Zone[] = [];
void (async () => {
  try {
    zones = (await invoke("get_zones")) as Zone[];
  } catch { zones = []; }
})();

// Rafraîchit les zones toutes les 5 s (le panneau de contrôle peut les modifier)
setInterval(() => {
  void invoke("get_zones").then((z) => (zones = z as Zone[])).catch(() => {});
}, 5000);

function overlapsZone(px: number, py: number, pw: number, ph: number): Zone | null {
  for (const z of zones) {
    if (px < z.x + z.w && px + pw > z.x && py < z.y + z.h && py + ph > z.y) return z;
  }
  return null;
}

function applyPose(p: PoseName): void {
  pose = p;
  robot.classList.remove("pose-sit", "pose-glide");
  robot.classList.remove("talking");
  if (p !== "idle") robot.classList.add(p === "walk" ? "pose-glide" : `pose-${p}`);
}

async function startStroll(): Promise<void> {
  const win = getCurrentWindow();
  const [pos, mon, size] = await Promise.all([
    win.outerPosition(),
    currentMonitor(),
    win.outerSize(),
  ]);
  void overlapsZone(pos.x, pos.y, size.width, size.height);
  const scale = mon?.scaleFactor ?? 1;
  const monX = mon?.position?.x ?? 0;
  const monW = mon?.size?.width ?? 1920;
  moveState = { x: pos.x, y: pos.y, scale, maxX: monX + monW - size.width };
  if (moveState.maxX < monX) moveState.maxX = monX;
  let dir = Math.random() < 0.5 ? -1 : 1;
  if ((dir < 0 && pos.x <= monX + 10) || (dir > 0 && pos.x >= moveState.maxX - 10)) {
    dir = -dir;
  }
  stroll = { dir, start: performance.now(), end: performance.now() + 1500 + Math.random() * 1500 };
  applyPose("walk" as PoseName); // glide (l'œuf roule, il ne marche pas)
}

// Glisser vers une position cible (commandes vocales/texte)
async function glideTo(tx?: number, ty?: number): Promise<void> {
  const win = getCurrentWindow();
  const [pos, mon, size] = await Promise.all([
    win.outerPosition(),
    currentMonitor(),
    win.outerSize(),
  ]);
  const scale = mon?.scaleFactor ?? 1;
  const monX = mon?.position?.x ?? 0;
  const monY = mon?.position?.y ?? 0;
  const monW = mon?.size?.width ?? 1920;
  const monH = mon?.size?.height ?? 1080;
  const maxX = monX + monW - size.width - 10;
  const maxY = monY + monH - size.height - 10;
  moveState = { x: pos.x, y: pos.y, scale, maxX: Math.max(monX, maxX) };

  // 🚫 Zones interdites : si la cible en chevauche une, on cherche la
  // position autorisée la plus proche (glissement vertical puis horizontal)
  const px = tx ?? pos.x;
  const py = ty ?? pos.y;
  const pw = size.width;
  const ph = size.height;
  let fx = px;
  let fy = py;
  if (overlapsZone(fx, fy, pw, ph)) {
    let found = false;
    for (let d = 20; d <= 600 && !found; d += 20) {
      for (const [ox, oy] of [[0, -d], [0, d], [-d, 0], [d, 0], [-d, -d], [d, d], [-d, d], [d, -d]]) {
        if (!overlapsZone(px + ox, fy + oy, pw, ph)) {
          fx = px + ox;
          fy = fy + oy;
          found = true;
          break;
        }
      }
    }
    if (!found) {
      say("Cette zone m'est totalement interdite 🚫", 4000);
      return;
    }
    say("🚫 C'est ma zone interdite, je glisse juste à côté !", 4000);
  }

  stroll = {
    dir: 0,
    start: performance.now(),
    end: performance.now() + 6000, // garde-fou
    targetX: fx !== undefined ? Math.max(monX + 10, Math.min(maxX, fx)) : undefined,
    targetY: fy !== undefined ? Math.max(monY + 10, Math.min(maxY, fy)) : undefined,
  };
  applyPose("walk");
}

// Boucle d'autonomie : il vit sa vie quand personne ne l'occupe
setInterval(() => {
  if (dragging || thinking || nidSpeaking || micRunning || pose !== "idle" || !accessCard.classList.contains("hidden")) return;
  const r = Math.random();
  if (r < 0.3) {
    void startStroll();
  } else if (r < 0.45) {
    applyPose("sit");
    poseUntil = performance.now() + 6000 + Math.random() * 8000;
  }
  // sinon : il reste tranquille
}, 5000);

// Fin des poses temporisées + déplacement pendant la marche
setInterval(() => {
  if (pose === "sit" && performance.now() > poseUntil) {
    applyPose("idle");
  }
  if (pose === "walk" && stroll) {
    if (performance.now() >= stroll.end) {
      stroll = null;
      applyPose("idle");
    } else if (stroll.targetX !== undefined || stroll.targetY !== undefined) {
      // Mode cible : on glisse vers le point demandé
      const step = (cur: number, target: number) =>
        Math.sign(target - cur) * Math.min(Math.abs(target - cur), 340 * moveState.scale * 0.033);
      let arrived = true;
      if (stroll.targetX !== undefined && Math.abs(stroll.targetX - moveState.x) > 3) {
        moveState.x += step(moveState.x, stroll.targetX);
        arrived = false;
      }
      if (stroll.targetY !== undefined && Math.abs(stroll.targetY - moveState.y) > 3) {
        moveState.y += step(moveState.y, stroll.targetY);
        arrived = false;
      }
      if (arrived) {
        stroll = null;
        applyPose("idle");
      } else {
        moveWindowTo(moveState.x, moveState.y);
      }
    } else {
      // Balade libre
      const elapsed = (performance.now() - stroll.start) / 1000;
      const remain = (stroll.end - performance.now()) / 1000;
      const ease = Math.min(1, elapsed / 0.4) * Math.min(1, remain / 0.4);
      moveState.x += stroll.dir * 70 * Math.max(0.15, ease) * moveState.scale * 0.033;
      moveState.x = Math.max(0, Math.min(moveState.maxX, moveState.x));
      moveWindowTo(moveState.x, moveState.y);
    }
  }
}, 33);

// ── 7. Commandes de pose & émotions ──────────────────────────────────
const POSE_COMMANDS: Array<[RegExp, PoseName, string]> = [
  [/\b(assis|assois[- ]?toi|s'asseoir|s'assoire|installe[- ]toi)\b/, "sit", "Je m'installe confortablement 🪺"],
  [/\b(debout|leve|releve)\b/, "idle", "Me voilà debout ! 🧍"],
];

function matchPoseCommand(text: string): [PoseName, string] | null {
  const lower = text.toLowerCase();
  for (const [re, p, reply] of POSE_COMMANDS) {
    if (re.test(lower)) return [p, reply];
  }
  return null;
}

// ── Commandes de déplacement : "va à gauche", "monte", "va au milieu" ──
async function handleMoveCommand(text: string): Promise<boolean> {
  const lower = text.toLowerCase();
  if (!/(va|vas|vas-y|glisse|deplace[- ]toi|déplace[- ]toi|bouge|va vers)/.test(lower)) {
    return false;
  }
  // Extrait la géométrie actuelle
  const win = getCurrentWindow();
  const [mon, size] = await Promise.all([currentMonitor(), win.outerSize()]);
  const monX = mon?.position?.x ?? 0;
  const monY = mon?.position?.y ?? 0;
  const monW = mon?.size?.width ?? 1920;
  const monH = mon?.size?.height ?? 1080;
  const maxX = monX + monW - size.width - 10;
  const maxY = monY + monH - size.height - 10;

  let tx: number | undefined;
  let ty: number | undefined;

  if (/(gauche|left)/.test(lower)) tx = monX + 10;
  else if (/(droite|right)/.test(lower)) tx = maxX;
  else if (/(milieu|centre|center)/.test(lower)) tx = monX + (monW - size.width) / 2;

  if (/(haut|monte|monter|up)/.test(lower)) ty = monY + 10;
  else if (/(bas|descend|descendre|down)/.test(lower)) ty = maxY;

  if (tx === undefined && ty === undefined) return false;

  await glideTo(tx, ty);
  say("J'y glisse ! 🥚", 2500);
  return true;
}


// ── Demandes d'accès : un badge sur l'œuf, une carte à la demande ────
interface AccessReq { app: string; process: string; title: string; need: "look" | "act"; intent: string }
const accessQueue: AccessReq[] = [];
let lastUserIntent = ""; // la phrase qui a déclenché la demande, rejouée après accord

void listen<Omit<AccessReq, "intent">>("nid://access-request", (e) => {
  const dup = accessQueue.some((r) => r.process === e.payload.process && r.need === e.payload.need);
  if (!dup) accessQueue.push({ ...e.payload, intent: lastUserIntent });
  refreshAccessBadge();
});

function refreshAccessBadge(): void {
  const n = accessQueue.length;
  accessBadge.classList.toggle("hidden", n === 0);
  document.getElementById("access-count")!.textContent = String(n);
  if (n === 0) closeAccessCard();
}

function esc(s: string): string {
  return s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
}

function openAccessCard(): void {
  const req = accessQueue[0];
  if (!req) return;
  const verb = req.need === "act" ? "agir sur" : "regarder";
  document.getElementById("access-text")!.innerHTML =
    `<b>${esc(req.app)}</b> · ${esc(req.title).slice(0, 60)}<br><span>Nid demande à ${verb} cette application.</span>` +
    (accessQueue.length > 1 ? `<em>${accessQueue.length - 1} autre(s) en attente</em>` : "");
  document.getElementById("access-allow")!.textContent =
    req.need === "act" ? "Autoriser les actions" : "Autoriser la lecture";
  accessCard.classList.remove("hidden");
}

function closeAccessCard(): void {
  accessCard.classList.add("hidden");
}

async function resolveAccess(level: "look" | "act" | "sanctuary" | "none"): Promise<void> {
  const req = accessQueue.shift();
  if (!req) return;
  // la règle porte sur l'application entière (processus::*)
  if (level !== "none") {
    await invoke("set_permission", { key: `${req.process}::*`, level }).catch(() => {});
  }
  refreshAccessBadge();
  if (accessQueue.length) openAccessCard();
  if ((level === "look" || level === "act") && req.intent) {
    // accord donné -> on rejoue la demande d'origine sans la faire répéter
    void converse(req.intent);
  }
}

accessBadge.addEventListener("pointerdown", (e) => e.stopPropagation());
accessBadge.addEventListener("click", (e) => {
  e.stopPropagation();
  if (accessCard.classList.contains("hidden")) openAccessCard();
  else closeAccessCard();
});
accessCard.addEventListener("pointerdown", (e) => e.stopPropagation());
document.getElementById("access-allow")!.addEventListener("click", () => {
  void resolveAccess(accessQueue[0]?.need ?? "look");
});
document.getElementById("access-deny")!.addEventListener("click", () => void resolveAccess("none"));
document.getElementById("access-sanctuary")!.addEventListener("click", () => void resolveAccess("sanctuary"));

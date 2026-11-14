import { getCurrentWindow, currentMonitor, PhysicalPosition } from "@tauri-apps/api/window";
import { invoke, Channel } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

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
});

window.addEventListener("pointermove", (e) => {
  if (!dragging) return;
  const dx = Math.round((e.screenX - dragStart.sx) * dragStart.scale);
  const dy = Math.round((e.screenY - dragStart.sy) * dragStart.scale);
  getCurrentWindow().setPosition(
    new PhysicalPosition(dragStart.wx + dx, dragStart.wy + dy)
  );
});

window.addEventListener("pointerup", () => {
  if (!dragging) return;
  dragging = false;
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
    const overUi = [robot, bubble, chatBar].some((el) => {
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

// Toggle de la barre de texte depuis le panneau de contrôle
void listen<{ visible: boolean }>("nid://chatbar", (e) => {
  chatBar.style.display = e.payload.visible ? "" : "none";
});

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
let voiceRequestId = 0;
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
  nextAudioTime += buf.duration;
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
function say(text: string, duration = 4000): void {
  bubble.textContent = text;
  bubble.classList.remove("hidden");
  robot.classList.add("talking");
  setTimeout(() => {
    bubble.classList.add("hidden");
    robot.classList.remove("talking");
  }, duration);
}

setTimeout(() => say("Salut Julien, je suis Nid ! 🪺"), 800);

// ── 4. Chat cérébral (streaming) ─────────────────────────────────────
let thinking = false;

async function sendMessage(): Promise<void> {
  const text = chatInput.value.trim();
  if (!text || thinking) return;
  chatInput.value = "";
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
  // 🏠 Ouvrir/fermer la maison
  if (/(maison|ton home|ouvre ta maison|ferme ta maison)/.test(text.toLowerCase())) {
    toggleHouse();
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
    thinking = true;
    try {
      // cible : ce qui suit "regarde" (sinon fenêtre au premier plan)
      let target = "";
      const m = lowerFirst.match(/(?:regarde|regarder|décris|decrit)\s+(?:la fenêtre\s+|le programme\s+|l'application\s+)?(?:mon\s+|ma\s+|le\s+|la\s+|les\s+)?(.+)/);
      if (m && m[1]) {
        target = m[1].replace(/\b(écran|ecran|fenêtre|fenetre)\b/g, "").trim();
      }
      if (/\b(écran|ecran)\b/.test(lowerFirst) && !target) target = ""; // tout l'écran → premier plan

      bubble.textContent = "Je regarde... 👁️";
      bubble.classList.remove("hidden");
      const desc = await invoke<string>("look_at", {
        target,
        question: "Décris ce que tu vois sur cette capture d'écran.",
      });
      say(desc.slice(0, 200), Math.min(14000, 4000 + desc.length * 50));
      void invoke("speak", { text: desc.slice(0, 300) }).catch(() => {});
    } catch (err) {
      say(String(err).slice(0, 120), 7000);
    } finally {
      thinking = false;
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

  bubble.textContent = "…";
  bubble.classList.remove("hidden");
  thinking = true;
  try {
    const onChunk = new Channel<string>();
    let acc = "";
    onChunk.onmessage = (tok) => {
      acc += tok;
      bubble.textContent = acc;
    };
    const reply = await invoke<string>("chat", { userMessage: text, onChunk });
    setTimeout(
      () => bubble.classList.add("hidden"),
      Math.min(12000, 2500 + reply.length * 60)
    );
    const voiceChan = new Channel<string>();
    const myId = ++voiceRequestId;
    voiceChan.onmessage = (b64) => {
      if (voiceRequestId !== myId) return; // chunks d'un ancien flux → ignorés
      if (b64 === "__RESET__") nextAudioTime = 0;
      else playPcmChunk(b64);
    };
    invoke("speak", { text: reply, onChunk: voiceChan }).catch((err) => {
      if (voiceRequestId === myId) {
        say("🎙️ Erreur voix : " + String(err).slice(0, 110), 8000);
      }
    });
  } catch (err) {
    say(String(err).slice(0, 90), 6000);
  } finally {
    thinking = false;
  }
}

// ── 5. Voix : micro → whisper → cerveau → TTS ────────────────────────
let mediaStream: MediaStream | null = null;
let recorder: MediaRecorder | null = null;
let audioChunks: Blob[] = [];
let recording = false;

micBtn.addEventListener("click", (e) => {
  e.stopPropagation();
  if (recording) void stopRecording();
  else void startRecording();
});

async function startRecording(): Promise<void> {
  try {
    mediaStream = await navigator.mediaDevices.getUserMedia({ audio: true });
    audioChunks = [];
    recorder = new MediaRecorder(mediaStream);
    recorder.ondataavailable = (ev) => audioChunks.push(ev.data);
    recorder.onstop = () => void processAudio();
    recorder.start();
    recording = true;
    micBtn.classList.add("recording");
    bubble.textContent = "Je t'écoute... 🎙️";
    bubble.classList.remove("hidden");
  } catch (err) {
    console.error("micro:", err);
    say("Je n'ai pas accès au micro 🙈", 5000);
  }
}

async function stopRecording(): Promise<void> {
  recording = false;
  micBtn.classList.remove("recording");
  recorder?.stop();
  mediaStream?.getTracks().forEach((t) => t.stop());
}

async function processAudio(): Promise<void> {
  try {
    bubble.textContent = "Je réfléchis...";
    const blob = new Blob(audioChunks);
    const audioCtx = new AudioContext();
    const buf = await audioCtx.decodeAudioData(await blob.arrayBuffer());

    const src = buf.getChannelData(0);
    const ratio = buf.sampleRate / 16000;
    const outLen = Math.floor(src.length / ratio);
    const i16 = new Int16Array(outLen);
    for (let i = 0; i < outLen; i++) {
      const s = src[Math.floor(i * ratio)] * 32767;
      i16[i] = Math.max(-32768, Math.min(32767, Math.round(s)));
    }
    audioCtx.close();

    const text = await invoke<string>("transcribe", { samples: Array.from(i16) });
    if (!text) {
      say("Je n'ai rien entendu 🤔", 4000);
      return;
    }
    await converse(text);
  } catch (err) {
    console.error("processAudio:", err);
    say("Oups, souci d'oreille 🙉", 4000);
  }
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
  if (dragging || thinking || recording || pose !== "idle") return;
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

// ── 8. La maison 🏠 : journal, album, porte, frigo ───────────────────
const house = document.getElementById("house")!;
const houseContent = document.getElementById("house-content")!;
let houseTab = "journal";

house.addEventListener("pointerdown", (e) => e.stopPropagation());

function toggleHouse(): void {
  house.classList.toggle("hidden");
  house.classList.toggle("hidden2");
  if (!house.classList.contains("hidden")) {
    renderHouse();
  }
}

document.getElementById("house-btn")!.addEventListener("click", (e) => {
  e.stopPropagation();
  toggleHouse();
});
document.getElementById("house-close")!.addEventListener("click", () => {
  house.classList.add("hidden");
  house.classList.add("hidden2");
});
house.querySelectorAll("#house-tabs button[data-tab]").forEach((btn) => {
  btn.addEventListener("click", (ev) => {
    ev.stopPropagation();
    houseTab = (ev.currentTarget as HTMLElement).dataset.tab!;
    house.querySelectorAll("#house-tabs button[data-tab]").forEach((b) => b.classList.remove("active"));
    (ev.currentTarget as HTMLElement).classList.add("active");
    renderHouse();
  });
});

function fmtTs(ts: number): string {
  const d = new Date(ts * 1000);
  return d.toLocaleDateString("fr") + " " + d.toLocaleTimeString("fr", { hour: "2-digit", minute: "2-digit" });
}

function renderHouse(): void {
  void (async () => {
    if (houseTab === "journal") {
      const events = (await invoke("get_journal", { limit: 50 })) as Array<{ ts: number; kind: string; detail: string }>;
      const icons: Record<string, string> = { action: "🖐️", vision: "👁️", door: "🚪" };
      houseContent.innerHTML = events.length
        ? events
            .map((e) => `<div class="house-item">${icons[e.kind] ?? "•"} <b>${e.kind}</b> ${e.detail}<span class="ts">${fmtTs(e.ts)}</span></div>`)
            .join("")
        : `<div class="house-empty">Le journal est vide — Nid n'a encore rien fait 🌙</div>`;
    } else if (houseTab === "album") {
      const memories = (await invoke("get_memories")) as Array<{ key: string; note: string; ts: number }>;
      const facts = (await invoke("get_facts")) as Array<{ subject: string; content: string; ts: number }>;
      houseContent.innerHTML =
        `<div class="section-title">🎯 Faits durables (consolidation)</div>` +
        (facts.length
          ? facts
              .map(
                (f) =>
                  `<div class="house-item"><b>${f.subject}</b> — ${f.content.slice(0, 140)}<span class="ts">${fmtTs(f.ts)}</span><button class="house-delete" data-ts="${f.ts}">🗑️</button></div>`
              )
              .join("")
          : `<div class="house-empty">Aucun fait consolidé encore — ça vient après quelques conversations</div>`) +
        `<div class="section-title" style="margin-top:10px">🖼️ Souvenirs de lieux (vision)</div>` +
        (memories.length
        ? memories
            .map(
              (m) =>
                `<div class="house-item"><b>${m.key.split("::")[0]}</b> — ${m.note.slice(0, 140)}<span class="ts">${fmtTs(m.ts)}</span><button class="house-delete" data-key="${m.key}">🗑️</button></div>`
            )
            .join("")
        : `<div class="house-empty">L'album est vide — demandez-lui de "regarder" une fenêtre autorisée 👁️</div>`);
      houseContent.querySelectorAll(".house-delete").forEach((btn) => {
        btn.addEventListener("click", async (ev) => {
          ev.stopPropagation();
          const el = ev.currentTarget as HTMLElement;
          if (el.dataset.key) {
            await invoke("clear_memory", { key: el.dataset.key });
          } else if (el.dataset.ts) {
            await invoke("forget_fact", { ts: Number(el.dataset.ts) });
          }
          renderHouse();
        });
      });
    } else if (houseTab === "door") {
      const perms = (await invoke("get_permissions")) as Record<string, string>;
      const entries = Object.entries(perms);
      houseContent.innerHTML = entries.length
        ? entries
            .map(
              ([key, level]) =>
                `<div class="house-item"><b>${key.split("::")[0]}</b> — ${key.split("::")[1]?.slice(0, 40)}<br>` +
                `<select data-key="${key}">` +
                ["sanctuary", "none", "look", "act", "trust"]
                  .map((l) => `<option value="${l}"${l === level ? " selected" : ""}>${l}</option>`)
                  .join("") +
                `</select></div>`
            )
            .join("")
        : `<div class="house-empty">La porte est vide — les permissions arrivent quand les fenêtres frappent 🚪</div>`;
      houseContent.querySelectorAll("select").forEach((sel) => {
        sel.addEventListener("change", async (ev) => {
          await invoke("set_permission", {
            key: (ev.currentTarget as HTMLSelectElement).dataset.key,
            level: (ev.currentTarget as HTMLSelectElement).value,
          });
        });
      });
    } else if (houseTab === "fridge") {
      const cfg = (await invoke("get_brain_config")) as { provider: string; model: string; url: string; vision_model: string; audio_model: string; api_key: string };
      houseContent.innerHTML =
        `<div class="house-form">` +
        `<label>Provider <input id="f-provider" value="${cfg.provider}"></label>` +
        `<label>Modèle <input id="f-model" value="${cfg.model}"></label>` +
        `<label>URL (ollama) <input id="f-url" value="${cfg.url}"></label>` +
        `<label>Vision <input id="f-vision" value="${cfg.vision_model}"></label>` +
        `<label>Audio <input id="f-audio" value="${cfg.audio_model ?? ""}" placeholder="gemini-tts:Kore"></label>` +
        `<label>Clé Gemini <input id="f-key" type="password" value="${cfg.api_key ?? ""}" placeholder="AIza..."></label>` +
        `<button id="f-save">🍽️ Ranger dans le frigo</button>` +
        `</div>`;
      document.getElementById("f-save")!.addEventListener("click", async (ev) => {
        ev.stopPropagation();
        await invoke("set_brain_config", {
          config: {
            provider: (document.getElementById("f-provider") as HTMLInputElement).value,
            model: (document.getElementById("f-model") as HTMLInputElement).value,
            url: (document.getElementById("f-url") as HTMLInputElement).value,
            vision_model: (document.getElementById("f-vision") as HTMLInputElement).value,
            audio_model: (document.getElementById("f-audio") as HTMLInputElement).value,
            api_key: (document.getElementById("f-key") as HTMLInputElement).value,
          },
        });
        say("C'est rangé dans le frigo ! 🍽️", 3000);
      });
    }
  })();
}

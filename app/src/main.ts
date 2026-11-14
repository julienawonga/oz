import { getCurrentWindow, currentMonitor, PhysicalPosition } from "@tauri-apps/api/window";
import { invoke, Channel } from "@tauri-apps/api/core";

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

window.addEventListener("pointerup", () => (dragging = false));

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

// ── 3. Frappe à la porte (permissions) ───────────────────────────────
interface AppWindow {
  title: string;
  process: string;
}
type Level = "sanctuary" | "none" | "look" | "act" | "trust";

const asked = new Set<string>();
let knockOpen = false;
const key = (w: AppWindow) => `${w.process}::${w.title}`;

function doorUi(w: AppWindow): void {
  knockOpen = true;
  bubble.innerHTML =
    `🚪 Je vois une nouvelle fenêtre :<br><b>${w.title}</b><br>` +
    `<div id="knock-btns">` +
    `<button data-lvl="look">👀 Regarder</button>` +
    `<button data-lvl="none">🚫 Pas maintenant</button>` +
    `<button data-lvl="sanctuary">🔒 Sanctuaire</button>` +
    `</div>`;
  bubble.classList.remove("hidden");

  bubble.querySelectorAll("#knock-btns button").forEach((btn) => {
    btn.addEventListener("click", async (ev) => {
      ev.stopPropagation();
      const lvl = (ev.currentTarget as HTMLElement).dataset.lvl as Level;
      await invoke("set_permission", { key: key(w), level: lvl });
      bubble.classList.add("hidden");
      knockOpen = false;
      const msg: Record<Level, string> = {
        look: "Merci, je garderai un œil ! 👀",
        none: "Pas de souci, je n'y touche pas 👋",
        sanctuary: "Compris, je ne regarde jamais ! 🔒",
        act: "D'accord !",
        trust: "Merci de ta confiance 🤝",
      };
      say(msg[lvl]);
    });
  });
}

async function watchWindows(): Promise<void> {
  if (knockOpen) return;
  try {
    const windows = (await invoke("list_windows")) as AppWindow[];
    for (const w of windows) {
      if (asked.has(key(w))) continue;
      asked.add(key(w));
      const perms = (await invoke("get_permissions")) as Record<string, string>;
      if (!perms[key(w)]) {
        doorUi(w);
        break;
      }
    }
  } catch (err) {
    console.error("watchWindows:", err);
  }
}
setTimeout(() => setInterval(watchWindows, 4000), 6000);

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
    void invoke("speak", { text: reply }).catch(() => {});
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
type PoseName = "idle" | "walk" | "sit" | "lie" | "jump";
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

function applyPose(p: PoseName): void {
  pose = p;
  robot.classList.remove("pose-sit", "pose-lie", "pose-jump", "pose-glide");
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
  stroll = {
    dir: 0,
    start: performance.now(),
    end: performance.now() + 6000, // garde-fou
    targetX: tx !== undefined ? Math.max(monX + 10, Math.min(maxX, tx)) : undefined,
    targetY: ty !== undefined ? Math.max(monY + 10, Math.min(maxY, ty)) : undefined,
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
  } else if (r < 0.55) {
    applyPose("lie");
    poseUntil = performance.now() + 5000 + Math.random() * 7000;
  } else if (r < 0.65) {
    applyPose("jump");
    setTimeout(() => applyPose("idle"), 500);
  }
  // sinon : il reste tranquille
}, 5000);

// Fin des poses temporisées + déplacement pendant la marche
setInterval(() => {
  if ((pose === "sit" || pose === "lie") && performance.now() > poseUntil) {
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
  [/\b(saute|sauter|hop)\b/, "jump", "Hop ! 🐇"],
  [/\b(allonge|allonger|couche|coucher|sieste|dors)\b/, "lie", "Ahhh, je me repose 😴"],
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

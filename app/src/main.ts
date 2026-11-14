import * as THREE from "three";
import { GLTFLoader } from "three/addons/loaders/GLTFLoader.js";
import { VRMLoaderPlugin, VRMUtils, type VRM } from "@pixiv/three-vrm";
import { getCurrentWindow, currentMonitor, PhysicalPosition } from "@tauri-apps/api/window";
import { invoke } from "@tauri-apps/api/core";

// ── Mini-Julien · Étapes 2 + 3 ──────────────────────────────────────
// Avatar VRM 3D + broker de permissions (frappe à la porte).

const stage = document.getElementById("stage")!;
const bubble = document.getElementById("speech-bubble")!;
const canvas = document.getElementById("scene") as HTMLCanvasElement;
const chatBar = document.getElementById("chat-bar")!;
const chatInput = document.getElementById("chat-input") as HTMLInputElement;

// ── 0. Drag manuel DPI-aware (validé à l'étape 1) ────────────────────
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

// ── 1. Scène Three.js transparente ───────────────────────────────────
const renderer = new THREE.WebGLRenderer({
  canvas,
  alpha: true,
  antialias: true,
});
renderer.setSize(window.innerWidth, window.innerHeight);
renderer.setPixelRatio(window.devicePixelRatio);
renderer.outputColorSpace = THREE.SRGBColorSpace;

const scene = new THREE.Scene();

const camera = new THREE.PerspectiveCamera(
  35,
  window.innerWidth / window.innerHeight,
  0.1,
  20
);
camera.position.set(0, 0.85, 3.1); // recul calculé pour voir TOUT le corps
camera.lookAt(0, 0.85, 0);

const light = new THREE.DirectionalLight(0xffffff, Math.PI);
light.position.set(1, 1.4, 0.8);
scene.add(light);
scene.add(new THREE.AmbientLight(0xffffff, 0.55));

// Cible invisible que l'avatar regarde (on ne bouge JAMAIS la caméra)
const lookTarget = new THREE.Object3D();
lookTarget.position.set(0, 1.3, 2.0);
scene.add(lookTarget);

// ── 2bis. Références pour l'animation des bras ──────────────────────
let leftArmNode: THREE.Object3D | null = null;
let rightArmNode: THREE.Object3D | null = null;
let leftForeArmNode: THREE.Object3D | null = null;
let rightForeArmNode: THREE.Object3D | null = null;
let leftHandNode: THREE.Object3D | null = null;
let rightHandNode: THREE.Object3D | null = null;
const ARM_DOWN = 1.15; // rotation qui ramène le bras le long du corps

// ── 2. Chargement du modèle VRM ──────────────────────────────────────
let vrm: VRM | null = null;

const loader = new GLTFLoader();
loader.register((parser) => new VRMLoaderPlugin(parser));

loader.load(
  "/models/robot.vrm",
  (gltf) => {
    vrm = gltf.userData.vrm as VRM;
    // Orientation : les VRM regardent vers -Z, on aligne vers la caméra (+Z)
    VRMUtils.rotateVRM0(vrm);
    scene.add(vrm.scene);

    // Pose naturelle : bras le long du corps (au lieu du T-pose)
    const norm = vrm.humanoid;
    leftArmNode = norm?.getNormalizedBoneNode("leftUpperArm") ?? null;
    rightArmNode = norm?.getNormalizedBoneNode("rightUpperArm") ?? null;
    leftForeArmNode = norm?.getNormalizedBoneNode("leftLowerArm") ?? null;
    rightForeArmNode = norm?.getNormalizedBoneNode("rightLowerArm") ?? null;
    leftHandNode = norm?.getNormalizedBoneNode("leftHand") ?? null;
    rightHandNode = norm?.getNormalizedBoneNode("rightHand") ?? null;

    // Le regard suit la cible invisible pilotée par le curseur
    if (vrm.lookAt) vrm.lookAt.target = lookTarget;
  },
  undefined,
  (err) => console.error("Erreur de chargement VRM:", err)
);

// ── 3. Suivi du curseur (cible de regard) + clignement + respiration ──
window.addEventListener("pointermove", (e) => {
  if (!vrm || dragging) return;
  const nx = (e.clientX / window.innerWidth) * 2 - 1;
  const ny = -((e.clientY / window.innerHeight) * 2 - 1);
  lookTarget.position.x = nx * 1.5;
  lookTarget.position.y = 1.3 + ny * 0.6;
});

let blinkTimer = 0;
let nextBlink = 2;

const clock = new THREE.Clock();

function animate() {
  requestAnimationFrame(animate);
  const delta = clock.getDelta();
  const t = clock.elapsedTime;

  if (vrm) {
    // Respiration : léger balancement du buste
    const bone = vrm.humanoid?.getRawBoneNode("hips");
    if (bone) {
      bone.position.y = (bone.userData.baseY ??= bone.position.y) + Math.sin(t * 1.6) * 0.006;
      bone.rotation.z = Math.sin(t * 0.8) * 0.02;
    }

    // Bras vivants : balancement doux en opposition de phase
    if (leftArmNode && rightArmNode) {
      leftArmNode.rotation.z = -ARM_DOWN + Math.sin(t * 1.6) * 0.05;
      rightArmNode.rotation.z = ARM_DOWN + Math.sin(t * 1.6 + Math.PI) * 0.05;
      leftArmNode.rotation.x = Math.sin(t * 0.7) * 0.03;
      rightArmNode.rotation.x = Math.sin(t * 0.7 + Math.PI) * 0.03;
    }

    // Avant-bras : léger flottement en retard de phase (effet chaine)
    if (leftForeArmNode && rightForeArmNode) {
      leftForeArmNode.rotation.x = 0.25 + Math.sin(t * 1.6 + 0.6) * 0.04;
      rightForeArmNode.rotation.x = 0.25 + Math.sin(t * 1.6 + Math.PI + 0.6) * 0.04;
      leftForeArmNode.rotation.y = Math.sin(t * 0.9) * 0.03;
      rightForeArmNode.rotation.y = Math.sin(t * 0.9 + Math.PI) * 0.03;
    }

    // Mains : elles suivent la chaine avec leur propre inertie
    if (leftHandNode && rightHandNode) {
      leftHandNode.rotation.z = Math.sin(t * 1.6 + 1.2) * 0.06;
      rightHandNode.rotation.z = Math.sin(t * 1.6 + Math.PI + 1.2) * 0.06;
      leftHandNode.rotation.x = Math.sin(t * 1.1) * 0.05;
      rightHandNode.rotation.x = Math.sin(t * 1.1 + Math.PI) * 0.05;
    }

    // Clignement des yeux
    blinkTimer += delta;
    const expr = vrm.expressionManager;
    if (expr) {
      let blinkValue = 0;
      if (blinkTimer > nextBlink) {
        // cycle : fermeture rapide puis réouverture
        const phase = (blinkTimer - nextBlink) / 0.25;
        blinkValue = phase < 0.5 ? phase * 2 : Math.max(0, 2 - phase * 2);
        if (phase >= 1) {
          blinkTimer = 0;
          nextBlink = 2 + Math.random() * 3;
        }
      }
      expr.setValue("blink", blinkValue);

      // Bouche qui parle si la bulle est visible
      const talking = !bubble.classList.contains("hidden");
      expr.setValue(
        "aa",
        talking ? 0.3 + Math.abs(Math.sin(t * 12)) * 0.4 : 0
      );
    }

    vrm.update(delta);
  }

  renderer.render(scene, camera);
}
animate();

// Redimensionnement
window.addEventListener("resize", () => {
  renderer.setSize(window.innerWidth, window.innerHeight);
  camera.aspect = window.innerWidth / window.innerHeight;
  camera.updateProjectionMatrix();
});

// ── 4. Bulle de dialogue ─────────────────────────────────────────────
function say(text: string, duration = 4000) {
  bubble.textContent = text;
  bubble.classList.remove("hidden");
  setTimeout(() => bubble.classList.add("hidden"), duration);
}

setTimeout(() => say("Salut Julien, je suis Nid ! 🪺"), 1200);

// ── 4ter. Chat cérébral → Provider Rust → Ollama ────────────────────
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
// Empêche le drag quand on clique dans la barre
chatBar.addEventListener("pointerdown", (e) => e.stopPropagation());

// ── 4quater. Voix : micro → whisper (Rust) → cerveau → TTS ──────────
const micBtn = document.getElementById("chat-mic")!;
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

    // Mono + rééchantillonnage 16 kHz (format attendu par whisper)
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

// Boucle de conversation commune : texte → LLM → bulle → TTS
async function converse(text: string): Promise<void> {
  bubble.textContent = text;
  bubble.classList.remove("hidden");
  thinking = true;
  try {
    const reply = await invoke<string>("chat", { userMessage: text });
    say(reply, Math.min(12000, 2500 + reply.length * 60));
    void invoke("speak", { text: reply }).catch(() => {});
  } catch (err) {
    say(String(err).slice(0, 90), 6000);
  } finally {
    thinking = false;
  }
}

// ── 4bis. Click-through pixel-perfect ──────────────────────────
// La fenêtre est un rectangle, mais la zone cliquable doit épouser
// la SILHOUETTE du robot. On teste la position globale du curseur
// (fournie par Rust, car une fenêtre traversante ne voit plus la souris)
// et on raycast dans la scène 3D : touché → fenêtre active,
// dans le vide → la fenêtre laisse passer les clics.

const raycaster = new THREE.Raycaster();
let cursorOverRobot = true; // on démarre actif pour attraper le robot

async function hitTestLoop(): Promise<void> {
  if (!vrm || dragging) return;
  try {
    const win = getCurrentWindow();
    const [cursor, outer, mon] = await Promise.all([
      invoke<{ x: number; y: number }>("get_cursor_pos"),
      win.outerPosition(),
      currentMonitor(),
    ]);
    const scale = mon?.scaleFactor ?? 1;
    // Pixels physiques écran → pixels CSS locaux à la fenêtre
    const lx = (cursor.x - outer.x) / scale;
    const ly = (cursor.y - outer.y) / scale;

    // Raycast à travers la silhouette du modèle
    const nx = (lx / window.innerWidth) * 2 - 1;
    const ny = -(ly / window.innerHeight) * 2 + 1;
    raycaster.setFromCamera(new THREE.Vector2(nx, ny), camera);
    const overModel = raycaster.intersectObject(vrm.scene, true).length > 0;

    // La bulle ou la barre de chat compte comme partie du robot
    const overUi = [bubble, chatBar].some((el) => {
      if (el.classList.contains("hidden")) return false;
      const r = el.getBoundingClientRect();
      return lx >= r.left && lx <= r.right && ly >= r.top && ly <= r.bottom;
    });

    const over = overModel || overUi;
    if (over !== cursorOverRobot) {
      cursorOverRobot = over;
      await win.setIgnoreCursorEvents(!over);
    }

    // Bonus : le regard suit le curseur GLOBAL (même hors de la fenêtre)
    const gnx = (lx / window.innerWidth) * 2 - 1;
    const gny = -(ly / window.innerHeight) * 2 + 1;
    lookTarget.position.x = THREE.MathUtils.clamp(gnx, -1, 1) * 1.5;
    lookTarget.position.y = 1.3 + THREE.MathUtils.clamp(gny, -1, 1) * 0.6;
  } catch {
    // fenêtre en cours de déplacement, on réessaie au prochain tick
  }
}
setInterval(hitTestLoop, 80);

// ── 5. Broker de permissions : frappe à la porte ────────────────────
// Toutes les 4 s : on liste les fenêtres visibles. Nouvelle fenêtre
// sans permission → le robot frappe et demande quoi faire.

interface AppWindow {
  title: string;
  process: string;
}

type Level = "sanctuary" | "none" | "look" | "act" | "trust";

const asked = new Set<string>(); // fenêtres déjà rencontrées cette session
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
      ev.stopPropagation(); // ne pas déclencher le drag
      const lvl = (ev.currentTarget as HTMLElement).dataset.lvl as Level;
      await invoke("set_permission", { key: key(w), level: lvl });
      bubble.classList.add("hidden");
      knockOpen = false;
      const msg: Record<Level, string> = {
        look: "Merci, je garderai un œil ! 👀",
        none: "Pas de souci, je n'y touche pas 👋",
        sanctuary: "Compris, ça restera entre nous... euh, je ne regarde jamais ! 🔒",
        act: "D'accord !",
        trust: "Merci de ta confiance 🤝",
      };
      say(msg[lvl]);
    });
  });
}

async function watchWindows(): Promise<void> {
  if (knockOpen) return; // une porte à la fois
  try {
    const windows = (await invoke("list_windows")) as AppWindow[];
    for (const w of windows) {
      if (asked.has(key(w))) continue;
      asked.add(key(w));
      const perms = (await invoke("get_permissions")) as Record<string, string>;
      if (!perms[key(w)]) {
        // Fenêtre jamais rencontrée → il frappe à la porte
        doorUi(w);
        break; // une demande à la fois
      }
    }
  } catch (err) {
    console.error("watchWindows:", err);
  }
}

setTimeout(() => setInterval(watchWindows, 4000), 6000);

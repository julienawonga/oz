import * as THREE from "three";
import { GLTFLoader } from "three/addons/loaders/GLTFLoader.js";
import { VRMLoaderPlugin, VRMUtils, type VRM } from "@pixiv/three-vrm";
import { getCurrentWindow, currentMonitor, PhysicalPosition } from "@tauri-apps/api/window";
import { invoke, Channel } from "@tauri-apps/api/core";

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

const light = new THREE.DirectionalLight(0xfff5e8, Math.PI * 0.7); // key chaude
light.position.set(1.2, 1.6, 1.0);
scene.add(light);
const rim = new THREE.DirectionalLight(0x9ec9ff, Math.PI * 0.35); // rim froide (dos-gauche)
rim.position.set(-1.2, 1.2, -1.2);
scene.add(rim);
scene.add(new THREE.AmbientLight(0xffffff, 0.35));

// Ombre portée au sol (blob) — l'indice 3D le plus fort :
// elle rétrécit et pâlit quand il est en l'air
const shadow = new THREE.Mesh(
  new THREE.CircleGeometry(0.28, 32),
  new THREE.MeshBasicMaterial({
    color: 0x000000,
    transparent: true,
    opacity: 0.28,
    depthWrite: false,
  })
);
shadow.rotation.x = -Math.PI / 2;
shadow.position.y = 0.01;
scene.add(shadow);

// Cible invisible que l'avatar regarde (on ne bouge JAMAIS la caméra)
const lookTarget = new THREE.Object3D();
lookTarget.position.set(0, 1.3, 2.0);
scene.add(lookTarget);

// ── 2bis. Références pour l'animation des bras ──────────────────────
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
    cacheBones(); // tous les os du rig, pour la machine à poses

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
  // Parallaxe caméra légère : renforce la sensation de volume
  camera.position.x = nx * 0.12;
  camera.position.y = 0.85 + ny * 0.08;
  camera.lookAt(0, 0.85, 0);
});

let blinkTimer = 0;
let nextBlink = 2;

const clock = new THREE.Clock();

// ── Machine à poses : idle / walk / sit / lie / jump ──────────────────
type PoseName = "idle" | "walk" | "sit" | "lie" | "jump";
let pose: PoseName = "idle";
let poseUntil = 0; // fin des poses temporisées (sit/lie)
let jumpStart = -1; // instant de départ du saut
let stroll: { dir: number; start: number; end: number } | null = null;
let moveState = { x: 0, y: 0, scale: 1, maxX: 0 };

const bones: Record<string, THREE.Object3D | null> = {};
function cacheBones() {
  if (!vrm) return;
  for (const n of [
    "hips", "spine", "chest",
    "leftUpperArm", "rightUpperArm", "leftLowerArm", "rightLowerArm",
    "leftHand", "rightHand",
    "leftUpperLeg", "rightUpperLeg", "leftLowerLeg", "rightLowerLeg",
  ]) {
    bones[n] = vrm.humanoid?.getNormalizedBoneNode(n as never) ?? null;
  }
}

// Déplacement de la FENÊTRE (partagé avec le drag)
function moveWindowTo(x: number, y: number): void {
  getCurrentWindow().setPosition(new PhysicalPosition(Math.round(x), Math.round(y)));
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
  moveState = {
    x: pos.x,
    y: pos.y,
    scale,
    maxX: monX + monW - size.width, // bornes de l'écran
  };
  if (moveState.maxX < monX) moveState.maxX = monX;
  let dir = Math.random() < 0.5 ? -1 : 1;
  // si déjà collé au bord, repart dans l'autre sens
  if ((dir < 0 && pos.x <= monX + 10) || (dir > 0 && pos.x >= moveState.maxX - 10)) dir = -dir;
  stroll = { dir, start: performance.now(), end: performance.now() + 1500 + Math.random() * 1500 };
  pose = "walk";
}

// ── Couche de gestes (micro-comportements humains) ──────────────────
type Gesture = "none" | "pockets" | "crossed" | "talk" | "scratch" | "wave";
let gesture: Gesture = "none";
let gestureUntil = 0; // fin des gestes spontanés

// Lissage courant (mis à jour chaque frame par animate)
let blendK = 0;
const set = (name: string, tx = 0, ty = 0, tz = 0) => {
  const b = bones[name];
  if (!b) return;
  b.rotation.x += (tx - b.rotation.x) * blendK;
  b.rotation.y += (ty - b.rotation.y) * blendK;
  b.rotation.z += (tz - b.rotation.z) * blendK;
};

function applyArms(t: number): void {
  // Fin des gestes temporisés
  if (gesture !== "none" && gesture !== "talk" && performance.now() > gestureUntil) {
    gesture = "none";
  }
  // Parler = gesticulation automatique (prioritaire, pendant idle)
  const talking = !bubble.classList.contains("hidden");
  if (talking && pose === "idle") gesture = "talk";

  switch (gesture) {
    case "pockets": {
      // Mains dans les poches : bras serrés, avant-bras vers les hanches
      set("leftUpperArm", -0.12, 0, -1.5);
      set("rightUpperArm", -0.12, 0, 1.5);
      set("leftLowerArm", 0.55, 0.35, 0);
      set("rightLowerArm", 0.55, -0.35, 0);
      set("leftHand", 0.3, 0, 0.2);
      set("rightHand", 0.3, 0, -0.2);
      break;
    }
    case "crossed": {
      // Bras croisés sur la poitrine
      set("leftUpperArm", -0.5, 0, -0.75);
      set("rightUpperArm", -0.5, 0, 0.75);
      set("leftLowerArm", 1.25, -0.7, 0.15);
      set("rightLowerArm", 1.25, 0.7, -0.15);
      set("leftHand", 0.2, 0, 0);
      set("rightHand", 0.2, 0, 0);
      break;
    }
    case "talk": {
      // Gesticulation au rythme de la parole (syncro avec la bouche)
      const g = Math.sin(t * 3.1); // rythme lent de phrase
      const m = Math.abs(Math.sin(t * 12)); // rythme rapide des syllabes
      set("leftUpperArm", g * 0.15, 0, -ARM_DOWN + 0.35 + m * 0.12);
      set("rightUpperArm", -g * 0.15, 0, ARM_DOWN - 0.3 - m * 0.15);
      set("leftLowerArm", 0.75 + Math.sin(t * 2.3) * 0.3, 0, Math.sin(t * 2.7) * 0.15);
      set("rightLowerArm", 0.6 + Math.sin(t * 2.9 + 1) * 0.25, 0, Math.sin(t * 3.1 + 2) * 0.2);
      set("leftHand", 0.2 + m * 0.3, 0, Math.sin(t * 4.7) * 0.25);
      set("rightHand", 0.2 - m * 0.3, 0, Math.sin(t * 4.2 + 1) * 0.25);
      break;
    }
    case "scratch": {
      // Main droite vers la tête (réflexion)
      set("leftUpperArm", 0, 0, -ARM_DOWN);
      set("leftLowerArm", 0.25, 0, 0);
      set("leftHand");
      set("rightUpperArm", -0.25, 0, 0.6);
      set("rightLowerArm", 2.1, -0.4, 0);
      set("rightHand", 0.4, 0, Math.sin(t * 6) * 0.2); // il gratte vraiment
      break;
    }
    case "wave": {
      // Salut : bras droit levé, avant-bras qui s'agite
      set("leftUpperArm");
      set("leftLowerArm", 0.25, 0, 0);
      set("leftHand");
      set("rightUpperArm", 0, 0, 2.55); // bras en l'air
      set("rightLowerArm", 0.2, 0, Math.sin(t * 8) * 0.35);
      set("rightHand", 0, 0, Math.sin(t * 8 + 0.5) * 0.3);
      break;
    }
    case "none": {
      // Repos : balancement doux (le comportement par défaut)
      set("leftUpperArm", Math.sin(t * 0.7) * 0.03, 0, -ARM_DOWN + Math.sin(t * 1.6) * 0.05);
      set("rightUpperArm", Math.sin(t * 0.7 + Math.PI) * 0.03, 0, ARM_DOWN + Math.sin(t * 1.6 + Math.PI) * 0.05);
      set("leftLowerArm", 0.25 + Math.sin(t * 1.6 + 0.6) * 0.04, Math.sin(t * 0.9) * 0.03, 0);
      set("rightLowerArm", 0.25 + Math.sin(t * 1.6 + Math.PI + 0.6) * 0.04, Math.sin(t * 0.9 + Math.PI) * 0.03, 0);
      set("leftHand", Math.sin(t * 1.1) * 0.05, 0, Math.sin(t * 1.6 + 1.2) * 0.06);
      set("rightHand", Math.sin(t * 1.1 + Math.PI) * 0.05, 0, Math.sin(t * 1.6 + Math.PI + 1.2) * 0.06);
      break;
    }
  }
}

function animate() {
  requestAnimationFrame(animate);
  const delta = clock.getDelta();
  const t = clock.elapsedTime;

  if (!vrm) {
    renderer.render(scene, camera);
    return;
  }

  // Fin des poses temporisées
  const nowMs = performance.now();
  if ((pose === "sit" || pose === "lie") && nowMs > poseUntil) pose = "idle";

  // Lissage : les rotations glissent vers leur cible (transitions douces)
  blendK = 1 - Math.exp(-delta * 7);

  // Hanches : position verticale + rotation, lissées
  const hips = bones["hips"];
  const baseHipY = hips ? (hips.userData.baseY ??= hips.position.y) : 0;

  let hipsY = baseHipY;
  let hipsRX = 0;
  let hipsRY = 0; // orientation du corps (3/4 de profil en marche)
  let hipsRZ = 0;

  switch (pose) {
    case "idle": {
      // Jambes détendues, bras pilotés par la couche de gestes
      set("leftUpperLeg");
      set("rightUpperLeg");
      set("leftLowerLeg");
      set("rightLowerLeg");
      applyArms(t);
      hipsY = baseHipY + Math.sin(t * 1.6) * 0.006;
      hipsRY = 0; // face à toi au repos
      hipsRZ = Math.sin(t * 0.8) * 0.02;
      break;
    }
    case "walk": {
      // Cycle complet : bras et jambes en opposition croisée,
      // genoux qui plient pendant la phase de balancement
      const w = Math.sin(t * 6);
      const c = Math.cos(t * 6); // phase de balancement des genoux
      // Bras : le rig étant symétrique (gauche=-X, droit=+X), le swing
      // naturel utilise le MÊME signe pour les deux — pas de miroir ici.
      // Bras plus proches du corps (±1.4) avec une amplitude modérée.
      set("leftUpperArm", 0, 0, -1.4 + w * 0.25);
      set("rightUpperArm", 0, 0, 1.4 + w * 0.25);
      set("leftLowerArm", 0.3, 0, 0);
      set("rightLowerArm", 0.3, 0, 0);
      set("leftHand", 0, 0, Math.sin(t * 6 + 1.2) * 0.1);
      set("rightHand", 0, 0, Math.sin(t * 6 + Math.PI + 1.2) * 0.1);
      set("leftUpperLeg", w * 0.5);
      set("rightUpperLeg", -w * 0.5);
      set("leftLowerLeg", 0.2 + Math.max(0, c) * 0.55); // genou plie au swing
      set("rightLowerLeg", 0.2 + Math.max(0, -c) * 0.55);
      hipsY = baseHipY + Math.abs(Math.sin(t * 6)) * 0.012;
      // 3/4 de profil vers la direction de marche (on garde le regard vers toi)
      hipsRY = (stroll?.dir ?? 0) * 1.05;
      hipsRZ = Math.sin(t * 6) * 0.03;

      // Déplacement réel de la fenêtre pendant la marche,
      // avec accélération/décélération en douceur
      if (stroll) {
        if (performance.now() < stroll.end) {
          const elapsed = (performance.now() - stroll.start) / 1000;
          const remain = (stroll.end - performance.now()) / 1000;
          const ease = Math.min(1, elapsed / 0.4) * Math.min(1, remain / 0.4);
          moveState.x += stroll.dir * 70 * Math.max(0.15, ease) * moveState.scale * delta;
          moveState.x = Math.max(0, Math.min(moveState.maxX, moveState.x));
          moveWindowTo(moveState.x, moveState.y);
        } else {
          stroll = null;
          pose = "idle";
        }
      } else {
        pose = "idle";
      }
      break;
    }
    case "sit": {
      // Hanches baissées, cuisses horizontales, tibias vers le bas
      set("leftUpperArm", -0.15, 0, -ARM_DOWN - 0.1);
      set("rightUpperArm", -0.15, 0, ARM_DOWN + 0.1);
      set("leftLowerArm", 0.4, 0, 0);
      set("rightLowerArm", 0.4, 0, 0);
      set("leftUpperLeg", -1.5);
      set("rightUpperLeg", -1.5);
      set("leftLowerLeg", 1.5);
      set("rightLowerLeg", 1.5);
      hipsY = baseHipY - 0.33;
      break;
    }
    case "lie": {
      // Couché SUR LE CÔTÉ (roulé dans le plan de l'écran) :
      // lisible depuis la caméra frontale, contrairement à une bascule avant/arrière
      hipsRZ = -Math.PI / 2 * 0.95;
      hipsY = baseHipY * 0.45;
      set("leftUpperArm", 0, 0, -ARM_DOWN - 0.35);
      set("rightUpperArm", 0, 0, ARM_DOWN + 0.35);
      set("leftLowerArm", 0.3, 0, 0);
      set("rightLowerArm", 0.3, 0, 0);
      set("leftUpperLeg", -0.15);
      set("rightUpperLeg", -0.05);
      set("leftLowerLeg", 0.3);
      set("rightLowerLeg", 0.1);
      break;
    }
    case "jump": {
      // Timeline complète : accroupi → propulsion → air (parabole) → réception
      const dur = 0.9;
      const p = (t - jumpStart) / dur;
      if (p >= 1) {
        pose = "idle";
      } else if (p >= 0) {
        if (p < 0.22) {
          // 1. Accroupi (anticipation, chargement)
          const c = Math.sin((p / 0.22) * Math.PI / 2); // 0 → 1
          hipsY = baseHipY - 0.09 * c;
          set("leftUpperLeg", -0.9 * c);
          set("rightUpperLeg", -0.9 * c);
          set("leftLowerLeg", 1.2 * c);
          set("rightLowerLeg", 1.2 * c);
          set("leftUpperArm", 0, 0, -ARM_DOWN - 0.3 * c); // bras en arrière
          set("rightUpperArm", 0, 0, ARM_DOWN + 0.3 * c);
        } else if (p < 0.78) {
          // 2. En l'air : vraie parabole, genoux repliés, bras levés
          const a = (p - 0.22) / 0.56; // 0 → 1
          const s = Math.sin(Math.PI * a);
          hipsY = baseHipY + 0.3 * s;
          set("leftUpperLeg", -0.5 * s);
          set("rightUpperLeg", -0.5 * s);
          set("leftLowerLeg", 0.9 * s);
          set("rightLowerLeg", 0.9 * s);
          set("leftUpperArm", 0, 0, -ARM_DOWN + 0.6 * s);
          set("rightUpperArm", 0, 0, ARM_DOWN - 0.6 * s);
          set("leftHand", 0, 0, -0.3 * s);
          set("rightHand", 0, 0, 0.3 * s);
        } else {
          // 3. Réception : ré-accroupi amorti qui se relâche
          const c = 1 - (p - 0.78) / 0.22; // 1 → 0
          hipsY = baseHipY - 0.08 * c;
          set("leftUpperLeg", -0.7 * c);
          set("rightUpperLeg", -0.7 * c);
          set("leftLowerLeg", 1.0 * c);
          set("rightLowerLeg", 1.0 * c);
          set("leftUpperArm", 0, 0, -ARM_DOWN - 0.2 * c);
          set("rightUpperArm", 0, 0, ARM_DOWN + 0.2 * c);
        }
      }
      break;
    }
  }

  if (hips) {
    hips.position.y += (hipsY - hips.position.y) * blendK;
    hips.rotation.x += (hipsRX - hips.rotation.x) * blendK;
    hips.rotation.y += (hipsRY - hips.rotation.y) * blendK;
    hips.rotation.z += (hipsRZ - hips.rotation.z) * blendK;
  }

  // L'ombre suit la hauteur : en l'air → plus petite, plus pâle
  const air = Math.max(0, hipsY - baseHipY);
  const shadowScale = 1 / (1 + air * 2.5);
  shadow.scale.setScalar(shadowScale);
  (shadow.material as THREE.MeshBasicMaterial).opacity = 0.28 * shadowScale;

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

// ── 4quinques. Autonomie : promenades et poses spontanées ───────────
setInterval(() => {
  if (dragging || thinking || pose !== "idle") return; // occupé → il attend
  const r = Math.random();
  if (r < 0.3) {
    void startStroll(); // il part en balade
  } else if (r < 0.45) {
    pose = "sit"; // il s'assoit un moment
    poseUntil = performance.now() + 6000 + Math.random() * 8000;
  } else if (r < 0.55) {
    pose = "lie"; // petite sieste
    poseUntil = performance.now() + 5000 + Math.random() * 7000;
  } else if (r < 0.65) {
    jumpStart = clock.elapsedTime + 0.05; // hop !
    pose = "jump";
  } else if (r < 0.85) {
    // Gesture spontané : il vit sa vie
    const pool: Gesture[] = ["pockets", "crossed", "scratch", "wave", "none"];
    gesture = pool[Math.floor(Math.random() * pool.length)];
    gestureUntil = performance.now() + 4000 + Math.random() * 4000;
  }
  // sinon : il reste tranquille
}, 5000);

// ── 4sexties. Commandes de pose (texte ou voix) ─────────────────────
const POSE_COMMANDS: Array<[RegExp, PoseName, string]> = [
  [/\b(assis|assois[- ]?toi|s'asseoir|s'assoire|installe[- ]toi)\b/, "sit", "Je m'installe confortablement 🪺"],
  [/\b(saute|sauter|hop)\b/, "jump", "Hop ! 🐇"],
  [/\b(allonge|allonger|couche|coucher|sieste|dors)\b/, "lie", "Ahhh, je me repose 😌"],
  [/\b(debout|debout|lève|leve|releve)\b/, "idle", "Me voilà debout ! 🧍"],
];

function matchPoseCommand(text: string): [PoseName, string] | null {
  const lower = text.toLowerCase();
  for (const [re, p, reply] of POSE_COMMANDS) {
    if (re.test(lower)) return [p, reply];
  }
  return null;
}

// Gestes sur commande : "croise les bras", "mains dans les poches"...
const GESTURE_COMMANDS: Array<[RegExp, Gesture]> = [
  [/\b(poches?|mains dans les poches)\b/, "pockets"],
  [/\b(croise|croiser)\b/, "crossed"],
  [/\b(gratte|gratter|démange)\b/, "scratch"],
  [/\b(salut|coucou|saluer)\b/, "wave"],
];

function matchGestureCommand(text: string): Gesture | null {
  const lower = text.toLowerCase();
  for (const [re, g] of GESTURE_COMMANDS) {
    if (re.test(lower)) return g;
  }
  return null;
}

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

// Boucle de conversation commune : texte → LLM (streaming) → bulle → TTS
async function converse(text: string): Promise<void> {
  // Commandes de pose : gérées localement, pas besoin du LLM
  const poseCmd = matchPoseCommand(text);
  if (poseCmd) {
    const [p, reply] = poseCmd;
    if (p === "jump") {
      jumpStart = clock.elapsedTime + 0.05;
      pose = "jump";
    } else {
      pose = p;
      poseUntil = performance.now() + 15000; // il obéit ~15 s puis reprend
    }
    say(reply, 4000);
    return;
  }

  // Gestes sur commande : appliqués silencieusement, puis le LLM répond
  const gest = matchGestureCommand(text);
  if (gest) {
    gesture = gest;
    gestureUntil = performance.now() + 9000;
  }

  bubble.textContent = "…";
  bubble.classList.remove("hidden");
  thinking = true;
  try {
    // Streaming : chaque token du LLM est affiché au fil de l'eau
    const onChunk = new Channel<string>();
    let acc = "";
    onChunk.onmessage = (tok) => {
      acc += tok;
      bubble.textContent = acc;
    };
    const reply = await invoke<string>("chat", { userMessage: text, onChunk });
    // Une fois le flux terminé, on planifie la disparition de la bulle
    setTimeout(() => bubble.classList.add("hidden"), Math.min(12000, 2500 + reply.length * 60));
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

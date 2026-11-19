import { getCurrentWindow, currentMonitor, PhysicalPosition } from "@tauri-apps/api/window";
import { invoke } from "@tauri-apps/api/core";
import { onPrefs, state } from "./state";
import { accessCard, robot, say } from "./ui";

// ── Poses, déplacements, zones interdites, autonomie ─────────────────

export type PoseName = "idle" | "walk" | "sit";
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
let moveState = { x: 0, y: 0, scale: 1, minX: 0, maxX: 0 };

// ── Zones interdites (rectangles en pixels physiques) ────────────────
export interface Zone { x: number; y: number; w: number; h: number; label: string; }
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

export function getZones(): Zone[] {
  return zones;
}

export function setZones(next: Zone[]): void {
  zones = next;
}

export function overlapsZone(px: number, py: number, pw: number, ph: number): Zone | null {
  for (const z of zones) {
    if (px < z.x + z.w && px + pw > z.x && py < z.y + z.h && py + ph > z.y) return z;
  }
  return null;
}

/** Limites de la FENÊTRE pour que l'ŒUF reste dans l'écran (bords
 *  compris) : la fenêtre, plus grande que l'œuf pour loger la bulle,
 *  peut déborder de l'écran. `egg(x, y)` : rectangle de l'œuf (pixels
 *  physiques) quand la fenêtre est en (x, y). */
export async function eggBounds() {
  const win = getCurrentWindow();
  const [pos, mon] = await Promise.all([win.outerPosition(), currentMonitor()]);
  const s = mon?.scaleFactor ?? 1;
  const monX = mon?.position?.x ?? 0;
  const monY = mon?.position?.y ?? 0;
  const monW = mon?.size?.width ?? 1920;
  const monH = mon?.size?.height ?? 1080;
  const r = robot.getBoundingClientRect();
  const M = 10;
  return {
    pos,
    scale: s,
    minX: monX - r.left * s + M,
    maxX: monX + monW - r.right * s - M,
    minY: monY - r.top * s + M,
    maxY: monY + monH - r.bottom * s - M,
    egg: (x: number, y: number) => ({ x: x + r.left * s, y: y + r.top * s, w: r.width * s, h: r.height * s }),
  };
}

/** L'œuf, si la fenêtre était en (x, y), chevaucherait-il une zone interdite ? */
function eggInZone(b: Awaited<ReturnType<typeof eggBounds>>, x: number, y: number): Zone | null {
  const e = b.egg(x, y);
  return overlapsZone(e.x, e.y, e.w, e.h);
}

export function moveWindowTo(x: number, y: number): void {
  getCurrentWindow().setPosition(new PhysicalPosition(Math.round(x), Math.round(y)));
}

export function applyPose(p: PoseName): void {
  pose = p;
  robot.classList.remove("pose-sit", "pose-glide");
  robot.classList.remove("talking");
  if (p !== "idle") robot.classList.add(p === "walk" ? "pose-glide" : `pose-${p}`);
}

async function startStroll(): Promise<void> {
  const b = await eggBounds();
  const { pos } = b;
  moveState = { x: pos.x, y: pos.y, scale: b.scale, minX: b.minX, maxX: Math.max(b.minX, b.maxX) };
  let dir = Math.random() < 0.5 ? -1 : 1;
  if ((dir < 0 && pos.x <= b.minX + 10) || (dir > 0 && pos.x >= moveState.maxX - 10)) {
    dir = -dir;
  }
  stroll = { dir, start: performance.now(), end: performance.now() + 1500 + Math.random() * 1500 };
  applyPose("walk" as PoseName); // glide (l'œuf roule, il ne marche pas)
}

// Glisser vers une position cible (commandes vocales/texte)
export async function glideTo(tx?: number, ty?: number): Promise<void> {
  const b = await eggBounds();
  const { pos } = b;
  moveState = { x: pos.x, y: pos.y, scale: b.scale, minX: b.minX, maxX: Math.max(b.minX, b.maxX) };

  // 🚫 Zones interdites : si la cible en chevauche une, on cherche la
  // position autorisée la plus proche (glissement vertical puis horizontal)
  const px = tx ?? pos.x;
  const py = ty ?? pos.y;
  let fx = px;
  let fy = py;
  if (eggInZone(b, fx, fy)) {
    let found = false;
    for (let d = 20; d <= 600 && !found; d += 20) {
      for (const [ox, oy] of [[0, -d], [0, d], [-d, 0], [d, 0], [-d, -d], [d, d], [-d, d], [d, -d]]) {
        if (!eggInZone(b, px + ox, fy + oy)) {
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
    targetX: Math.max(b.minX, Math.min(b.maxX, fx)),
    targetY: Math.max(b.minY, Math.min(b.maxY, fy)),
  };
  applyPose("walk");
}

// ── Modes de déplacement (réglage, ou demandé à la voix) ──
// libre : il se balade ; fixe : il reste où on le pose ; bord : il se pose
// au bord de l'écran le plus proche ; concentration : petit et discret
// dans le coin en bas à droite, sans balade ni prise de parole.
let appliedMode = "";
onPrefs((p, first) => {
  const mode = p.move_mode || "libre";
  robot.classList.toggle("focus-mode", mode === "concentration");
  if (mode === appliedMode) return;
  appliedMode = mode;
  // au démarrage, on ne déplace pas un œuf que Julien a déjà posé
  if (first && mode !== "concentration") return;
  if (mode === "bord") void glideToEdge();
  if (mode === "concentration") void glideToCorner();
});

async function glideToEdge(): Promise<void> {
  const b = await eggBounds();
  const { x, y } = b.pos;
  const d = { left: x - b.minX, right: b.maxX - x, bottom: b.maxY - y, top: y - b.minY };
  const nearest = (Object.keys(d) as Array<keyof typeof d>).reduce((a, k) => (d[k] < d[a] ? k : a), "bottom");
  if (nearest === "left") await glideTo(b.minX, y);
  else if (nearest === "right") await glideTo(b.maxX, y);
  else if (nearest === "top") await glideTo(x, b.minY);
  else await glideTo(x, b.maxY);
}

async function glideToCorner(): Promise<void> {
  const b = await eggBounds();
  await glideTo(b.maxX, b.maxY);
}

// Boucle d'autonomie : il vit sa vie quand personne ne l'occupe
setInterval(() => {
  if (
    state.dragging || state.thinking || state.ozSpeaking || state.micRunning || state.performing ||
    pose !== "idle" || !accessCard.classList.contains("hidden")
  ) return;
  if (state.moveMode === "concentration") return; // il ne bouge plus du tout
  const r = Math.random();
  if (r < 0.3 && state.moveMode === "libre") {
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
    // Julien l'attrape : la balade s'arrête net, sinon elle remet la
    // fenêtre à son ancienne position toutes les 33 ms et l'œuf « résiste »
    if (state.dragging) {
      stroll = null;
      applyPose("idle");
      return;
    }
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
      moveState.x = Math.max(moveState.minX, Math.min(moveState.maxX, moveState.x));
      moveWindowTo(moveState.x, moveState.y);
    }
  }
}, 33);

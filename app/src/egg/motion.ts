import { getCurrentWindow, currentMonitor, PhysicalPosition } from "@tauri-apps/api/window";
import { invoke } from "@tauri-apps/api/core";
import { state } from "./state";
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
let moveState = { x: 0, y: 0, scale: 1, maxX: 0 };

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

function moveWindowTo(x: number, y: number): void {
  getCurrentWindow().setPosition(new PhysicalPosition(Math.round(x), Math.round(y)));
}

export function applyPose(p: PoseName): void {
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
export async function glideTo(tx?: number, ty?: number): Promise<void> {
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
  if (
    state.dragging || state.thinking || state.ozSpeaking || state.micRunning ||
    pose !== "idle" || !accessCard.classList.contains("hidden")
  ) return;
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

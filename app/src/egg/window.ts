import { getCurrentWindow, currentMonitor, PhysicalPosition } from "@tauri-apps/api/window";
import { invoke } from "@tauri-apps/api/core";
import { onEggClick } from "./initiative";
import { eggBounds, glideTo, overlapsZone } from "./motion";
import { state } from "./state";
import { accessCard, bubble, eyeLeft, eyeRight, placeBubble, robot, say, stage } from "./ui";

// ── Fenêtre transparente : drag, click-through, regard, clignement ───

// ── Drag manuel DPI-aware ──
let dragStart: { wx: number; wy: number; sx: number; sy: number; scale: number } | null = null;
let downAt = { t: 0, x: 0, y: 0 };
let lastMove: { x: number; y: number } | null = null;
let moveQueued = false;

stage.addEventListener("pointerdown", async (e) => {
  if (e.button !== 0) return;
  e.preventDefault();
  // dragging AVANT l'await : la balade s'arrête tout de suite et ne
  // reprend pas la main sur la fenêtre pendant qu'on la tient
  state.dragging = true;
  dragStart = null;
  // capture : on garde les mouvements même si le curseur sort de la fenêtre
  try { stage.setPointerCapture(e.pointerId); } catch { /* pointeur déjà relâché */ }
  downAt = { t: performance.now(), x: e.screenX, y: e.screenY };
  const sx = e.screenX;
  const sy = e.screenY;
  const win = getCurrentWindow();
  const [pos, mon] = await Promise.all([win.outerPosition(), currentMonitor()]);
  if (!state.dragging) return; // relâché pendant la lecture de la position
  dragStart = { wx: pos.x, wy: pos.y, sx, sy, scale: mon?.scaleFactor ?? 1 };
});

// un seul setPosition par image : sinon les appels s'empilent et l'œuf traîne
function flushMove(): void {
  moveQueued = false;
  if (!state.dragging || !dragStart || !lastMove) return;
  const dx = Math.round((lastMove.x - dragStart.sx) * dragStart.scale);
  const dy = Math.round((lastMove.y - dragStart.sy) * dragStart.scale);
  void getCurrentWindow().setPosition(
    new PhysicalPosition(dragStart.wx + dx, dragStart.wy + dy)
  );
}

window.addEventListener("pointermove", (e) => {
  if (!state.dragging) return;
  lastMove = { x: e.screenX, y: e.screenY };
  if (!moveQueued) {
    moveQueued = true;
    requestAnimationFrame(flushMove);
  }
});

// capture perdue (Alt+Tab, clic droit système...) : on lâche proprement
window.addEventListener("pointercancel", () => {
  state.dragging = false;
  dragStart = null;
  lastMove = null;
});

window.addEventListener("pointerup", (e) => {
  if (!state.dragging) return;
  state.dragging = false;
  dragStart = null;
  lastMove = null;
  // simple clic (peu de mouvement, bref) -> il t'écoute (ou répond à sa main levée)
  const moved = Math.hypot(e.screenX - downAt.x, e.screenY - downAt.y);
  if (moved < 5 && performance.now() - downAt.t < 350) {
    onEggClick();
    return;
  }
  void placeBubble(true);
  // 🚫 Déposé dans une zone interdite ? Il s'échappe tout seul
  void (async () => {
    const b = await eggBounds();
    const egg = b.egg(b.pos.x, b.pos.y);
    if (overlapsZone(egg.x, egg.y, egg.w, egg.h)) {
      say("C'est ma zone interdite, je ne reste pas là !", 4000);
      await glideTo(); // reste sur place MAIS cherche la place libre la plus proche
    }
  })();
});

// ── Click-through : la zone active épouse l'œuf ──
let cursorOverRobot = true;
const HIT_MARGIN = 24; // px logiques autour de l'œuf, de la bulle et de la carte

async function hitTestLoop(): Promise<void> {
  if (state.dragging) return;
  try {
    const win = getCurrentWindow();
    // un seul appel au Rust : curseur déjà ramené à la fenêtre, en pixels logiques
    const { x: lx, y: ly } = await invoke<{ x: number; y: number }>("cursor_in_window");

    // Zone active : l'œuf + la bulle + la carte d'accès, avec une marge.
    // Sans elle, un clic donné juste en arrivant sur l'œuf partait avant que
    // la fenêtre cesse de laisser passer les clics : il tombait sur la
    // fenêtre du dessous (souvent le panneau, centré sous l'œuf), qui
    // passait alors au premier plan.
    const overUi = [robot, bubble, accessCard].some((el) => {
      // un élément masqué ne doit pas bloquer les clics derrière lui
      if (el.classList.contains("hidden")) return false;
      const r = el.getBoundingClientRect();
      return lx >= r.left - HIT_MARGIN && lx <= r.right + HIT_MARGIN && ly >= r.top - HIT_MARGIN && ly <= r.bottom + HIT_MARGIN;
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
setInterval(hitTestLoop, 40);

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

// ── Clignement des yeux ──
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

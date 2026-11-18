import { getCurrentWindow, currentMonitor, PhysicalPosition } from "@tauri-apps/api/window";
import { invoke } from "@tauri-apps/api/core";
import { toggleChatBar } from "./chatbar";
import { glideTo, overlapsZone } from "./motion";
import { state } from "./state";
import { accessCard, bubble, chatBar, eyeLeft, eyeRight, robot, say, stage } from "./ui";

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

// ── Click-through : la zone active épouse l'œuf ──
let cursorOverRobot = true;

async function hitTestLoop(): Promise<void> {
  if (state.dragging) return;
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

    // Zone active : l'œuf + la bulle + la barre de chat + la carte d'accès
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

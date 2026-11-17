import { getCurrentWindow, currentMonitor, PhysicalPosition } from "@tauri-apps/api/window";
import { invoke } from "@tauri-apps/api/core";
import { toggleChatBar } from "./chatbar";
import { glideTo, overlapsZone } from "./motion";
import { state } from "./state";
import { accessCard, bubble, chatBar, eyeLeft, eyeRight, robot, say, stage } from "./ui";

// ── Fenêtre transparente : drag, click-through, regard, clignement ───

// ── Drag manuel DPI-aware ──
let dragStart = { wx: 0, wy: 0, sx: 0, sy: 0, scale: 1 };
let downAt = { t: 0, x: 0, y: 0 };

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
  state.dragging = true;
  downAt = { t: performance.now(), x: e.screenX, y: e.screenY };
});

window.addEventListener("pointermove", (e) => {
  if (!state.dragging) return;
  const dx = Math.round((e.screenX - dragStart.sx) * dragStart.scale);
  const dy = Math.round((e.screenY - dragStart.sy) * dragStart.scale);
  getCurrentWindow().setPosition(
    new PhysicalPosition(dragStart.wx + dx, dragStart.wy + dy)
  );
});

window.addEventListener("pointerup", (e) => {
  if (!state.dragging) return;
  state.dragging = false;
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

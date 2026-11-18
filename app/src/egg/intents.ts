import { getCurrentWindow, currentMonitor } from "@tauri-apps/api/window";
import { listen } from "@tauri-apps/api/event";
import { applyPose, glideTo } from "./motion";

// ── Actions du corps d'Oz, déclenchées par le cerveau ────────────────
// Plus aucun mot-clé ici : Gemini comprend la demande de Julien et appelle
// un outil (src-tauri/src/tools.rs). Les outils qui concernent le corps de
// l'œuf (se déplacer, changer de pose) arrivent par l'événement `oz://do`.
// La réponse parlée vient du cerveau : ces actions restent silencieuses.

interface DoEvent { action: "move" | "pose"; direction?: string; pose?: string }

void listen<DoEvent>("oz://do", (e) => {
  const { action, direction, pose } = e.payload;
  if (action === "move" && direction) void moveTo(direction);
  if (action === "pose") applyPose(pose === "assis" ? "sit" : "idle");
});

/** Glisse vers un bord / coin / le centre de l'écran courant. */
async function moveTo(direction: string): Promise<void> {
  const win = getCurrentWindow();
  const [mon, size] = await Promise.all([currentMonitor(), win.outerSize()]);
  const monX = mon?.position?.x ?? 0;
  const monY = mon?.position?.y ?? 0;
  const monW = mon?.size?.width ?? 1920;
  const monH = mon?.size?.height ?? 1080;
  const words = direction.split(" ");

  let tx: number | undefined;
  let ty: number | undefined;
  if (words.includes("gauche")) tx = monX + 10;
  if (words.includes("droite")) tx = monX + monW - size.width - 10;
  if (words.includes("haut")) ty = monY + 10;
  if (words.includes("bas")) ty = monY + monH - size.height - 10;
  if (words.includes("centre")) {
    tx = monX + (monW - size.width) / 2;
    ty = monY + (monH - size.height) / 2;
  }
  if (tx !== undefined || ty !== undefined) await glideTo(tx, ty);
}

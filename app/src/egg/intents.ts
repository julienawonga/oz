import { listen } from "@tauri-apps/api/event";
import { performFigure, type Target } from "./figures";
import { stopMicStream } from "./mic";
import { applyPose, eggBounds, glideTo } from "./motion";
import { state } from "./state";

// ── Actions du corps d'Oz, déclenchées par le cerveau ────────────────
// Plus aucun mot-clé ici : Gemini comprend la demande de Julien et appelle
// un outil (src-tauri/src/tools.rs). Les outils qui concernent le corps de
// l'œuf (se déplacer, changer de pose) arrivent par l'événement `oz://do`.
// La réponse parlée vient du cerveau : ces actions restent silencieuses.

interface DoEvent {
  action: "move" | "pose" | "figure" | "goto" | "end";
  direction?: string;
  pose?: string;
  figure?: string;
  x?: number; // lieu nommé (position de la fenêtre)
  y?: number;
}

void listen<DoEvent>("oz://do", (e) => {
  const { action, direction, pose, figure, x, y } = e.payload;
  if (action === "figure" && figure) void figureTo(figure, direction, x, y);
  if (action === "goto" && x !== undefined && y !== undefined) void glideTo(x, y);
  if (action === "end") void endConversation();
  if (action === "move" && direction) void moveTo(direction);
  if (action === "pose") applyPose(pose === "assis" ? "sit" : "idle");
});

/** Figure, avec une destination éventuelle (lieu ou direction). */
async function figureTo(figure: string, direction?: string, x?: number, y?: number): Promise<void> {
  let target: Target | undefined;
  if (x !== undefined && y !== undefined) target = { x, y };
  else if (direction) target = await directionTarget(direction);
  await performFigure(figure, target);
}

/** Glisse vers un bord / coin / le centre de l'écran courant. */
async function moveTo(direction: string): Promise<void> {
  const { x, y } = await directionTarget(direction);
  if (x !== undefined || y !== undefined) await glideTo(x, y);
}

/** Position de fenêtre pour une direction : c'est l'ŒUF qui touche le
 *  bord, pas sa fenêtre (plus grande que lui). */
async function directionTarget(direction: string): Promise<Target> {
  const b = await eggBounds();
  const words = direction.toLowerCase().split(/\s+/);

  let tx: number | undefined;
  let ty: number | undefined;
  if (words.includes("gauche")) tx = b.minX;
  if (words.includes("droite")) tx = b.maxX;
  if (words.includes("haut")) ty = b.minY;
  if (words.includes("bas")) ty = b.maxY;
  if (words.includes("centre")) {
    tx = (b.minX + b.maxX) / 2;
    ty = (b.minY + b.maxY) / 2;
  }
  return { x: tx, y: ty };
}

/** Julien clôt la conversation : Oz finit sa phrase, puis n'écoute plus. */
async function endConversation(): Promise<void> {
  const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));
  state.holdMic = false;
  // sa dernière réponse arrive juste après l'outil : on la laisse partir
  for (let waited = 0; waited < 4000 && !state.ozSpeaking; waited += 100) await sleep(100);
  for (let waited = 0; waited < 10_000 && state.ozSpeaking; waited += 100) await sleep(100);
  if (state.micRunning) await stopMicStream();
}

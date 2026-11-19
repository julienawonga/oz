import { applyPose, eggBounds, moveWindowTo } from "./motion";
import { state } from "./state";
import { robot } from "./ui";

// ── Figures : petits numéros d'Oz (outil Gemini « do_trick ») ──
// Deux familles :
//  · trajets : la FENÊTRE suit une courbe (zigzag, cercle, huit...) ;
//  · sur place : le CORPS s'anime en CSS (toupie, salto, demi-tour...).
// Une danse enchaîne les deux. Pendant une figure, la balade autonome
// est suspendue (state.performing).

export type Figure =
  | "zigzag" | "cercle" | "huit" | "vague" | "spirale"
  | "toupie" | "salto" | "demi_tour" | "saut" | "danse";

/** Trajet : décalage (px logiques) en fonction de t ∈ [0, 1]. */
type Path = { ms: number; at: (t: number, dir: number) => [number, number] };

const TAU = Math.PI * 2;
const PATHS: Partial<Record<Figure, Path>> = {
  // dents de scie en avançant vers le centre de l'écran
  zigzag: {
    ms: 3200,
    at: (t, dir) => {
      const teeth = 4;
      const tri = 1 - Math.abs(((t * teeth * 2) % 2) - 1); // 0 → 1 → 0
      return [dir * 420 * t, -70 * tri];
    },
  },
  cercle: {
    ms: 3000,
    at: (t, dir) => {
      const r = 130;
      return [dir * (r - r * Math.cos(TAU * t)), -r * Math.sin(TAU * t)];
    },
  },
  huit: {
    ms: 4200,
    at: (t, dir) => [dir * 150 * Math.sin(TAU * t), -70 * Math.sin(2 * TAU * t)],
  },
  vague: {
    ms: 3200,
    at: (t, dir) => [dir * 440 * t, -55 * Math.sin(3 * TAU * t)],
  },
  spirale: {
    ms: 3600,
    at: (t, dir) => {
      const r = 20 + 140 * t;
      return [dir * r * Math.sin(2.5 * TAU * t), -r * (1 - Math.cos(2.5 * TAU * t)) / 2];
    },
  },
};

/** Figures sur place : classe CSS et durée (voir styles.css, « Figures »). */
const MOVES: Partial<Record<Figure, number>> = {
  toupie: 1400,
  salto: 1100,
  demi_tour: 2600,
  saut: 1200,
};

const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));
/** ~30 images/s : comme la balade, sans inonder l'IPC de setPosition. */
const FRAME_MS = 33;

// Les figures demandées d'affilée (« un cercle puis une toupie ») s'enchaînent.
let queue: Promise<void> = Promise.resolve();

/** Destination (position de la fenêtre, pixels physiques) ; un axe absent
 *  garde la position actuelle. */
export interface Target { x?: number; y?: number }

export function performFigure(name: string, target?: Target): Promise<void> {
  queue = queue.then(() => run(name as Figure, target));
  return queue;
}

async function run(fig: Figure, target?: Target): Promise<void> {
  if (state.dragging) return;
  state.performing = true;
  try {
    if (target && (fig === "zigzag" || fig === "vague")) {
      // la figure EST le trajet : il ondule jusqu'à la destination
      await travel(fig, target);
    } else {
      if (target) await goTo(target); // il y va, puis fait sa figure là-bas
      if (fig === "danse") await dance();
      else if (PATHS[fig]) await runPath(PATHS[fig]!);
      else if (MOVES[fig]) await move(fig);
    }
  } finally {
    state.performing = false;
  }
}

async function move(fig: Figure): Promise<void> {
  robot.classList.add(`fig-${fig}`);
  await sleep(MOVES[fig]!);
  robot.classList.remove(`fig-${fig}`);
}

/** Suit un trajet, décalé pour qu'il tienne entièrement dans l'écran. */
async function runPath(path: Path): Promise<void> {
  const b = await eggBounds();
  const s = b.scale;
  // vers le centre de l'écran : il a la place de dérouler sa figure
  const dir = b.pos.x < (b.minX + b.maxX) / 2 ? 1 : -1;
  // boîte englobante du trajet, pour recaler l'origine
  let [x0, x1, y0, y1] = [0, 0, 0, 0];
  for (let i = 0; i <= 100; i++) {
    const [dx, dy] = path.at(i / 100, dir);
    x0 = Math.min(x0, dx * s); x1 = Math.max(x1, dx * s);
    y0 = Math.min(y0, dy * s); y1 = Math.max(y1, dy * s);
  }
  const ox = Math.max(b.minX - x0, Math.min(b.maxX - x1, b.pos.x));
  const oy = Math.max(b.minY - y0, Math.min(b.maxY - y1, b.pos.y));

  applyPose("walk"); // roulis du corps pendant le trajet
  // trop près d'un bord : il se recale d'abord en glissant vers l'origine
  if (Math.hypot(ox - b.pos.x, oy - b.pos.y) > 4) await tween(b.pos.x, b.pos.y, ox, oy, 350);
  const start = performance.now();
  await new Promise<void>((done) => {
    const frame = () => {
      if (state.dragging) return done(); // Julien l'attrape : on arrête
      const t = Math.min(1, (performance.now() - start) / path.ms);
      const [dx, dy] = path.at(t, dir);
      moveWindowTo(ox + dx * s, oy + dy * s);
      if (t < 1) setTimeout(frame, FRAME_MS);
      else done();
    };
    setTimeout(frame, FRAME_MS);
  });
  applyPose("idle");
}

/** Glisse jusqu'à la destination (bornée à l'écran). */
async function goTo(target: Target): Promise<void> {
  const b = await eggBounds();
  const tx = Math.max(b.minX, Math.min(b.maxX, target.x ?? b.pos.x));
  const ty = Math.max(b.minY, Math.min(b.maxY, target.y ?? b.pos.y));
  const dist = Math.hypot(tx - b.pos.x, ty - b.pos.y);
  if (dist < 4) return;
  applyPose("walk");
  await tween(b.pos.x, b.pos.y, tx, ty, Math.max(500, (dist / (380 * b.scale)) * 1000));
  applyPose("idle");
}

/** Zigzag ou vague en allant quelque part : l'ondulation est
 *  perpendiculaire au chemin et s'éteint au départ et à l'arrivée. */
async function travel(kind: "zigzag" | "vague", target: Target): Promise<void> {
  const b = await eggBounds();
  const s = b.scale;
  const fx = b.pos.x;
  const fy = b.pos.y;
  const tx = Math.max(b.minX, Math.min(b.maxX, target.x ?? fx));
  const ty = Math.max(b.minY, Math.min(b.maxY, target.y ?? fy));
  const dx = tx - fx;
  const dy = ty - fy;
  const dist = Math.hypot(dx, dy);
  if (dist < 40 * s) return runPath(PATHS[kind]!); // déjà là : figure sur place
  const [px, py] = [-dy / dist, dx / dist]; // perpendiculaire au chemin
  const cycles = Math.max(2, Math.round(dist / (150 * s)));
  const amp = 60 * s;
  const ms = Math.max(1800, (dist / (300 * s)) * 1000);
  const wave = (t: number) =>
    kind === "zigzag"
      ? 2 * (1 - Math.abs(((t * cycles * 2) % 2) - 1)) - 1 // triangle −1 → 1
      : Math.sin(TAU * cycles * t);

  applyPose("walk");
  const start = performance.now();
  await new Promise<void>((done) => {
    const frame = () => {
      if (state.dragging) return done();
      const t = Math.min(1, (performance.now() - start) / ms);
      const env = Math.min(1, t * 6, (1 - t) * 6); // calme au départ et à l'arrivée
      const off = amp * env * wave(t);
      const x = Math.max(b.minX, Math.min(b.maxX, fx + dx * t + px * off));
      const y = Math.max(b.minY, Math.min(b.maxY, fy + dy * t + py * off));
      moveWindowTo(x, y);
      if (t < 1) setTimeout(frame, FRAME_MS);
      else done();
    };
    setTimeout(frame, FRAME_MS);
  });
  applyPose("idle");
}

/** Glissement linéaire d'un point à un autre. */
function tween(fx: number, fy: number, tx: number, ty: number, ms: number): Promise<void> {
  const start = performance.now();
  return new Promise((done) => {
    const frame = () => {
      const t = Math.min(1, (performance.now() - start) / ms);
      moveWindowTo(fx + (tx - fx) * t, fy + (ty - fy) * t);
      if (t < 1 && !state.dragging) setTimeout(frame, FRAME_MS);
      else done();
    };
    setTimeout(frame, FRAME_MS);
  });
}

async function dance(): Promise<void> {
  await move("saut");
  await runPath(PATHS.vague!);
  await move("toupie");
  await move("demi_tour");
  await move("salto");
}

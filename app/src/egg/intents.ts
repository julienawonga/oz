import { getCurrentWindow, currentMonitor } from "@tauri-apps/api/window";
import { invoke } from "@tauri-apps/api/core";
import { applyPose, getZones, glideTo, setZones, type PoseName } from "./motion";
import { say, setThinking, showBubble } from "./ui";

// ── Routeur d'intentions LOCALES (instantanées, sans LLM) ────────────
// Chaque intention : `match` (test rapide sur le texte en minuscules) puis
// `run`. Un `run` qui retourne explicitement `false` décline : on passe à
// l'intention suivante (puis au cerveau si aucune ne prend). L'ordre de la
// table EST la priorité.

interface Intent {
  name: string;
  match: (lower: string) => boolean;
  run: (text: string, lower: string) => boolean | void | Promise<boolean | void>;
}

// ── Zones interdites (locales, persistées) ──
async function windowRect(): Promise<{ pos: { x: number; y: number }; size: { width: number; height: number } }> {
  const win = getCurrentWindow();
  const [pos, size] = await Promise.all([win.outerPosition(), win.outerSize()]);
  return { pos, size };
}

// ── Pose : "assis-toi", "debout"... ──
const POSE_COMMANDS: Array<[RegExp, PoseName, string]> = [
  [/\b(assis|assois[- ]?toi|s'asseoir|s'assoire|installe[- ]toi)\b/, "sit", "Je m'installe confortablement 🪺"],
  [/\b(debout|leve|releve)\b/, "idle", "Me voilà debout ! 🧍"],
];

// ── Déplacement : "va à gauche", "monte", "va au milieu" ──
async function runMove(lower: string): Promise<boolean> {
  // Extrait la géométrie actuelle
  const win = getCurrentWindow();
  const [mon, size] = await Promise.all([currentMonitor(), win.outerSize()]);
  const monX = mon?.position?.x ?? 0;
  const monY = mon?.position?.y ?? 0;
  const monW = mon?.size?.width ?? 1920;
  const monH = mon?.size?.height ?? 1080;
  const maxX = monX + monW - size.width - 10;
  const maxY = monY + monH - size.height - 10;

  let tx: number | undefined;
  let ty: number | undefined;

  if (/(gauche|left)/.test(lower)) tx = monX + 10;
  else if (/(droite|right)/.test(lower)) tx = maxX;
  else if (/(milieu|centre|center)/.test(lower)) tx = monX + (monW - size.width) / 2;

  if (/(haut|monte|monter|up)/.test(lower)) ty = monY + 10;
  else if (/(bas|descend|descendre|down)/.test(lower)) ty = maxY;

  if (tx === undefined && ty === undefined) return false;

  await glideTo(tx, ty);
  say("J'y glisse ! 🥚", 2500);
  return true;
}

// ── Vision : "regarde mon éditeur", "que vois-tu ?"... ──
async function runLook(lower: string): Promise<void> {
  setThinking(true);
  try {
    // cible : ce qui suit "regarde" (sinon fenêtre au premier plan)
    let target = "";
    const m = lower.match(/(?:regarde|regarder|décris|decrit)\s+(?:la fenêtre\s+|le programme\s+|l'application\s+)?(?:mon\s+|ma\s+|le\s+|la\s+|les\s+)?(.+)/);
    if (m && m[1]) {
      target = m[1].replace(/\b(écran|ecran|fenêtre|fenetre)\b/g, "").trim();
    }
    if (/\b(écran|ecran)\b/.test(lower) && !target) target = ""; // tout l'écran → premier plan

    showBubble("Je regarde... 👁️");
    const desc = await invoke<string>("look_at", {
      target,
      question: "Décris ce que tu vois sur cette capture d'écran.",
    });
    say(desc.slice(0, 200), Math.min(14000, 4000 + desc.length * 50));
    void invoke("speak", { text: desc.slice(0, 300) }).catch(() => {});
  } catch (err) {
    say(String(err).slice(0, 120), 7000);
  } finally {
    setThinking(false);
  }
}

const INTENTS: Intent[] = [
  {
    // L'ancienne « maison » vit désormais dans le panneau d'administration
    name: "house",
    match: (l) => /(maison|ton home|ouvre ta maison|ferme ta maison)/.test(l),
    run: () => {
      say("Ma mémoire et mon activité sont dans le panneau d'administration.", 4500);
    },
  },
  {
    name: "zone-forbid",
    match: (l) => /(zone interdite|interdis cette zone|interdit ici|zone defensif|zone de défense)/.test(l),
    run: async () => {
      const { pos, size } = await windowRect();
      const zones = getZones();
      zones.push({ x: pos.x, y: pos.y, w: size.width, h: size.height, label: "interdit" });
      await invoke("set_zones", { zones });
      say("Compris, je ne mettrai plus mes pattes ici 🚫", 4500);
    },
  },
  {
    name: "zone-allow",
    match: (l) => /(autorise cette zone|supprime la zone|permis ici|enlève la zone|enleve la zone)/.test(l),
    run: async () => {
      const { pos, size } = await windowRect();
      const before = getZones().length;
      // garde les zones qui ne chevauchent PAS la position actuelle
      setZones(getZones().filter((z) =>
        !(pos.x < z.x + z.w && pos.x + size.width > z.x && pos.y < z.y + z.h && pos.y + size.height > z.y)
      ));
      const zones = getZones();
      await invoke("set_zones", { zones });
      say(before !== zones.length ? "D'accord, je peux revenir ici 😌" : "Il n'y a pas de zone interdite ici 🤔", 4500);
    },
  },
  {
    name: "zone-list",
    match: (l) => /(montre les zones|mes zones|combien de zones)/.test(l),
    run: () => {
      const zones = getZones();
      say(zones.length === 0 ? "Aucune zone interdite pour l'instant 🌍" : `J'ai ${zones.length} zone(s) interdite(s) en mémoire 🚫`, 4500);
    },
  },
  {
    name: "look",
    // pas de conflit avec les actions (ferme / couche / allonge)
    match: (l) =>
      /\b(regarde|regarder|que vois|qu'est[- ]ce que tu vois|décris|decrit)\b/.test(l) &&
      !/\b(ferme|couche|allonge)\b/.test(l),
    run: (_text, lower) => runLook(lower),
  },
  {
    name: "move",
    match: (l) => /(va|vas|vas-y|glisse|deplace[- ]toi|déplace[- ]toi|bouge|va vers)/.test(l),
    run: (_text, lower) => runMove(lower), // false si aucune direction → on laisse passer
  },
  {
    name: "pose",
    match: (l) => POSE_COMMANDS.some(([re]) => re.test(l)),
    run: (_text, lower) => {
      const cmd = POSE_COMMANDS.find(([re]) => re.test(lower));
      if (!cmd) return false;
      applyPose(cmd[1]);
      say(cmd[2], 4000);
    },
  },
];

/** Tente les intentions locales dans l'ordre ; true si l'une a pris la main. */
export async function handleLocalIntent(text: string): Promise<boolean> {
  const lower = text.toLowerCase();
  for (const intent of INTENTS) {
    if (!intent.match(lower)) continue;
    const res = await intent.run(text, lower);
    if (res !== false) return true;
  }
  return false;
}

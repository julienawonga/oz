import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { unsilenceTalk } from "./audio";
import { startMicStream, stopMicStream, toggleListening } from "./mic";
import { state } from "./state";
import { robot, say } from "./ui";

// ── Oz engage la conversation (décidé par src-tauri/src/initiative.rs) ──
// « appeler »       : il appelle Julien par son prénom, jusqu'à trois fois,
//                     micro ouvert ; dès que Julien répond, il enchaîne.
// « lever_la_main » : signe discret sur l'œuf + petit son ; un clic sur
//                     l'œuf et il parle. Ignoré, il laisse tomber.
// L'issue (répondu, ignoré, occupé) est rendue au backend, qui s'en sert
// pour se faire plus discret quand Julien ne répond pas.

interface Initiative {
  maniere: "appeler" | "lever_la_main";
  sujet: string;
  premiere_phrase: string;
}

const NOTE = "[Note interne, ce n'est pas Julien qui parle]";
const CALLS = 3;
const ANSWER_WAIT_MS = 5000; // silence laissé à Julien après chaque appel
const RAISED_HAND_MS = 90_000;

let current: Initiative | null = null;
let heard = false;
let raisedTimer = 0;

void listen<Initiative>("oz://initiative", (e) => void start(e.payload));
// Julien parle : c'est une réponse à l'appel en cours
void listen<string>("oz://voice-user-text", () => {
  if (current?.maniere === "appeler") heard = true;
});

async function start(init: Initiative): Promise<void> {
  if (current || state.micRunning || state.ozSpeaking || state.thinking || state.dragging) {
    report("busy");
    return;
  }
  current = init;
  if (init.maniere === "appeler") await callOut(init);
  else raiseHand();
}

function report(outcome: "answered" | "ignored" | "busy"): void {
  void invoke("initiative_result", { outcome }).catch(() => {});
}

function finish(outcome: "answered" | "ignored" | "busy"): void {
  current = null;
  state.holdMic = false;
  clearTimeout(raisedTimer);
  robot.classList.remove("hand-raised");
  report(outcome);
}

const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

/** Attend que la condition soit vraie, au plus `ms`. */
async function waitFor(cond: () => boolean, ms: number): Promise<boolean> {
  const end = performance.now() + ms;
  while (performance.now() < end) {
    if (cond()) return true;
    await sleep(100);
  }
  return cond();
}

/** Envoie une consigne à la conversation vocale ; faux si elle échoue. */
async function prompt(text: string): Promise<boolean> {
  try {
    await invoke("live_prompt", { text });
    return true;
  } catch (err) {
    say(String(err).slice(0, 110), 6000);
    return false;
  }
}

// ── Appeler Julien par son prénom ──
async function callOut(init: Initiative): Promise<void> {
  heard = false;
  state.holdMic = true; // pas de coupure sur le silence entre deux appels
  await startMicStream();
  if (!state.micRunning) return finish("busy");
  unsilenceTalk();

  const calls = [
    `${NOTE} Tu prends l'initiative de parler à Julien. Ton sujet : ${init.sujet} Pour l'instant, appelle-le seulement par son prénom pour attirer son attention, comme un ami dans la pièce (par exemple « Julien ? »), rien d'autre. Quand il te répondra, enchaîne naturellement, en commençant par quelque chose comme : « ${init.premiere_phrase} »`,
    `${NOTE} Il n'a pas répondu. Rappelle-le, seulement son prénom, un peu plus fort ou autrement.`,
    `${NOTE} Toujours rien. Dernier appel, seulement son prénom.`,
  ];
  for (const text of calls.slice(0, CALLS)) {
    if (heard || current !== init) break;
    if (!(await prompt(text))) {
      await stopMicStream();
      return finish("busy");
    }
    // qu'il ait fini de parler, puis un temps pour répondre
    await waitFor(() => state.ozSpeaking || heard, 6000);
    await waitFor(() => !state.ozSpeaking, 10_000);
    await waitFor(() => heard || current !== init, ANSWER_WAIT_MS);
  }
  if (current !== init) return; // déjà clos (clic, Échap)
  if (heard) return finish("answered"); // la conversation continue, micro ouvert
  await stopMicStream();
  finish("ignored");
}

// ── Lever la main ──
function raiseHand(): void {
  robot.classList.add("hand-raised");
  chime();
  raisedTimer = window.setTimeout(() => finish("ignored"), RAISED_HAND_MS);
}

async function answerRaisedHand(init: Initiative): Promise<void> {
  clearTimeout(raisedTimer);
  robot.classList.remove("hand-raised");
  await startMicStream();
  unsilenceTalk();
  const ok = await prompt(
    `${NOTE} Julien a vu que tu voulais lui parler et il t'écoute. Parle-lui maintenant. Ton sujet : ${init.sujet} Commence par quelque chose comme : « ${init.premiere_phrase} »`
  );
  finish(ok ? "answered" : "busy");
}

/** Clic sur l'œuf : répond à Oz s'il a la main levée ou appelle, sinon micro. */
export function onEggClick(): void {
  if (current?.maniere === "lever_la_main") {
    void answerRaisedHand(current);
    return;
  }
  if (current?.maniere === "appeler") {
    heard = true; // « oui ? » par le geste : on attend ce que Julien va dire
    return;
  }
  toggleListening();
}

/** Échap : Oz laisse tomber ce qu'il voulait dire. */
export function cancelInitiative(): void {
  if (!current) return;
  if (current.maniere === "appeler") void stopMicStream();
  finish("ignored");
}

/** Deux notes douces : « hé ». */
function chime(): void {
  try {
    const ctx = new AudioContext();
    const now = ctx.currentTime;
    [660, 880].forEach((freq, i) => {
      const osc = ctx.createOscillator();
      const gain = ctx.createGain();
      osc.type = "sine";
      osc.frequency.value = freq;
      const t = now + i * 0.16;
      gain.gain.setValueAtTime(0, t);
      gain.gain.linearRampToValueAtTime(0.12, t + 0.02);
      gain.gain.exponentialRampToValueAtTime(0.001, t + 0.35);
      osc.connect(gain).connect(ctx.destination);
      osc.start(t);
      osc.stop(t + 0.4);
    });
    window.setTimeout(() => void ctx.close(), 1000);
  } catch {
    /* pas de son : le signe visuel suffit */
  }
}

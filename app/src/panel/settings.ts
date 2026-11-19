import { invoke } from "@tauri-apps/api/core";
import { emit } from "@tauri-apps/api/event";
import { content, esc, pageHead, toast } from "./util";

// ── Réglages de comportement ──
interface Prefs {
  show_transcript: boolean;
  initiative: boolean;
  language: string;
  mic_shortcut: string;
}

const LANGUAGES: [string, string][] = [
  ["fr-FR", "Français"],
  ["en-US", "English"],
  ["es-ES", "Español"],
  ["de-DE", "Deutsch"],
  ["it-IT", "Italiano"],
  ["pt-BR", "Português"],
  ["nl-NL", "Nederlands"],
];

/** « Ctrl+Shift+Space » → touches dessinées. */
export function kbd(shortcut: string): string {
  if (!shortcut) return `<span class="kbd">Aucun</span>`;
  const label = (k: string) => (k === "Space" ? "Espace" : k);
  return `<span class="kbd">${shortcut.split("+").map((k) => `<span>${esc(label(k))}</span>`).join("")}</span>`;
}

export async function renderSettings(): Promise<void> {
  let prefs = (await invoke("get_prefs")) as Prefs;

  const row = (title: string, hint: string, control: string, forId = "") =>
    `<${forId ? `label for="${forId}"` : "div"} class="setting">
       <span class="setting-text"><strong>${title}</strong><span class="hint">${hint}</span></span>
       <span class="setting-control">${control}</span>
     </${forId ? "label" : "div"}>`;
  const sw = (id: string, on: boolean) => `<input id="${id}" class="switch" type="checkbox" role="switch"${on ? " checked" : ""}>`;

  content.innerHTML =
    pageHead("Réglages", "Comment Oz parle, écoute et se comporte. Chaque changement est appliqué tout de suite.") +
    `<section class="panel"><div class="panel-head"><h3>Voix</h3></div>` +
    row("Langue", "Celle qu'il parle, écrit et comprend. La conversation en cours repart de zéro.",
      `<select id="s-language" aria-label="Langue">${LANGUAGES.map(([code, name]) =>
        `<option value="${code}"${code === prefs.language ? " selected" : ""}>${name}</option>`).join("")}</select>`) +
    row("Raccourci du micro", "Partout, même quand une autre fenêtre est au premier plan. Même effet qu'un clic sur l'œuf.",
      `<button id="s-shortcut" class="btn shortcut-field" type="button">${kbd(prefs.mic_shortcut)}</button>
       <button id="s-shortcut-clear" class="btn ghost sm" type="button" title="Retirer le raccourci">Retirer</button>`) +
    `</section>
    <section class="panel"><div class="panel-head"><h3>Affichage</h3></div>` +
    row("Transcription dans la bulle", "Ce que vous vous dites s'affiche au-dessus de l'œuf. Coupée, Oz parle sans texte à l'écran.",
      sw("s-transcript", prefs.show_transcript), "s-transcript") +
    `</section>
    <section class="panel"><div class="panel-head"><h3>Prise de parole</h3></div>` +
    row("Oz peut engager la conversation", "À ton retour, le matin ou après une longue session, il décide s'il a quelque chose à dire : il t'appelle, ou lève la main si tu as l'air occupé. « Pas maintenant » le fait taire un moment.",
      sw("s-initiative", prefs.initiative), "s-initiative") +
    row("Essayer", "Oz cherche tout de suite quelque chose à te dire, sans attendre un signal. Il ne coupe pas une conversation en cours.",
      `<button class="btn" id="s-try" type="button">Essayer maintenant</button>`) +
    `</section>`;

  const el = <T extends HTMLElement>(id: string) => document.getElementById(id) as T;
  const shortcutBtn = el<HTMLButtonElement>("s-shortcut");

  const save = async (patch: Partial<Prefs>, ok: string): Promise<boolean> => {
    const next = { ...prefs, ...patch };
    try {
      await invoke("set_prefs", { prefs: next });
      prefs = next;
      toast(ok);
      void emit("oz://panel-prefs", prefs);
      return true;
    } catch (err) {
      toast(String(err), false);
      return false;
    }
  };

  el<HTMLInputElement>("s-transcript").addEventListener("change", (e) =>
    void save({ show_transcript: (e.target as HTMLInputElement).checked }, "Réglage enregistré"));
  el<HTMLInputElement>("s-initiative").addEventListener("change", (e) =>
    void save({ initiative: (e.target as HTMLInputElement).checked }, "Réglage enregistré"));
  el<HTMLSelectElement>("s-language").addEventListener("change", (e) =>
    void save({ language: (e.target as HTMLSelectElement).value }, "Langue enregistrée"));

  // ── capture du raccourci : clic, puis la combinaison ; Échap annule ──
  let recording = false;
  const stopRecording = () => {
    recording = false;
    shortcutBtn.classList.remove("recording");
    shortcutBtn.innerHTML = kbd(prefs.mic_shortcut);
    window.removeEventListener("keydown", onKey, true);
  };
  const onKey = (e: KeyboardEvent) => {
    e.preventDefault();
    e.stopPropagation();
    if (e.code === "Escape") return stopRecording();
    const key = keyName(e.code);
    if (!key) return; // modificateur seul : on attend la touche
    const mods = [e.ctrlKey && "Ctrl", e.altKey && "Alt", e.shiftKey && "Shift", e.metaKey && "Super"].filter(Boolean);
    if (mods.length === 0 && !/^F\d+$/.test(key)) {
      toast("Ajoute au moins Ctrl, Alt ou Shift", false);
      return;
    }
    const combo = [...mods, key].join("+");
    void save({ mic_shortcut: combo }, "Raccourci enregistré").then(stopRecording);
  };
  shortcutBtn.addEventListener("click", () => {
    if (recording) return stopRecording();
    recording = true;
    shortcutBtn.classList.add("recording");
    shortcutBtn.textContent = "Appuie sur les touches…";
    window.addEventListener("keydown", onKey, true);
  });
  shortcutBtn.addEventListener("blur", () => recording && stopRecording());
  el("s-shortcut-clear").addEventListener("click", () => {
    void save({ mic_shortcut: "" }, "Raccourci retiré").then(stopRecording);
  });

  el("s-try").addEventListener("click", async (e) => {
    const btn = e.currentTarget as HTMLButtonElement;
    btn.disabled = true;
    btn.textContent = "Oz réfléchit…";
    try {
      await invoke("initiative_try");
    } catch (err) {
      toast(String(err), false);
    } finally {
      btn.disabled = false;
      btn.textContent = "Essayer maintenant";
    }
  });
}

/** Code de touche du navigateur → nom compris par le raccourci global. */
function keyName(code: string): string | null {
  if (/^Key[A-Z]$/.test(code)) return code.slice(3);
  if (/^Digit\d$/.test(code)) return code.slice(5);
  if (/^F\d{1,2}$/.test(code)) return code;
  const named: Record<string, string> = {
    Space: "Space", Enter: "Enter", Backquote: "Backquote", Minus: "Minus", Equal: "Equal",
    BracketLeft: "BracketLeft", BracketRight: "BracketRight", Semicolon: "Semicolon",
    Quote: "Quote", Comma: "Comma", Period: "Period", Slash: "Slash", Backslash: "Backslash",
    Insert: "Insert", Home: "Home", End: "End", PageUp: "PageUp", PageDown: "PageDown",
    Pause: "Pause", ArrowUp: "ArrowUp", ArrowDown: "ArrowDown", ArrowLeft: "ArrowLeft", ArrowRight: "ArrowRight",
  };
  return named[code] ?? null;
}

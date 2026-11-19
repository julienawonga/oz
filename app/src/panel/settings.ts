import { invoke } from "@tauri-apps/api/core";
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

export async function renderSettings(): Promise<void> {
  let prefs = (await invoke("get_prefs")) as Prefs;
  const toggle = (id: string, label: string, hint: string, on: boolean) =>
    `<label class="toggle" for="${id}">
       <input id="${id}" type="checkbox"${on ? " checked" : ""}>
       <span class="toggle-text"><strong>${label}</strong><span class="hint">${hint}</span></span>
     </label>`;

  content.innerHTML =
    pageHead("Réglages", "Comportement d'Oz à l'écran. Pris en compte immédiatement.") +
    `<section class="panel"><div class="panel-head"><h3>Voix et langue</h3></div>
      <div class="field"><label for="s-language">Langue d'Oz</label>
        <select id="s-language">${LANGUAGES.map(([code, name]) =>
          `<option value="${code}"${code === prefs.language ? " selected" : ""}>${name}</option>`).join("")}
        </select>
        <div class="hint">Langue dans laquelle il parle, écrit et comprend. La conversation en cours repart de zéro.</div></div>
      <div class="field"><label for="s-shortcut">Raccourci du micro</label>
        <div class="with-action">
          <input id="s-shortcut" type="text" readonly value="${esc(prefs.mic_shortcut)}" placeholder="Aucun">
          <button class="btn" id="s-shortcut-clear" type="button">Aucun</button>
        </div>
        <div class="hint">Clique dans le champ puis appuie sur la combinaison voulue. Fonctionne partout, même quand Oz n'a pas le focus : même effet qu'un clic sur l'œuf.</div></div>
    </section>
    <section class="panel"><div class="panel-head"><h3>Affichage</h3></div>` +
    toggle("s-transcript", "Transcription dans la bulle",
      "Affiche ce que vous vous dites pendant la conversation vocale. Coupée, Oz parle sans texte à l'écran.",
      prefs.show_transcript) +
    `</section>
    <section class="panel"><div class="panel-head"><h3>Prise de parole</h3></div>` +
    toggle("s-initiative", "Oz peut engager la conversation",
      "À ton retour, le matin ou après une longue session, il décide s'il a quelque chose à te dire : il t'appelle par ton prénom, ou lève la main si tu as l'air occupé. Dis-lui « pas maintenant » pour qu'il se taise un moment.",
      prefs.initiative) +
    `<div class="form-actions"><button class="btn" id="s-try" type="button">Essayer maintenant</button></div>
    </section>`;

  const el = <T extends HTMLElement>(id: string) => document.getElementById(id) as T;
  const shortcutInput = el<HTMLInputElement>("s-shortcut");

  const save = async (patch: Partial<Prefs>, ok: string) => {
    const next = { ...prefs, ...patch };
    try {
      await invoke("set_prefs", { prefs: next });
      prefs = next;
      toast(ok);
    } catch (err) {
      toast(String(err), false);
      shortcutInput.value = prefs.mic_shortcut; // l'ancien reste actif
    }
  };

  el<HTMLInputElement>("s-transcript").addEventListener("change", (e) =>
    void save({ show_transcript: (e.target as HTMLInputElement).checked }, "Réglage enregistré"));
  el<HTMLInputElement>("s-initiative").addEventListener("change", (e) =>
    void save({ initiative: (e.target as HTMLInputElement).checked }, "Réglage enregistré"));
  el<HTMLSelectElement>("s-language").addEventListener("change", (e) =>
    void save({ language: (e.target as HTMLSelectElement).value }, "Langue enregistrée"));

  // capture du raccourci : modificateurs + une touche
  shortcutInput.addEventListener("focus", () => (shortcutInput.placeholder = "Appuie sur la combinaison…"));
  shortcutInput.addEventListener("blur", () => (shortcutInput.placeholder = "Aucun"));
  shortcutInput.addEventListener("keydown", (e) => {
    e.preventDefault();
    const key = keyName(e.code);
    if (!key) return; // modificateur seul : on attend la touche
    const mods = [e.ctrlKey && "Ctrl", e.altKey && "Alt", e.shiftKey && "Shift", e.metaKey && "Super"].filter(Boolean);
    if (mods.length === 0 && !/^F\d+$/.test(key)) {
      toast("Ajoute au moins Ctrl, Alt ou Shift", false);
      return;
    }
    const combo = [...mods, key].join("+");
    shortcutInput.value = combo;
    shortcutInput.blur();
    void save({ mic_shortcut: combo }, `Raccourci : ${combo}`);
  });
  el("s-shortcut-clear").addEventListener("click", () => {
    shortcutInput.value = "";
    void save({ mic_shortcut: "" }, "Raccourci retiré");
  });

  el("s-try").addEventListener("click", async (e) => {
    const btn = e.currentTarget as HTMLButtonElement;
    btn.disabled = true;
    try {
      await invoke("initiative_try");
      toast("Oz réfléchit à ce qu'il va te dire");
    } catch (err) {
      toast(String(err), false);
    } finally {
      btn.disabled = false;
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

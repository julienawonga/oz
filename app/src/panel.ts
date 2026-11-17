import { invoke, Channel } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

// ── Administration de Nid ────────────────────────────────────────────
// Conversation, permissions, zones, modèles, journal d'activité.
// Toutes les données viennent des mêmes commandes Rust que l'œuf.

const content = document.getElementById("content")!;

let tab = "chat";
let busy = false;
let journalFilter = "all";

interface Msg {
  role: "user" | "nid";
  text: string;
}
const chatLog: Msg[] = [];

document.querySelectorAll<HTMLButtonElement>("#tabs button").forEach((btn) => {
  btn.addEventListener("click", () => {
    tab = btn.dataset.tab!;
    document.querySelectorAll("#tabs button").forEach((b) => b.classList.remove("active"));
    btn.classList.add("active");
    render();
  });
});

// ── Utilitaires ──
const ICONS: Record<string, string> = {
  send: '<path d="M22 2 11 13M22 2l-7 20-4-9-9-4 20-7z"/>',
  trash: '<path d="M3 6h18M8 6V4h8v2m-9 0 1 14h8l1-14"/>',
  eye: '<path d="M2 12s4-7 10-7 10 7 10 7-4 7-10 7S2 12 2 12z"/><circle cx="12" cy="12" r="3"/>',
  save: '<path d="M5 3h11l3 3v15H5z"/><path d="M8 3v6h8V3M8 21v-7h8v7"/>',
};
function icon(name: string): string {
  return `<svg class="icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">${ICONS[name] ?? ""}</svg>`;
}

function fmtTs(ts: number): string {
  const d = new Date(ts * 1000);
  return d.toLocaleDateString("fr", { day: "2-digit", month: "short" }) + " " +
    d.toLocaleTimeString("fr", { hour: "2-digit", minute: "2-digit" });
}

function esc(s: unknown): string {
  return String(s ?? "").replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;");
}

function toast(text: string, ok = true): void {
  const el = document.createElement("div");
  el.className = `toast ${ok ? "ok" : "err"}`;
  el.textContent = text;
  document.body.appendChild(el);
  setTimeout(() => el.classList.add("out"), 2400);
  setTimeout(() => el.remove(), 2800);
}

function pageHead(title: string, desc: string): string {
  return `<div class="page-head"><h2>${title}</h2><p>${desc}</p></div>`;
}

// Les échanges faits depuis l'œuf apparaissent aussi ici
void listen<Msg>("nid://chat-line", (e) => {
  chatLog.push(e.payload);
  if (tab !== "chat") return;
  const log = document.getElementById("chat-log");
  if (log) {
    log.querySelector(".empty")?.remove();
    appendMsg(log, e.payload.role, e.payload.text);
  }
});

function appendMsg(log: HTMLElement, role: string, text: string): HTMLElement {
  const div = document.createElement("div");
  div.className = `msg ${role}`;
  div.textContent = text;
  log.appendChild(div);
  div.scrollIntoView({ block: "end" });
  return div;
}

function render(): void {
  void renderAsync().catch((err) => {
    content.innerHTML = `<div class="empty"><strong>Impossible de charger cette section</strong>${esc(err)}</div>`;
  });
}

async function renderAsync(): Promise<void> {
  if (tab === "chat") return renderChat();
  if (tab === "perms") return renderPerms();
  if (tab === "zones") return renderZones();
  if (tab === "brain") return renderBrain();
  if (tab === "memory") return renderMemory();
  if (tab === "journal") return renderJournal();
}

// ── Conversation ──
function renderChat(): void {
  content.innerHTML = `
    <div class="chat-wrap">
      <div id="chat-log"></div>
      <form id="chat-form">
        <input id="panel-chat-input" placeholder="Écrire à Nid" autocomplete="off" aria-label="Message" />
        <button class="btn primary" type="submit">${icon("send")}Envoyer</button>
      </form>
    </div>`;
  const log = document.getElementById("chat-log")!;
  if (chatLog.length === 0) {
    log.innerHTML = `<div class="empty"><strong>Aucun message</strong>Les échanges avec Nid, depuis ici ou depuis l'œuf, apparaissent dans cette vue.</div>`;
  }
  for (const m of chatLog) appendMsg(log, m.role, m.text);

  document.getElementById("chat-form")!.addEventListener("submit", (e) => {
    e.preventDefault();
    const input = document.getElementById("panel-chat-input") as HTMLInputElement;
    const text = input.value.trim();
    if (!text || busy) return;
    input.value = "";
    chatLog.push({ role: "user", text });
    void sendToNid(text);
  });
  (document.getElementById("panel-chat-input") as HTMLInputElement).focus();
}

async function sendToNid(text: string): Promise<void> {
  busy = true;
  render();
  const form = document.getElementById("chat-form")!;
  form.classList.add("busy");
  const log = document.getElementById("chat-log")!;
  const streaming = appendMsg(log, "nid streaming", "…");

  const onChunk = new Channel<string>();
  let acc = "";
  onChunk.onmessage = (tok) => {
    acc += tok;
    streaming.textContent = acc;
    streaming.scrollIntoView({ block: "end" });
  };
  try {
    const reply = await invoke<string>("chat", { userMessage: text, onChunk });
    streaming.classList.remove("streaming");
    streaming.textContent = reply;
    chatLog.push({ role: "nid", text: reply });
  } catch (err) {
    streaming.classList.remove("streaming");
    streaming.classList.add("error");
    streaming.textContent = String(err);
  } finally {
    busy = false;
    form.classList.remove("busy");
    (document.getElementById("panel-chat-input") as HTMLInputElement | null)?.focus();
  }
}

// ── Permissions ──
const LEVELS: Array<[string, string]> = [
  ["sanctuary", "Sanctuaire"],
  ["none", "Aucun accès"],
  ["look", "Lecture"],
  ["act", "Actions"],
  ["trust", "Confiance"],
];

async function renderPerms(): Promise<void> {
  const perms = (await invoke("get_permissions")) as Record<string, string>;
  const groups = new Map<string, Array<[string, string, string]>>();
  for (const [key, level] of Object.entries(perms)) {
    const [proc, title] = key.split("::");
    if (!groups.has(proc)) groups.set(proc, []);
    groups.get(proc)!.push([key, title ?? "", level]);
  }

  let html = pageHead(
    "Permissions",
    "Niveau d'accès accordé à Nid pour chaque fenêtre. Les règles sont appliquées par le broker ; un sanctuaire n'est jamais visible, même dans les listes."
  );
  if (groups.size === 0) {
    html += `<div class="panel"><div class="empty"><strong>Aucune permission enregistrée</strong>Elles apparaissent quand Nid rencontre de nouvelles fenêtres.</div></div>`;
  }
  for (const [proc, rows] of [...groups.entries()].sort()) {
    html += `<section class="panel"><div class="panel-head"><h3>${esc(proc)}</h3><span class="badge">${rows.length}</span></div>`;
    for (const [key, title, level] of rows) {
      html += `<div class="list-row"><div class="main"><div class="title">${title === "*" ? "Toutes les fenêtres de l'application" : esc(title) || "(sans titre)"}</div></div>
        <select data-key="${esc(key)}" aria-label="Niveau d'accès">` +
        LEVELS.map(([v, label]) => `<option value="${v}"${v === level ? " selected" : ""}>${label}</option>`).join("") +
        `</select></div>`;
    }
    html += `</section>`;
  }
  content.innerHTML = html;

  content.querySelectorAll("select").forEach((sel) => {
    sel.addEventListener("change", async (ev) => {
      const s = ev.currentTarget as HTMLSelectElement;
      await invoke("set_permission", { key: s.dataset.key, level: s.value });
      toast("Permission enregistrée");
    });
  });
}

// ── Zones ──
async function renderZones(): Promise<void> {
  const zones = (await invoke("get_zones")) as Array<{ x: number; y: number; w: number; h: number; label: string }>;
  let html = pageHead(
    "Zones restreintes",
    "Zones de l'écran où Nid ne se rend pas. Pour en créer une, placez Nid à l'endroit voulu puis écrivez « zone interdite »."
  );
  html += `<section class="panel"><div class="panel-head"><h3>Zones définies</h3><span class="badge">${zones.length}</span></div>`;
  if (zones.length === 0) {
    html += `<div class="empty"><strong>Aucune zone</strong>Nid peut se déplacer librement sur l'écran.</div>`;
  } else {
    zones.forEach((z, i) => {
      html += `<div class="list-row"><div class="main"><div class="title">Zone ${i + 1}</div>
        <div class="meta">Position ${Math.round(z.x)}, ${Math.round(z.y)} · Taille ${Math.round(z.w)} × ${Math.round(z.h)} px</div></div>
        <button class="btn danger sm" data-i="${i}">${icon("trash")}Supprimer</button></div>`;
    });
  }
  html += `</section>`;
  content.innerHTML = html;

  content.querySelectorAll<HTMLButtonElement>("button[data-i]").forEach((btn) => {
    btn.addEventListener("click", async () => {
      if (!confirm("Supprimer cette zone restreinte ?")) return;
      zones.splice(Number(btn.dataset.i), 1);
      await invoke("set_zones", { zones });
      toast("Zone supprimée");
      void renderZones();
    });
  });
}

// ── Modèles ──
interface BrainCfg {
  provider: string; model: string; url: string;
  vision_model: string; audio_model: string; api_key: string;
}

async function renderBrain(): Promise<void> {
  const cfg = (await invoke("get_brain_config")) as BrainCfg;
  const field = (id: string, label: string, value: string, hint = "") =>
    `<div class="field"><label for="${id}">${label}</label>
     <input id="${id}" type="text" value="${esc(value)}" autocomplete="off" spellcheck="false">
     ${hint ? `<div class="hint">${hint}</div>` : ""}</div>`;

  content.innerHTML =
    pageHead("Modèles", "Fournisseur et modèles utilisés pour la conversation, la vision et la voix. Les changements sont pris en compte au message suivant.") +
    `<section class="panel"><div class="panel-head"><h3>Conversation</h3></div>
      <div class="field"><label for="b-provider">Fournisseur</label>
        <select id="b-provider">
          <option value="gemini"${cfg.provider === "gemini" ? " selected" : ""}>Gemini (cloud)</option>
          <option value="ollama"${cfg.provider === "ollama" ? " selected" : ""}>Ollama (local)</option>
        </select></div>` +
    field("b-model", "Modèle", cfg.model) +
    field("b-url", "URL du serveur Ollama", cfg.url, "Utilisée uniquement avec le fournisseur Ollama.") +
    `</section>
    <section class="panel"><div class="panel-head"><h3>Vision et voix</h3></div>` +
    field("b-vision", "Modèle de vision", cfg.vision_model, "Analyse des captures de fenêtres autorisées.") +
    field("b-audio", "Modèle de voix", cfg.audio_model ?? "", "Exemples : gemini-live, gemini-tts:Kore. Vide pour la voix du système.") +
    `</section>
    <section class="panel"><div class="panel-head"><h3>Authentification</h3></div>
      <div class="field"><label for="b-key">Clé API Gemini</label>
        <div class="with-action">
          <input id="b-key" type="password" value="${esc(cfg.api_key ?? "")}" autocomplete="off" spellcheck="false">
          <button class="btn" id="b-reveal" type="button">${icon("eye")}<span>Afficher</span></button>
        </div>
        <div class="hint">Stockée localement dans la configuration de l'application.</div></div>
      <div class="form-actions"><button class="btn primary" id="b-save" type="button">${icon("save")}Enregistrer</button></div>
    </section>`;

  const val = (id: string) => (document.getElementById(id) as HTMLInputElement).value.trim();
  const keyInput = document.getElementById("b-key") as HTMLInputElement;
  document.getElementById("b-reveal")!.addEventListener("click", (e) => {
    const show = keyInput.type === "password";
    keyInput.type = show ? "text" : "password";
    (e.currentTarget as HTMLElement).querySelector("span")!.textContent = show ? "Masquer" : "Afficher";
  });
  document.getElementById("b-save")!.addEventListener("click", async () => {
    if (val("b-provider") === "gemini" && !val("b-key")) return toast("Une clé API est requise pour Gemini", false);
    if (!val("b-model")) return toast("Le modèle de conversation est requis", false);
    await invoke("set_brain_config", {
      config: {
        provider: val("b-provider"), model: val("b-model"), url: val("b-url"),
        vision_model: val("b-vision"), audio_model: val("b-audio"), api_key: val("b-key"),
      },
    });
    toast("Configuration enregistrée");
  });
}

// ── Mémoire ──
async function renderMemory(): Promise<void> {
  const facts = (await invoke("get_facts")) as Array<{ subject: string; content: string; ts: number }>;
  const places = (await invoke("get_memories")) as Array<{ key: string; note: string; ts: number }>;

  const row = (title: string, text: string, ts: number, attr: string) =>
    `<div class="list-row"><div class="main"><div class="title" style="white-space:normal">${esc(title)}</div>
      <div class="meta" style="white-space:normal">${esc(text)}</div>
      <div class="meta"><time>${fmtTs(ts)}</time></div></div>
      <button class="btn danger sm" ${attr}>${icon("trash")}Oublier</button></div>`;

  let html = pageHead("Mémoire", "Ce que Nid retient : faits durables issus des conversations et notes sur les fenêtres observées. Chaque élément peut être supprimé.");
  html += `<section class="panel"><div class="panel-head"><h3>Faits durables</h3><span class="badge">${facts.length}</span></div>`;
  html += facts.length
    ? facts.map((f) => row(f.subject, f.content, f.ts, `data-ts="${f.ts}"`)).join("")
    : `<div class="empty"><strong>Aucun fait</strong>Ils sont consolidés après quelques conversations.</div>`;
  html += `</section><section class="panel"><div class="panel-head"><h3>Notes sur les fenêtres</h3><span class="badge">${places.length}</span></div>`;
  html += places.length
    ? places.map((m) => row(m.key.split("::")[0], m.note, m.ts, `data-key="${esc(m.key)}"`)).join("")
    : `<div class="empty"><strong>Aucune note</strong>Elles apparaissent quand Nid observe une fenêtre autorisée.</div>`;
  html += `</section>`;
  content.innerHTML = html;

  content.querySelectorAll<HTMLButtonElement>("button[data-ts], button[data-key]").forEach((btn) =>
    btn.addEventListener("click", async () => {
      if (btn.dataset.key) await invoke("clear_memory", { key: btn.dataset.key });
      else await invoke("forget_fact", { ts: Number(btn.dataset.ts) });
      toast("Élément supprimé");
      void renderMemory();
    })
  );
}

// ── Activité ──
const KINDS: Record<string, [string, string]> = {
  action: ["Action", "ok"],
  vision: ["Vision", "info"],
  door: ["Permission", "warn"],
  voice: ["Voix", ""],
};

async function renderJournal(): Promise<void> {
  const events = (await invoke("get_journal", { limit: 200 })) as Array<{ ts: number; kind: string; detail: string }>;
  const shown = journalFilter === "all" ? events : events.filter((e) => e.kind === journalFilter);
  const chips = [["all", "Tout"], ...Object.entries(KINDS).map(([k, v]) => [k, v[0]])]
    .map(([k, label]) => `<button class="chip${k === journalFilter ? " active" : ""}" data-f="${k}">${label}</button>`)
    .join("");

  let html = pageHead("Activité", "Historique de ce que Nid a fait, vu et obtenu comme autorisations.") +
    `<div class="filters">${chips}</div><section class="panel">`;
  if (shown.length === 0) {
    html += `<div class="empty"><strong>Aucune activité</strong>Rien à afficher pour ce filtre.</div>`;
  } else {
    for (const e of shown) {
      const [label, tone] = KINDS[e.kind] ?? [e.kind, ""];
      html += `<div class="list-row"><div class="main"><div class="title" style="white-space:normal">${esc(e.detail)}</div>
        <div class="meta"><span class="badge ${tone}">${esc(label)}</span> <time>${fmtTs(e.ts)}</time></div></div></div>`;
    }
  }
  html += `</section>`;
  content.innerHTML = html;

  content.querySelectorAll<HTMLButtonElement>(".chip").forEach((c) =>
    c.addEventListener("click", () => {
      journalFilter = c.dataset.f!;
      void renderJournal();
    })
  );
}

render();

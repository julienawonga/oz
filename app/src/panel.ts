import { invoke, Channel } from "@tauri-apps/api/core";
import { emit } from "@tauri-apps/api/event";

// ── Panneau de contrôle de Nid — l'interface de Julien ───────────────
// Chat complet, zones, permissions, cerveau, journal.
// Toutes les données viennent des mêmes commandes Rust que l'œuf.

const content = document.getElementById("content")!;

let tab = "chat";

// État local du chat
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

function fmtTs(ts: number): string {
  return new Date(ts * 1000).toLocaleDateString("fr") + " " + new Date(ts * 1000).toLocaleTimeString("fr", { hour: "2-digit", minute: "2-digit" });
}

function esc(s: string): string {
  return s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
}

function render(): void {
  void renderAsync();
}

async function renderAsync(): Promise<void> {
  if (tab === "chat") return renderChat();
  if (tab === "zones") return renderZones();
  if (tab === "perms") return renderPerms();
  if (tab === "brain") return renderBrain();
  if (tab === "journal") return renderJournal();
}

function renderChat(): void {
  content.innerHTML = `
    <div class="card" style="margin-bottom:10px">
      <div class="row">
        <span>🗑️ Barre de texte de l'œuf</span>
        <button id="toggle-chatbar" style="border:none;background:var(--accent);color:#fff;border-radius:8px;padding:5px 12px;cursor:pointer;font-size:12px">Masquer</button>
      </div>
    </div>
    <div id="chat-log"></div>
    <form id="chat-form">
      <input id="panel-chat-input" placeholder="Écris à Nid..." autocomplete="off" />
      <button type="submit">➤</button>
    </form>`;
  const log = document.getElementById("chat-log")!;
  for (const m of chatLog) {
    const div = document.createElement("div");
    div.className = `msg ${m.role === "user" ? "user" : "nid"}`;
    div.textContent = m.text;
    log.appendChild(div);
  }
  log.scrollTop = log.scrollHeight;

  let chatbarVisible = true;
  const toggleBtn = document.getElementById("toggle-chatbar") as HTMLElement | null;
  toggleBtn?.addEventListener("click", () => {
    chatbarVisible = !chatbarVisible;
    toggleBtn.textContent = chatbarVisible ? "Masquer" : "Afficher";
    void emit("nid://chatbar", { visible: chatbarVisible });
  });

  document.getElementById("chat-form")!.addEventListener("submit", (e) => {
    e.preventDefault();
    const input = document.getElementById("panel-chat-input") as HTMLInputElement;
    const text = input.value.trim();
    if (!text) return;
    input.value = "";
    chatLog.push({ role: "user", text });
    void sendToNid(text);
  });
}

async function sendToNid(text: string): Promise<void> {
  render(); // affiche le message user
  const log = document.getElementById("chat-log")!;
  const streaming = document.createElement("div");
  streaming.className = "msg nid streaming";
  streaming.textContent = "…";
  log.appendChild(streaming);
  log.scrollTop = log.scrollHeight;

  const onChunk = new Channel<string>();
  let acc = "";
  onChunk.onmessage = (tok) => {
    acc += tok;
    streaming.textContent = acc;
    log.scrollTop = log.scrollHeight;
  };
  try {
    const reply = await invoke<string>("chat", { userMessage: text, onChunk });
    streaming.classList.remove("streaming");
    streaming.textContent = reply;
    chatLog.push({ role: "nid", text: reply });
  } catch (err) {
    streaming.classList.remove("streaming");
    streaming.textContent = String(err);
  }
}

// ── Zones ──
async function renderZones(): Promise<void> {
  const zones = (await invoke("get_zones")) as Array<{ x: number; y: number; w: number; h: number; label: string }>;
  content.innerHTML =
    `<div class="section-title">${zones.length} zone(s) interdite(s) — crée-les en demandant à Nid : "zone interdite" à l'endroit voulu</div>` +
    (zones.length
      ? zones
          .map(
            (z, i) =>
              `<div class="card"><div class="row"><span>🚫 Zone ${i + 1} <span class="muted">(${Math.round(z.x)}, ${Math.round(z.y)}) ${Math.round(z.w)}×${Math.round(z.h)} px</span></span>` +
              `<button class="danger" data-i="${i}">Supprimer</button></div></div>`
          )
          .join("")
      : `<div class="empty">Aucune zone interdite 🌍</div>`);

  content.querySelectorAll("button.danger").forEach((btn) => {
    btn.addEventListener("click", async () => {
      const i = Number((btn as HTMLElement).dataset.i);
      zones.splice(i, 1);
      await invoke("set_zones", { zones });
      void renderZones();
    });
  });
}

// ── Permissions (la porte) ──
async function renderPerms(): Promise<void> {
  const perms = (await invoke("get_permissions")) as Record<string, string>;
  const entries = Object.entries(perms);
  const levels = ["sanctuary", "none", "look", "act", "trust"];
  const labels: Record<string, string> = {
    sanctuary: "🔒 Sanctuaire",
    none: "🚫 Rien",
    look: "👀 Regarder",
    act: "🤝 Agir",
    trust: "✅ Confiance",
  };
  content.innerHTML =
    `<div class="section-title">Les permissions par fenêtre — le contrat de Nid</div>` +
    (entries.length
      ? entries
          .map(([key, level]) => {
            const [proc, title] = key.split("::");
            return `<div class="card"><b>${esc(proc)}</b> <span class="muted">${esc(title ?? "").slice(0, 50)}</span>` +
              `<div class="row"><select data-key="${esc(key)}">` +
              levels.map((l) => `<option value="${l}"${l === level ? " selected" : ""}>${labels[l]}</option>`).join("") +
              `</select></div></div>`;
          })
          .join("")
      : `<div class="empty">Aucune permission enregistrée 🚪</div>`);

  content.querySelectorAll("select").forEach((sel) => {
    sel.addEventListener("change", async (ev) => {
      const s = ev.currentTarget as HTMLSelectElement;
      await invoke("set_permission", { key: s.dataset.key, level: s.value });
    });
  });
}

// ── Cerveau (frigo) ──
async function renderBrain(): Promise<void> {
  const cfg = (await invoke("get_brain_config")) as { provider: string; model: string; url: string; vision_model: string; audio_model: string; api_key: string };
  content.innerHTML =
    `<div class="section-title">Le cerveau de Nid — LLM-agnostic, changement instantané</div>` +
    `<div class="form">` +
    `<label>Provider<input id="b-provider" value="${esc(cfg.provider)}"></label>` +
    `<label>Modèle de conversation<input id="b-model" value="${esc(cfg.model)}"></label>` +
    `<label>URL du provider<input id="b-url" value="${esc(cfg.url)}"></label>` +
    `<label>Modèle vision<input id="b-vision" value="${esc(cfg.vision_model)}"></label>` +
    `<button id="b-save">💾 Enregistrer</button>` +
    `</div>` +
    `<div class="card" style="margin-top:10px"><span class="muted">💡 Modèles Ollama suggérés : lfm3:latest (rapide), lfm2.5:latest (plus de connaissances), moondream:latest (vision)</span></div>`;
  document.getElementById("b-save")!.addEventListener("click", async () => {
    await invoke("set_brain_config", {
      config: {
        provider: (document.getElementById("b-provider") as HTMLInputElement).value,
        model: (document.getElementById("b-model") as HTMLInputElement).value,
        url: (document.getElementById("b-url") as HTMLInputElement).value,
        vision_model: (document.getElementById("b-vision") as HTMLInputElement).value,
      },
    });
    alert("Cerveau mis à jour ✅ — Nid l'utilise dès le prochain message.");
  });
}

// ── Journal ──
async function renderJournal(): Promise<void> {
  const events = (await invoke("get_journal", { limit: 100 })) as Array<{ ts: number; kind: string; detail: string }>;
  const icons: Record<string, string> = { action: "🖐️", vision: "👁️", door: "🚪" };
  content.innerHTML =
    `<div class="section-title">Les quittances — tout ce que Nid a fait, vu, et les permissions accordées</div>` +
    (events.length
      ? events
          .map((e) => `<div class="card">${icons[e.kind] ?? "•"} <b>${e.kind}</b> ${esc(e.detail)}<div class="muted">${fmtTs(e.ts)}</div></div>`)
          .join("")
      : `<div class="empty">Journal vide 🌙</div>`);
}

render();

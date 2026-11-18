import { listen } from "@tauri-apps/api/event";
import { renderBrain } from "./panel/brain";
import { renderChat } from "./panel/chat";
import { renderJournal } from "./panel/journal";
import { renderMemory } from "./panel/memory";
import { renderPerms } from "./panel/perms";
import { content, esc, view } from "./panel/util";
import { renderZones } from "./panel/zones";

// ── Administration d'Oz ────────────────────────────────────────────
// Conversation, permissions, zones, modèles, mémoire, journal d'activité.
// Chaque onglet vit dans src/panel/<onglet>.ts ; ce fichier ne fait que
// le routage. Toutes les données viennent des mêmes commandes Rust que l'œuf.

document.querySelectorAll<HTMLButtonElement>("#tabs button").forEach((btn) => {
  btn.addEventListener("click", () => {
    view.tab = btn.dataset.tab!;
    document.querySelectorAll("#tabs button").forEach((b) => b.classList.remove("active"));
    btn.classList.add("active");
    render();
  });
});

function render(): void {
  void renderAsync().catch((err) => {
    content.innerHTML = `<div class="empty"><strong>Impossible de charger cette section</strong>${esc(err)}</div>`;
  });
}

async function renderAsync(): Promise<void> {
  if (view.tab === "chat") return renderChat();
  if (view.tab === "perms") return renderPerms();
  if (view.tab === "zones") return renderZones();
  if (view.tab === "brain") return renderBrain();
  if (view.tab === "memory") return renderMemory();
  if (view.tab === "journal") return renderJournal();
}

render();

// ── Rafraîchissement automatique ──
// Le panneau reste ouvert en arrière-plan : sans ça, il montrait l'état du
// démarrage. On relit les onglets « vivants » quand il reprend le focus et
// quand Oz rencontre une nouvelle fenêtre. Les onglets avec saisie (chat,
// modèles) ne sont pas redessinés pour ne pas effacer ce qu'on tape.
const LIVE_TABS = new Set(["perms", "zones", "memory", "journal"]);
function refreshLive(): void {
  if (LIVE_TABS.has(view.tab) && !document.querySelector("#content select:focus")) render();
}
window.addEventListener("focus", refreshLive);
document.addEventListener("visibilitychange", () => {
  if (!document.hidden) refreshLive();
});
void listen("oz://access-request", refreshLive);
// onglet Permissions affiché : la liste des fenêtres suit l'écran en continu
setInterval(() => {
  if (view.tab === "perms" && !document.hidden && document.hasFocus()) refreshLive();
}, 4000);

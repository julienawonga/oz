import { invoke } from "@tauri-apps/api/core";
import { content, esc, toast, pageHead } from "./util";

// ── Permissions ──
const LEVELS: Array<[string, string]> = [
  ["sanctuary", "Sanctuaire"],
  ["none", "Aucun accès"],
  ["look", "Lecture"],
  ["act", "Actions"],
  ["trust", "Confiance"],
];

interface OpenWindow { title: string; process: string; app: string; label: string }

function levelSelect(key: string, level: string): string {
  return `<select class="level level-${level}" data-key="${esc(key)}" aria-label="Niveau d'accès">` +
    LEVELS.map(([v, label]) => `<option value="${v}"${v === level ? " selected" : ""}>${label}</option>`).join("") +
    `</select>`;
}

export async function renderPerms(): Promise<void> {
  const [perms, open] = await Promise.all([
    invoke("get_permissions") as Promise<Record<string, string>>,
    invoke("list_windows") as Promise<OpenWindow[]>,
  ]);

  // ── Fenêtres ouvertes MAINTENANT (relues à chaque affichage du panneau) ──
  // une ligne par application ; le niveau réglé ici porte sur « processus::* »
  const apps = new Map<string, { app: string; titles: string[] }>();
  for (const w of open) {
    const a = apps.get(w.process) ?? { app: w.app, titles: [] };
    if (w.label) a.titles.push(w.label);
    apps.set(w.process, a);
  }
  let openHtml = `<section class="panel"><div class="panel-head"><h3>Fenêtres ouvertes</h3><span class="badge">${apps.size}</span></div>`;
  if (apps.size === 0) {
    openHtml += `<div class="empty"><strong>Aucune fenêtre visible</strong></div>`;
  }
  for (const [proc, a] of [...apps.entries()].sort((x, y) => x[1].app.localeCompare(y[1].app))) {
    const level = perms[`${proc}::*`] ?? "none";
    openHtml += `<div class="list-row"><div class="main"><div class="title">${esc(a.app)}</div>
      <div class="meta">${esc(a.titles.slice(0, 2).join(" · ").slice(0, 90))}</div></div>${levelSelect(`${proc}::*`, level)}</div>`;
  }
  openHtml += `</section>`;
  const groups = new Map<string, Array<[string, string, string]>>();
  for (const [key, level] of Object.entries(perms)) {
    const [proc, title] = key.split("::");
    if (!groups.has(proc)) groups.set(proc, []);
    groups.get(proc)!.push([key, title ?? "", level]);
  }

  let html = pageHead(
    "Permissions",
    "Niveau d'accès accordé à Oz pour chaque fenêtre. Les règles sont appliquées par le broker ; un sanctuaire n'est jamais visible, même dans les listes."
  ) + openHtml + `<h3 class="section-title">Règles enregistrées</h3>`;
  if (groups.size === 0) {
    html += `<div class="panel"><div class="empty"><strong>Aucune permission enregistrée</strong>Elles apparaissent quand Oz rencontre de nouvelles fenêtres.</div></div>`;
  }
  if (groups.size > 0) {
    html += `<section class="panel">`;
    for (const [proc, rows] of [...groups.entries()].sort()) {
      for (const [key, title, level] of rows) {
        const what = title === "*" ? "Toutes les fenêtres" : esc(title) || "(sans titre)";
        html += `<div class="list-row"><div class="main"><div class="title">${esc(proc.replace(/\.exe$/i, ""))}</div>
          <div class="meta">${what}</div></div>${levelSelect(key, level)}</div>`;
      }
    }
    html += `</section>`;
  }
  content.innerHTML = html;

  content.querySelectorAll("select").forEach((sel) => {
    sel.addEventListener("change", async (ev) => {
      const s = ev.currentTarget as HTMLSelectElement;
      await invoke("set_permission", { key: s.dataset.key, level: s.value });
      toast("Permission enregistrée");
      void renderPerms(); // les deux sections restent cohérentes
    });
  });
}


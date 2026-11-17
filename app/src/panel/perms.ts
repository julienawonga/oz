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

export async function renderPerms(): Promise<void> {
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


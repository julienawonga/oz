import { invoke } from "@tauri-apps/api/core";
import { content, fmtTs, esc, pageHead } from "./util";

let journalFilter = "all";

// ── Activité ──
const KINDS: Record<string, [string, string]> = {
  action: ["Action", "ok"],
  vision: ["Vision", "info"],
  door: ["Permission", "warn"],
  voice: ["Voix", ""],
};

export async function renderJournal(): Promise<void> {
  const events = (await invoke("get_journal", { limit: 200 })) as Array<{ ts: number; kind: string; detail: string }>;
  const shown = journalFilter === "all" ? events : events.filter((e) => e.kind === journalFilter);
  const chips = [["all", "Tout"], ...Object.entries(KINDS).map(([k, v]) => [k, v[0]])]
    .map(([k, label]) => `<button class="chip${k === journalFilter ? " active" : ""}" data-f="${k}">${label}</button>`)
    .join("");

  let html = pageHead("Activité", "Historique de ce qu'Oz a fait, vu et obtenu comme autorisations.") +
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


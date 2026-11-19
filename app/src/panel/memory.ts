import { invoke } from "@tauri-apps/api/core";
import { content, icon, fmtTs, esc, toast, pageHead } from "./util";

// ── Mémoire ──
export async function renderMemory(): Promise<void> {
  const facts = (await invoke("get_facts")) as Array<{ subject: string; content: string; ts: number }>;
  const places = (await invoke("get_memories")) as Array<{ key: string; note: string; ts: number }>;
  // les plus récents en premier
  facts.sort((a, b) => b.ts - a.ts);
  places.sort((a, b) => b.ts - a.ts);

  const row = (title: string, text: string, ts: number, attr: string) =>
    `<div class="list-row"><div class="main"><div class="title" style="white-space:normal">${esc(title)}</div>
      <div class="meta" style="white-space:normal">${esc(text)}</div>
      <div class="meta"><time>${fmtTs(ts)}</time></div></div>
      <button class="btn danger sm" ${attr}>${icon("trash")}Oublier</button></div>`;

  let html = pageHead("Mémoire", "Ce qu'Oz retient : faits durables issus des conversations et notes sur les fenêtres observées. Chaque élément peut être supprimé.");
  html += `<section class="panel"><div class="panel-head"><h3>Faits durables</h3><span class="badge">${facts.length}</span></div>`;
  html += facts.length
    ? facts.map((f, i) => row(f.subject, f.content, f.ts, `data-fact="${i}"`)).join("")
    : `<div class="empty"><strong>Aucun fait</strong>Ils sont consolidés après quelques conversations.</div>`;
  html += `</section><section class="panel"><div class="panel-head"><h3>Notes sur les fenêtres</h3><span class="badge">${places.length}</span></div>`;
  html += places.length
    ? places.map((m) => row(m.key.split("::")[0], m.note, m.ts, `data-key="${esc(m.key)}"`)).join("")
    : `<div class="empty"><strong>Aucune note</strong>Elles apparaissent quand Oz observe une fenêtre autorisée.</div>`;
  html += `</section>`;
  content.innerHTML = html;

  content.querySelectorAll<HTMLButtonElement>("button[data-fact], button[data-key]").forEach((btn) =>
    btn.addEventListener("click", async () => {
      if (btn.dataset.key) await invoke("clear_memory", { key: btn.dataset.key });
      else {
        const f = facts[Number(btn.dataset.fact)];
        await invoke("forget_fact", { ts: f.ts, content: f.content });
      }
      toast("Élément supprimé");
      void renderMemory();
    })
  );
}


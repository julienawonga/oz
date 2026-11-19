import { invoke } from "@tauri-apps/api/core";
import { content, esc, pageHead, toast } from "./util";

// ── Outils : les toolsets d'Oz, à activer ou couper ──
// Chaque groupe d'outils vient d'un module (toolsets.rs). Un groupe coupé
// n'est plus proposé à Gemini, et le code refuse ses outils.

interface ToolsetInfo {
  id: string;
  label: string;
  description: string;
  core: boolean;
  enabled: boolean;
  tools: string[];
}

export async function renderToolsets(): Promise<void> {
  const sets = (await invoke("get_toolsets")) as ToolsetInfo[];
  const on = sets.filter((s) => s.enabled).reduce((n, s) => n + s.tools.length, 0);
  let html = pageHead(
    "Outils",
    "Ce qu'Oz peut faire, rangé par groupes. Coupe un groupe : Oz ne le voit plus et le code refuse ses outils. Le changement compte dès la prochaine phrase."
  );
  html += `<section class="panel"><div class="panel-head"><h3>Groupes d'outils</h3><span class="badge">${on} outil(s) actif(s)</span></div>`;
  html += sets
    .map(
      (s) => `<div class="setting">
        <span class="setting-text"><strong>${esc(s.label)}${s.core ? ' <span class="badge">toujours actif</span>' : ""}</strong>
          <span class="hint">${esc(s.description)}</span>
          <span class="tool-chips">${s.tools.map((t) => `<code>${esc(t)}</code>`).join("")}</span></span>
        <span class="setting-control"><input class="switch" type="checkbox" role="switch" data-ts="${esc(s.id)}"
          aria-label="Activer ${esc(s.label)}"${s.enabled ? " checked" : ""}${s.core ? " disabled" : ""}></span>
      </div>`
    )
    .join("");
  html += `</section>`;
  content.innerHTML = html;

  content.querySelectorAll<HTMLInputElement>("input[data-ts]").forEach((sw) =>
    sw.addEventListener("change", async () => {
      try {
        await invoke("set_toolset_enabled", { id: sw.dataset.ts, enabled: sw.checked });
        toast(sw.checked ? "Groupe activé" : "Groupe coupé");
      } catch (err) {
        toast(String(err), false);
      }
      void renderToolsets();
    })
  );
}

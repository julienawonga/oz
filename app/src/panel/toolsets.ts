import { invoke } from "@tauri-apps/api/core";
import { content, esc, pageHead, toast } from "./util";

// ── Outils : les toolsets d'Oz, à activer ou couper ──
// Chaque groupe d'outils vient d'un module (toolsets.rs). Un groupe coupé
// n'est plus proposé à Gemini, et le code refuse ses outils.

interface Manifest {
  id: string;
  name: string;
  version: string;
  description: string;
  author: string;
  tools: { name: string; description: string; script: string }[];
}
interface ModuleInfo { manifest: Manifest; skills: string[]; changed: string[] }
interface Review { manifest: Manifest; scripts: Record<string, string>; skills: string[] }

/** Module en cours d'examen avant installation (null = aucun). */
let review: Review | null = null;

interface ToolsetInfo {
  id: string;
  label: string;
  description: string;
  core: boolean;
  enabled: boolean;
  tools: string[];
}

export async function renderToolsets(): Promise<void> {
  if (review) return renderReview(review);
  const sets = (await invoke("get_toolsets")) as ToolsetInfo[];
  const modules = (await invoke("get_modules")) as ModuleInfo[];
  const on = sets.filter((s) => s.enabled).reduce((n, s) => n + s.tools.length, 0);
  let html = pageHead(
    "Outils",
    "Ce qu'Oz peut faire, rangé par groupes, et les modules que tu installes. Coupe un groupe : Oz ne le voit plus et le code refuse ses outils. Le changement compte dès la prochaine phrase."
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

  // ── Modules installables ──
  html += `<section class="panel"><div class="panel-head"><h3>Modules</h3><span class="badge">${modules.length}</span></div>
    <div class="field"><label for="mod-source">Installer un module</label>
      <div class="with-action"><input id="mod-source" type="text" placeholder="https://github.com/… ou C:\\chemin\\vers\\le\\module" autocomplete="off" spellcheck="false">
      <button class="btn primary" id="mod-prepare" type="button">Examiner</button></div>
      <div class="hint">Un dossier avec un module.json et ses scripts PowerShell. Rien ne s'exécute avant que tu aies relu son code et cliqué sur Installer.</div></div>`;
  html += modules.length
    ? modules
        .map(
          (m) => `<div class="list-row" style="flex-wrap:wrap"><div class="main">
            <div class="title" style="white-space:normal">${esc(m.manifest.name)} <span class="badge">${esc(m.manifest.version || "?")}</span>
              ${m.changed.length ? '<span class="badge">script modifié : accord redemandé</span>' : ""}</div>
            <div class="meta" style="white-space:normal">${esc(m.manifest.description)}${m.manifest.author ? " · par " + esc(m.manifest.author) : ""}</div>
            <span class="tool-chips">${m.manifest.tools.map((t) => `<code>${esc(t.name)}</code>`).join("")}
              ${m.skills.map((k) => `<code>skill : ${esc(k)}</code>`).join("")}</span></div>
            <button class="btn danger sm" data-uninstall="${esc(m.manifest.id)}" type="button">Désinstaller</button></div>`
        )
        .join("")
    : `<div class="empty"><strong>Aucun module</strong>Essaie le module d'exemple du dépôt : examples\\modules\\window-sizes (réduire, agrandir, restaurer une fenêtre).</div>`;
  html += `</section>`;
  content.innerHTML = html;

  const prepare = async () => {
    const source = (document.getElementById("mod-source") as HTMLInputElement).value.trim();
    if (!source) return;
    const btn = document.getElementById("mod-prepare") as HTMLButtonElement;
    btn.disabled = true;
    btn.textContent = "Récupération…";
    try {
      review = (await invoke("prepare_module", { source })) as Review;
      void renderToolsets();
    } catch (err) {
      toast(String(err), false);
      btn.disabled = false;
      btn.textContent = "Examiner";
    }
  };
  document.getElementById("mod-prepare")!.addEventListener("click", () => void prepare());
  document.getElementById("mod-source")!.addEventListener("keydown", (e) => {
    if ((e as KeyboardEvent).key === "Enter") void prepare();
  });
  content.querySelectorAll<HTMLButtonElement>("button[data-uninstall]").forEach((b) =>
    b.addEventListener("click", async () => {
      if (!confirm(`Désinstaller le module « ${b.dataset.uninstall} » ?`)) return;
      try {
        await invoke("uninstall_module", { id: b.dataset.uninstall });
        toast("Module désinstallé");
      } catch (err) {
        toast(String(err), false);
      }
      void renderToolsets();
    })
  );

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

/** Relecture avant installation : outils, skills et code de chaque script. */
function renderReview(r: Review): void {
  const m = r.manifest;
  content.innerHTML =
    pageHead(`Installer « ${esc(m.name)} » ?`, "Relis ce que le module va pouvoir faire. Ses scripts tourneront sans te redemander, tant qu'ils ne changent pas.") +
    `<section class="panel"><div class="panel-head"><h3>${esc(m.name)}</h3><span class="badge">${esc(m.version || "?")}</span></div>
      <div class="field"><span class="hint">${esc(m.description)}${m.author ? " · par " + esc(m.author) : ""}</span></div>
      ${m.tools.map((t) => `<div class="field"><strong><code>${esc(t.name)}</code></strong><span class="hint">${esc(t.description)} · script ${esc(t.script)}</span></div>`).join("")}
      ${r.skills.length ? `<div class="field"><strong>Skills livrés</strong><span class="hint">${r.skills.map(esc).join(", ")}</span></div>` : ""}
    </section>
    <section class="panel"><div class="panel-head"><h3>Code des scripts</h3></div>
      ${Object.entries(r.scripts).map(([name, code]) => `<div class="field"><strong>${esc(name)}</strong><pre class="tool-code">${esc(code)}</pre></div>`).join("")}
      <div class="form-actions">
        <button class="btn" id="mod-cancel" type="button">Annuler</button>
        <button class="btn primary" id="mod-install" type="button">Installer</button>
      </div>
    </section>`;
  document.getElementById("mod-cancel")!.addEventListener("click", async () => {
    await invoke("cancel_module").catch(() => {});
    review = null;
    void renderToolsets();
  });
  document.getElementById("mod-install")!.addEventListener("click", async () => {
    try {
      await invoke("install_module", { id: m.id });
      toast(`Module « ${m.name} » installé`);
    } catch (err) {
      toast(String(err), false);
    }
    review = null;
    void renderToolsets();
  });
}

import { invoke } from "@tauri-apps/api/core";
import { content, esc, fmtTs, icon, pageHead, toast } from "./util";

// ── Skills ──
// Savoir-faire écrits en texte : Oz voit leur nom et leur description, et
// charge les instructions (outil read_skill) quand la situation s'y prête.

interface Skill {
  id: string;
  name: string;
  description: string;
  instructions: string;
  enabled: boolean;
  ts: number;
  proposed?: boolean;
  reason?: string;
}

interface CustomTool {
  name: string;
  description: string;
  parameters: string;
  code: string;
  status: string;
  approved_code: string;
  ts: number;
  uses: number;
}

const EMPTY: Skill = { id: "", name: "", description: "", instructions: "", enabled: true, ts: 0, proposed: false, reason: "" };

/** Skill en cours d'édition (null = liste). */
let editing: Skill | null = null;

export async function renderSkills(): Promise<void> {
  if (editing) return renderEditor(editing);
  const skills = (await invoke("get_skills")) as Skill[];
  const tools = (await invoke("get_custom_tools")) as CustomTool[];
  let html = pageHead(
    "Skills",
    "Des savoir-faire que tu apprends à Oz, et les outils qu'il s'est fabriqués. Quand la situation correspond à la description d'un skill, il lit les instructions et les suit. Un nouveau skill ou outil compte à partir de la conversation suivante."
  );
  html += `<section class="panel"><div class="panel-head"><h3>Skills</h3><span class="badge">${skills.length}</span>
    <button class="btn primary sm" id="sk-new" type="button" style="margin-left:auto">${icon("plus")}Ajouter</button></div>`;
  html += skills.length
    ? skills
        .map(
          (s, i) => s.proposed
            ? `<div class="list-row"><div class="main">
            <div class="title" style="white-space:normal">${esc(s.name)} <span class="badge">Proposé par Oz</span></div>
            <div class="meta" style="white-space:normal">${esc(s.description)}</div>
            ${s.reason ? `<div class="meta" style="white-space:normal">Pourquoi : ${esc(s.reason)}</div>` : ""}</div>
            <button class="btn primary sm" data-accept="${i}" type="button">Accepter</button>
            <button class="btn sm" data-edit="${i}" type="button">Modifier</button>
            <button class="btn danger sm" data-del="${i}" type="button">Refuser</button></div>`
            : `<div class="list-row"><div class="main">
            <div class="title" style="white-space:normal">${esc(s.name)}</div>
            <div class="meta" style="white-space:normal">${esc(s.description)}</div>
            <div class="meta"><time>${fmtTs(s.ts)}</time></div></div>
            <input class="switch" type="checkbox" role="switch" data-toggle="${i}" aria-label="Activer ${esc(s.name)}"${s.enabled ? " checked" : ""}>
            <button class="btn sm" data-edit="${i}" type="button">Modifier</button>
            <button class="btn danger sm" data-del="${i}" type="button">${icon("trash")}Supprimer</button></div>`
        )
        .join("")
    : `<div class="empty"><strong>Aucun skill</strong>Ajoute-en un : par exemple comment préparer ta journée, ou comment réagir quand tu lances une réunion.</div>`;
  html += `</section>`;

  // outils qu'Oz s'est fabriqués (PowerShell, testés après ton accord)
  html += `<section class="panel"><div class="panel-head"><h3>Outils fabriqués par Oz</h3><span class="badge">${tools.length}</span></div>`;
  html += tools.length
    ? tools
        .map((t, i) => {
          const state = t.status !== "actif" ? "En test" : t.approved_code === t.code ? "Actif" : "Modifié, accord à redonner";
          return `<div class="list-row" style="flex-wrap:wrap"><div class="main">
            <div class="title" style="white-space:normal">${esc(t.name)} <span class="badge">${state}</span></div>
            <div class="meta" style="white-space:normal">${esc(t.description)}</div>
            <div class="meta">${t.parameters ? "Paramètres : " + esc(t.parameters) + " · " : ""}${t.uses} utilisation(s) · <time>${fmtTs(t.ts)}</time></div></div>
            <button class="btn sm" data-code="${i}" type="button">Voir le code</button>
            <button class="btn danger sm" data-tool="${i}" type="button">${icon("trash")}Supprimer</button>
            <pre class="tool-code hidden" id="tool-code-${i}">${esc(t.code)}</pre></div>`;
        })
        .join("")
    : `<div class="empty"><strong>Aucun outil</strong>Quand une capacité lui manque, Oz peut s'écrire un outil PowerShell. Il est testé seulement après ton accord, sur l'œuf.</div>`;
  html += `</section>`;
  content.innerHTML = html;

  content.querySelectorAll<HTMLButtonElement>("button[data-code]").forEach((b) =>
    b.addEventListener("click", () => document.getElementById(`tool-code-${b.dataset.code}`)!.classList.toggle("hidden"))
  );
  content.querySelectorAll<HTMLButtonElement>("button[data-tool]").forEach((b) =>
    b.addEventListener("click", async () => {
      const t = tools[Number(b.dataset.tool)];
      if (!confirm(`Supprimer l'outil « ${t.name} » ?`)) return;
      await invoke("delete_custom_tool", { name: t.name });
      toast("Outil supprimé");
      void renderSkills();
    })
  );

  document.getElementById("sk-new")!.addEventListener("click", () => {
    editing = { ...EMPTY };
    void renderSkills();
  });
  content.querySelectorAll<HTMLButtonElement>("button[data-edit]").forEach((b) =>
    b.addEventListener("click", () => {
      editing = { ...skills[Number(b.dataset.edit)] };
      void renderSkills();
    })
  );
  content.querySelectorAll<HTMLInputElement>("input[data-toggle]").forEach((t) =>
    t.addEventListener("change", async () => {
      const s = skills[Number(t.dataset.toggle)];
      try {
        await invoke("save_skill", { skill: { ...s, enabled: t.checked } });
        toast(t.checked ? "Skill activé" : "Skill désactivé");
      } catch (e) {
        toast(String(e), false);
      }
      void renderSkills();
    })
  );
  content.querySelectorAll<HTMLButtonElement>("button[data-accept]").forEach((b) =>
    b.addEventListener("click", async () => {
      const s = skills[Number(b.dataset.accept)];
      await invoke("save_skill", { skill: { ...s, enabled: true, proposed: false, reason: "" } });
      toast("Skill accepté");
      void renderSkills();
    })
  );
  content.querySelectorAll<HTMLButtonElement>("button[data-del]").forEach((b) =>
    b.addEventListener("click", async () => {
      const s = skills[Number(b.dataset.del)];
      if (!confirm(`Supprimer le skill « ${s.name} » ?`)) return;
      await invoke("delete_skill", { id: s.id });
      toast("Skill supprimé");
      void renderSkills();
    })
  );
}

function renderEditor(s: Skill): void {
  const creating = !s.id;
  content.innerHTML =
    pageHead(creating ? "Nouveau skill" : "Modifier le skill", "Écris comme si tu expliquais à Oz : quand s'en servir, puis quoi faire.") +
    `<section class="panel">
      <div class="field"><label for="sk-name">Nom</label>
        <input id="sk-name" type="text" value="${esc(s.name)}" placeholder="Préparer ma journée" autocomplete="off"></div>
      <div class="field"><label for="sk-desc">Quand s'en servir</label>
        <input id="sk-desc" type="text" value="${esc(s.description)}" maxlength="240" placeholder="Quand Julien commence sa journée ou demande ce qu'il a à faire" autocomplete="off">
        <div class="hint">C'est la seule partie qu'Oz voit en permanence : elle doit lui dire clairement dans quelle situation lire ce skill.</div></div>
      <div class="field"><label for="sk-instr">Instructions</label>
        <textarea id="sk-instr" rows="12" maxlength="8000" placeholder="1. Regarde son calendrier dans Chrome&#10;2. Résume les trois choses importantes&#10;3. Demande-lui par laquelle il veut commencer">${esc(s.instructions)}</textarea></div>
      <div class="field"><label class="with-action" style="align-items:center;gap:10px">
        <input id="sk-enabled" class="switch" type="checkbox" role="switch"${s.enabled ? " checked" : ""}>Actif</label></div>
      <div class="form-actions">
        <button class="btn" id="sk-cancel" type="button">Annuler</button>
        <button class="btn primary" id="sk-save" type="button">${icon("save")}Enregistrer</button>
      </div>
    </section>`;
  (document.getElementById(creating ? "sk-name" : "sk-instr") as HTMLElement).focus();

  document.getElementById("sk-cancel")!.addEventListener("click", () => {
    editing = null;
    void renderSkills();
  });
  document.getElementById("sk-save")!.addEventListener("click", async () => {
    const val = (id: string) => (document.getElementById(id) as HTMLInputElement | HTMLTextAreaElement).value;
    const skill: Skill = {
      ...s,
      name: val("sk-name"),
      description: val("sk-desc"),
      instructions: val("sk-instr"),
      enabled: (document.getElementById("sk-enabled") as HTMLInputElement).checked,
      proposed: false,
      reason: "",
    };
    try {
      await invoke("save_skill", { skill });
      toast(creating ? "Skill ajouté" : "Skill enregistré");
      editing = null;
      void renderSkills();
    } catch (e) {
      toast(String(e), false);
    }
  });
}

/** Quitter l'onglet abandonne l'édition en cours. */
export function leaveSkills(): void {
  editing = null;
}

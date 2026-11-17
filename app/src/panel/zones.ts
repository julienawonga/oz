import { invoke } from "@tauri-apps/api/core";
import { content, icon, toast, pageHead } from "./util";

// ── Zones ──
export async function renderZones(): Promise<void> {
  const zones = (await invoke("get_zones")) as Array<{ x: number; y: number; w: number; h: number; label: string }>;
  let html = pageHead(
    "Zones restreintes",
    "Zones de l'écran où Oz ne se rend pas. Pour en créer une, placez Oz à l'endroit voulu puis écrivez « zone interdite »."
  );
  html += `<section class="panel"><div class="panel-head"><h3>Zones définies</h3><span class="badge">${zones.length}</span></div>`;
  if (zones.length === 0) {
    html += `<div class="empty"><strong>Aucune zone</strong>Oz peut se déplacer librement sur l'écran.</div>`;
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


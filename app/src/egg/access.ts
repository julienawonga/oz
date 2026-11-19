import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { converse } from "./converse";
import { state } from "./state";
import { accessBadge, accessCard, placeBubble } from "./ui";

// ── Demandes d'accès : un badge sur l'œuf, une carte à la demande ────
interface AccessReq { app: string; process: string; title: string; need: "look" | "act"; intent: string; exec?: undefined }
/** Commande, lecture de fichier ou outil fabriqué : Oz montre ce qu'il veut lancer. */
interface ExecReq { exec: { id: number; title: string; detail: string }; app: string; process: string; need: "exec" }
type Req = AccessReq | ExecReq;
const accessQueue: Req[] = [];

void listen<Omit<AccessReq, "intent">>("oz://access-request", (e) => {
  const dup = accessQueue.some((r) => r.process === e.payload.process && r.need === e.payload.need);
  if (!dup) accessQueue.push({ ...e.payload, intent: state.lastUserIntent });
  refreshAccessBadge();
});

void listen<{ id: number; title: string; detail: string }>("oz://exec-request", (e) => {
  accessQueue.push({ exec: e.payload, app: "Oz", process: "", need: "exec" });
  refreshAccessBadge();
  openAccessCard(); // on montre tout de suite ce qu'il veut lancer
});

function refreshAccessBadge(): void {
  const n = accessQueue.length;
  accessBadge.classList.toggle("hidden", n === 0);
  document.getElementById("access-count")!.textContent = String(n);
  if (n === 0) closeAccessCard();
}

function esc(s: string): string {
  return s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
}

function openAccessCard(): void {
  const req = accessQueue[0];
  if (!req) return;
  const sanctuary = document.getElementById("access-sanctuary")!;
  sanctuary.classList.toggle("hidden", !!req.exec);
  if (req.exec) {
    document.getElementById("access-text")!.innerHTML =
      `<b>${esc(req.exec.title)}</b><pre class="exec-detail">${esc(req.exec.detail)}</pre>` +
      (accessQueue.length > 1 ? `<em>${accessQueue.length - 1} autre(s) en attente</em>` : "");
    document.getElementById("access-allow")!.textContent = "Autoriser";
    accessCard.classList.remove("hidden");
    void placeBubble(true);
    return;
  }
  const verb = req.need === "act" ? "agir sur" : "regarder";
  document.getElementById("access-text")!.innerHTML =
    `<b>${esc(req.app)}</b> · ${esc(req.title).slice(0, 60)}<br><span>Oz demande à ${verb} cette application.</span>` +
    (accessQueue.length > 1 ? `<em>${accessQueue.length - 1} autre(s) en attente</em>` : "");
  document.getElementById("access-allow")!.textContent =
    req.need === "act" ? "Autoriser les actions" : "Autoriser la lecture";
  accessCard.classList.remove("hidden");
  void placeBubble(true);
}

export function closeAccessCard(): void {
  accessCard.classList.add("hidden");
}

async function resolveAccess(level: "look" | "act" | "sanctuary" | "none"): Promise<void> {
  const req = accessQueue.shift();
  if (!req) return;
  if (req.exec) {
    await invoke("exec_decision", { id: req.exec.id, allow: level !== "none" }).catch(() => {});
    refreshAccessBadge();
    if (accessQueue.length) openAccessCard();
    return;
  }
  // la règle porte sur l'application entière (processus::*)
  if (level !== "none") {
    await invoke("set_permission", { key: `${req.process}::*`, level }).catch(() => {});
  }
  refreshAccessBadge();
  if (accessQueue.length) openAccessCard();
  if ((level === "look" || level === "act") && req.intent) {
    // accord donné -> on rejoue la demande d'origine sans la faire répéter.
    // En pleine conversation vocale, dans la session Live elle-même : le
    // cerveau texte, lui, répondait sans relancer l'outil (vue jamais lancée).
    if (state.micRunning) {
      const verb = level === "act" ? "agir sur" : "regarder";
      void invoke("live_prompt", {
        text: `[Note interne] Julien vient de t'autoriser à ${verb} ${req.app}. Refais maintenant sa demande avec le bon outil, sans commenter l'autorisation : « ${req.intent} »`,
      }).catch(() => void converse(req.intent));
    } else {
      void converse(req.intent);
    }
  }
}

accessBadge.addEventListener("pointerdown", (e) => e.stopPropagation());
accessBadge.addEventListener("click", (e) => {
  e.stopPropagation();
  if (accessCard.classList.contains("hidden")) openAccessCard();
  else closeAccessCard();
});
accessCard.addEventListener("pointerdown", (e) => e.stopPropagation());
document.getElementById("access-allow")!.addEventListener("click", () => {
  const need = accessQueue[0]?.need ?? "look";
  void resolveAccess(need === "exec" ? "act" : need);
});
document.getElementById("access-deny")!.addEventListener("click", () => void resolveAccess("none"));
document.getElementById("access-sanctuary")!.addEventListener("click", () => void resolveAccess("sanctuary"));

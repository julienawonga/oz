import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { converse } from "./converse";
import { state } from "./state";
import { accessBadge, accessCard } from "./ui";

// ── Demandes d'accès : un badge sur l'œuf, une carte à la demande ────
interface AccessReq { app: string; process: string; title: string; need: "look" | "act"; intent: string }
const accessQueue: AccessReq[] = [];

void listen<Omit<AccessReq, "intent">>("nid://access-request", (e) => {
  const dup = accessQueue.some((r) => r.process === e.payload.process && r.need === e.payload.need);
  if (!dup) accessQueue.push({ ...e.payload, intent: state.lastUserIntent });
  refreshAccessBadge();
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
  const verb = req.need === "act" ? "agir sur" : "regarder";
  document.getElementById("access-text")!.innerHTML =
    `<b>${esc(req.app)}</b> · ${esc(req.title).slice(0, 60)}<br><span>Nid demande à ${verb} cette application.</span>` +
    (accessQueue.length > 1 ? `<em>${accessQueue.length - 1} autre(s) en attente</em>` : "");
  document.getElementById("access-allow")!.textContent =
    req.need === "act" ? "Autoriser les actions" : "Autoriser la lecture";
  accessCard.classList.remove("hidden");
}

export function closeAccessCard(): void {
  accessCard.classList.add("hidden");
}

async function resolveAccess(level: "look" | "act" | "sanctuary" | "none"): Promise<void> {
  const req = accessQueue.shift();
  if (!req) return;
  // la règle porte sur l'application entière (processus::*)
  if (level !== "none") {
    await invoke("set_permission", { key: `${req.process}::*`, level }).catch(() => {});
  }
  refreshAccessBadge();
  if (accessQueue.length) openAccessCard();
  if ((level === "look" || level === "act") && req.intent) {
    // accord donné -> on rejoue la demande d'origine sans la faire répéter
    void converse(req.intent);
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
  void resolveAccess(accessQueue[0]?.need ?? "look");
});
document.getElementById("access-deny")!.addEventListener("click", () => void resolveAccess("none"));
document.getElementById("access-sanctuary")!.addEventListener("click", () => void resolveAccess("sanctuary"));

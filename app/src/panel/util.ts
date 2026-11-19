// ── Utilitaires partagés du panneau ──

export const content = document.getElementById("content")!;

/** Onglet courant (partagé : le chat live doit savoir s'il est affiché). */
export const view = { tab: "chat" };

const ICONS: Record<string, string> = {
  send: '<path d="M22 2 11 13M22 2l-7 20-4-9-9-4 20-7z"/>',
  trash: '<path d="M3 6h18M8 6V4h8v2m-9 0 1 14h8l1-14"/>',
  eye: '<path d="M2 12s4-7 10-7 10 7 10 7-4 7-10 7S2 12 2 12z"/><circle cx="12" cy="12" r="3"/>',
  plus: '<path d="M12 5v14M5 12h14"/>',
  save: '<path d="M5 3h11l3 3v15H5z"/><path d="M8 3v6h8V3M8 21v-7h8v7"/>',
};
export function icon(name: string): string {
  return `<svg class="icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">${ICONS[name] ?? ""}</svg>`;
}

export function fmtTs(ts: number): string {
  const d = new Date(ts * 1000);
  return d.toLocaleDateString("fr", { day: "2-digit", month: "short" }) + " " +
    d.toLocaleTimeString("fr", { hour: "2-digit", minute: "2-digit" });
}

export function esc(s: unknown): string {
  return String(s ?? "").replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;");
}

export function toast(text: string, ok = true): void {
  const el = document.createElement("div");
  el.className = `toast ${ok ? "ok" : "err"}`;
  el.textContent = text;
  document.body.appendChild(el);
  setTimeout(() => el.classList.add("out"), 2400);
  setTimeout(() => el.remove(), 2800);
}

export function pageHead(title: string, desc: string): string {
  return `<div class="page-head"><h2>${title}</h2><p>${desc}</p></div>`;
}

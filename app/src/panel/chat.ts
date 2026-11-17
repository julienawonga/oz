import { invoke, Channel } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { content, view, icon } from "./util";

interface Msg {
  role: "user" | "nid";
  text: string;
}
let busy = false;
const chatLog: Msg[] = [];

// Les échanges faits depuis l'œuf apparaissent aussi ici
void listen<Msg>("nid://chat-line", (e) => {
  chatLog.push(e.payload);
  if (view.tab !== "chat") return;
  const log = document.getElementById("chat-log");
  if (log) {
    log.querySelector(".empty")?.remove();
    appendMsg(log, e.payload.role, e.payload.text);
  }
});

function appendMsg(log: HTMLElement, role: string, text: string): HTMLElement {
  const div = document.createElement("div");
  div.className = `msg ${role}`;
  div.textContent = text;
  log.appendChild(div);
  div.scrollIntoView({ block: "end" });
  return div;
}

// ── Conversation ──
export function renderChat(): void {
  content.innerHTML = `
    <div class="chat-wrap">
      <div id="chat-log"></div>
      <form id="chat-form">
        <input id="panel-chat-input" placeholder="Écrire à Nid" autocomplete="off" aria-label="Message" />
        <button class="btn primary" type="submit">${icon("send")}Envoyer</button>
      </form>
    </div>`;
  const log = document.getElementById("chat-log")!;
  if (chatLog.length === 0) {
    log.innerHTML = `<div class="empty"><strong>Aucun message</strong>Les échanges avec Nid, depuis ici ou depuis l'œuf, apparaissent dans cette vue.</div>`;
  }
  for (const m of chatLog) appendMsg(log, m.role, m.text);

  document.getElementById("chat-form")!.addEventListener("submit", (e) => {
    e.preventDefault();
    const input = document.getElementById("panel-chat-input") as HTMLInputElement;
    const text = input.value.trim();
    if (!text || busy) return;
    input.value = "";
    chatLog.push({ role: "user", text });
    void sendToNid(text);
  });
  (document.getElementById("panel-chat-input") as HTMLInputElement).focus();
}

async function sendToNid(text: string): Promise<void> {
  busy = true;
  renderChat();
  const form = document.getElementById("chat-form")!;
  form.classList.add("busy");
  const log = document.getElementById("chat-log")!;
  const streaming = appendMsg(log, "nid streaming", "…");

  const onChunk = new Channel<string>();
  let acc = "";
  onChunk.onmessage = (tok) => {
    acc += tok;
    streaming.textContent = acc;
    streaming.scrollIntoView({ block: "end" });
  };
  try {
    const reply = await invoke<string>("chat", { userMessage: text, onChunk });
    streaming.classList.remove("streaming");
    streaming.textContent = reply;
    chatLog.push({ role: "nid", text: reply });
  } catch (err) {
    streaming.classList.remove("streaming");
    streaming.classList.add("error");
    streaming.textContent = String(err);
  } finally {
    busy = false;
    form.classList.remove("busy");
    (document.getElementById("panel-chat-input") as HTMLInputElement | null)?.focus();
  }
}

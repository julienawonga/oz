import { invoke, Channel } from "@tauri-apps/api/core";
import { emit } from "@tauri-apps/api/event";
import { stopVoice } from "./audio";
import { hideChatBar } from "./chatbar";
import { handleLocalIntent } from "./intents";
import { state } from "./state";
import { chatBar, chatInput, say, setThinking, showBubble } from "./ui";

// ── Conversation : intentions locales, sinon cerveau en streaming ────

async function sendMessage(): Promise<void> {
  const text = chatInput.value.trim();
  if (!text || state.thinking) return;
  chatInput.value = "";
  hideChatBar();
  await converse(text);
}

chatInput.addEventListener("keydown", (e) => {
  if (e.key === "Enter") void sendMessage();
});
document.getElementById("chat-send")!.addEventListener("click", (e) => {
  e.stopPropagation();
  void sendMessage();
});
chatBar.addEventListener("pointerdown", (e) => e.stopPropagation());

export async function converse(text: string): Promise<void> {
  state.lastUserIntent = text;

  // Commandes locales instantanées (maison, zones, vision, déplacement, pose)
  if (await handleLocalIntent(text)) return;

  stopVoice(); // une nouvelle question coupe la réponse précédente
  void emit("nid://chat-line", { role: "user", text }); // miroir dans le panneau
  setThinking(true);
  try {
    const onChunk = new Channel<string>();
    let acc = "";
    let fed = 0;
    // phrase complète = ponctuation forte suivie d'un espace/fin, ou saut de ligne
    const SENTENCE = /^[\s\S]*?(?:[.!?…]+(?=\s|$)|\n)/;
    onChunk.onmessage = (tok) => {
      acc += tok;
      showBubble(acc, { typing: false });
      if (state.voiceLiveMode) {
        for (;;) {
          const m = acc.slice(fed).match(SENTENCE);
          if (!m) break;
          fed += m[0].length;
          if (m[0].trim().length > 2) void invoke("voice_feed", { text: m[0] }).catch(() => {});
        }
      }
    };
    const reply = await invoke<string>("chat", { userMessage: text, onChunk });
    if (!reply.trim()) {
      say("Hmm, je n'ai rien trouvé à répondre 🤔", 4000);
      return;
    }
    void emit("nid://chat-line", { role: "nid", text: reply });
    showBubble(reply, { ttl: Math.min(14000, 3000 + reply.length * 60) });
    // gemini-live : on envoie le reste de la phrase puis on CLÔT le tour,
    // sinon le modèle n'ouvre jamais la bouche (voice_end déclenche la parole)
    if (state.voiceLiveMode) {
      const tail = acc.slice(fed).trim();
      if (tail) await invoke("voice_feed", { text: tail }).catch(() => {});
      void invoke("voice_end").catch((err) => say("🎙️ " + String(err).slice(0, 100), 6000));
    } else {
      invoke("speak", { text: reply }).catch(() => {});
    }
  } catch (err) {
    say(friendlyError(err), 7000);
  } finally {
    setThinking(false);
  }
}

/** Erreurs techniques → phrase courte et utile dans la bulle. */
function friendlyError(err: unknown): string {
  const msg = String(err);
  if (/injoignable|connexion|timeout/i.test(msg)) return "Je n'arrive pas à joindre mon cerveau 🧠 — vérifie la connexion ou le frigo 🍽️";
  if (/clé|api|401|403/i.test(msg)) return "Ma clé API ne passe pas 🔑 — regarde dans le frigo 🍽️";
  if (/429/.test(msg)) return "Trop de questions d'un coup 😵 — redonne-moi une seconde";
  return msg.slice(0, 140);
}

import { invoke, Channel } from "@tauri-apps/api/core";
import { emit } from "@tauri-apps/api/event";
import { claimVoice, markVoiceClosed, stopVoice } from "./audio";
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

const READ_ALOUD = "Lis à voix haute, mot pour mot, sans rien ajouter ni commenter, ce texte : ";

// La parole de Julien (micro) passe par le même chemin que le texte :
// intentions locales, broker de permissions, mémoire, puis cerveau.
state.onUserSpeech = (text) => {
  void converse(text);
};

export async function converse(text: string): Promise<void> {
  state.lastUserIntent = text;

  // Commandes locales instantanées (maison, zones, vision, déplacement, pose)
  if (await handleLocalIntent(text)) return;

  stopVoice(); // une nouvelle question coupe la réponse précédente
  void emit("oz://chat-line", { role: "user", text }); // miroir dans le panneau
  setThinking(true);
  try {
    const onChunk = new Channel<string>();
    let acc = "";
    let fed = 0;
    let spokeIntro = false;
    // Live ne doit pas répondre de lui-même : on lui demande de LIRE le texte du cerveau
    const feed = (piece: string): Promise<void> => {
      claimVoice();
      const text = spokeIntro ? piece : READ_ALOUD + piece;
      spokeIntro = true;
      return invoke<void>("voice_feed", { text }).catch(() => {});
    };
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
          if (m[0].trim().length > 2) void feed(m[0]);
        }
      }
    };
    const reply = await invoke<string>("chat", { userMessage: text, onChunk });
    if (!reply.trim()) {
      say("Hmm, je n'ai rien trouvé à répondre 🤔", 4000);
      return;
    }
    void emit("oz://chat-line", { role: "oz", text: reply });
    showBubble(reply, { ttl: Math.min(14000, 3000 + reply.length * 60) });
    // gemini-live : on envoie le reste de la phrase puis on CLÔT le tour,
    // sinon le modèle n'ouvre jamais la bouche (voice_end déclenche la parole)
    if (state.voiceLiveMode) {
      // les réponses d'actions (« fermé ») n'arrivent pas en flux : on lit alors la réponse entière
      const spoken = acc.trim() ? acc : reply;
      const tail = spoken.slice(fed).trim();
      if (tail) await feed(tail);
      if (spokeIntro) {
        markVoiceClosed();
        void invoke("voice_end").catch((err) => say("🎙️ " + String(err).slice(0, 100), 6000));
      }
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

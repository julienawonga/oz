import { invoke, Channel } from "@tauri-apps/api/core";
import { emit } from "@tauri-apps/api/event";
import { claimVoice, markVoiceClosed, READ_ALOUD, releaseVoice, stopVoice } from "./audio";
import { hideChatBar } from "./chatbar";
import "./intents"; // exécute les actions du corps demandées par le cerveau
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

// La parole de Julien (micro) passe par le même chemin que le texte :
// intentions locales, broker de permissions, mémoire, puis cerveau.
state.onUserSpeech = (text) => {
  void converse(text);
};

// Numéro de la question en cours : une nouvelle question rend la précédente
// obsolète, ses phrases restantes ne doivent plus partir à la voix.
let currentTurn = 0;

export async function converse(text: string): Promise<void> {
  const myTurn = ++currentTurn;
  const stale = (): boolean => myTurn !== currentTurn;
  state.lastUserIntent = text;

  stopVoice(); // une nouvelle question coupe la réponse précédente
  void emit("oz://chat-line", { role: "user", text }); // miroir dans le panneau
  setThinking(true);
  try {
    const onChunk = new Channel<string>();
    let acc = "";
    let fed = 0;
    let spokeIntro = false;
    let voiceErrorShown = false;
    // Envois vocaux STRICTEMENT dans l'ordre : la première phrase ouvre la
    // session (~1 s) ; sans file, les phrases suivantes et surtout voice_end
    // pouvaient la doubler -> tour clos avant le texte -> Oz restait muet.
    let voiceChain: Promise<void> = Promise.resolve();
    // Live ne doit pas répondre de lui-même : on lui demande de LIRE le texte du cerveau
    const feed = (piece: string): Promise<void> => {
      claimVoice();
      const text = spokeIntro ? piece : READ_ALOUD + piece;
      spokeIntro = true;
      voiceChain = voiceChain.then(() => sendPiece(text));
      return voiceChain;
    };
    const sendPiece = (text: string): Promise<void> => {
      // session en échec, ou question remplacée par une plus récente : on se tait
      if (voiceErrorShown || stale()) return Promise.resolve();
      return invoke<void>("voice_feed", { text }).catch((err) => {
        // la voix ne doit jamais échouer en silence : on le dit une fois dans la bulle
        if (!voiceErrorShown) {
          voiceErrorShown = true;
          releaseVoice();
          say("Voix indisponible : " + String(err).slice(0, 110), 8000);
        }
      });
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
        await voiceChain; // toutes les phrases sont parties AVANT de clore le tour
        if (!voiceErrorShown && !stale()) {
          markVoiceClosed();
          void invoke("voice_end").catch((err) => say("🎙️ " + String(err).slice(0, 100), 6000));
        }
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

import { invoke } from "@tauri-apps/api/core";
import { emit } from "@tauri-apps/api/event";
import { content, icon, esc, toast, pageHead } from "./util";

// ── Modèles ──
interface BrainCfg {
  provider: string; model: string; url: string;
  vision_model: string; audio_model: string; api_key: string;
}

export async function renderBrain(): Promise<void> {
  const cfg = (await invoke("get_brain_config")) as BrainCfg;
  const field = (id: string, label: string, value: string, hint = "") =>
    `<div class="field"><label for="${id}">${label}</label>
     <input id="${id}" type="text" value="${esc(value)}" autocomplete="off" spellcheck="false">
     ${hint ? `<div class="hint">${hint}</div>` : ""}</div>`;

  content.innerHTML =
    pageHead("Modèles", "Fournisseur et modèles utilisés pour la conversation, la vision et la voix. Les changements sont pris en compte au message suivant.") +
    `<section class="panel"><div class="panel-head"><h3>Conversation</h3></div>
      <div class="field"><label for="b-provider">Fournisseur</label>
        <select id="b-provider">
          <option value="gemini"${cfg.provider === "gemini" ? " selected" : ""}>Gemini (cloud)</option>
          <option value="ollama"${cfg.provider === "ollama" ? " selected" : ""}>Ollama (local)</option>
        </select></div>` +
    field("b-model", "Modèle", cfg.model) +
    field("b-url", "URL du serveur Ollama", cfg.url, "Utilisée uniquement avec le fournisseur Ollama.") +
    `</section>
    <section class="panel"><div class="panel-head"><h3>Vision et voix</h3></div>` +
    field("b-vision", "Modèle de vision", cfg.vision_model, "Analyse des captures de fenêtres autorisées.") +
    field("b-audio", "Modèle de voix", cfg.audio_model ?? "", "Exemples : gemini-live, gemini-tts:Kore. Vide pour la voix du système.") +
    `</section>
    <section class="panel"><div class="panel-head"><h3>Authentification</h3></div>
      <div class="field"><label for="b-key">Clé API Gemini</label>
        <div class="with-action">
          <input id="b-key" type="password" value="${esc(cfg.api_key ?? "")}" autocomplete="off" spellcheck="false">
          <button class="btn" id="b-reveal" type="button">${icon("eye")}<span>Afficher</span></button>
        </div>
        <div class="hint">Stockée localement dans la configuration de l'application.</div></div>
      <div class="form-actions"><button class="btn primary" id="b-save" type="button">${icon("save")}Enregistrer</button></div>
    </section>`;

  const val = (id: string) => (document.getElementById(id) as HTMLInputElement).value.trim();
  const keyInput = document.getElementById("b-key") as HTMLInputElement;
  document.getElementById("b-reveal")!.addEventListener("click", (e) => {
    const show = keyInput.type === "password";
    keyInput.type = show ? "text" : "password";
    (e.currentTarget as HTMLElement).querySelector("span")!.textContent = show ? "Masquer" : "Afficher";
  });
  document.getElementById("b-save")!.addEventListener("click", async () => {
    if (val("b-provider") === "gemini" && !val("b-key")) return toast("Une clé API est requise pour Gemini", false);
    if (!val("b-model")) return toast("Le modèle de conversation est requis", false);
    await invoke("set_brain_config", {
      config: {
        provider: val("b-provider"), model: val("b-model"), url: val("b-url"),
        vision_model: val("b-vision"), audio_model: val("b-audio"), api_key: val("b-key"),
      },
    });
    void emit("nid://brain-updated"); // l'œuf relit le mode voix sans redémarrer
    toast("Configuration enregistrée");
  });
}


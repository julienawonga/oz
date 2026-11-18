<p align="center">
  <img src="docs/assets/oz.svg" alt="Oz, le petit œuf" width="120">
</p>

<h1 align="center">Oz</h1>

<p align="center">
  <strong>Un petit œuf qui vit sur ton bureau Windows.</strong><br>
  Il t'écoute, te répond à voix haute, regarde ton écran quand tu le lui demandes,<br>
  et ne touche à tes fenêtres qu'avec ta permission.
</p>

<p align="center">
  <img src="https://img.shields.io/badge/Windows-10%20%7C%2011-0078D4?style=for-the-badge&logo=windows11&logoColor=white" alt="Windows 10 | 11">
  <img src="https://img.shields.io/badge/Tauri-2-24C8DB?style=for-the-badge&logo=tauri&logoColor=white" alt="Tauri 2">
  <img src="https://img.shields.io/badge/Rust-stable-B7410E?style=for-the-badge&logo=rust&logoColor=white" alt="Rust">
  <img src="https://img.shields.io/badge/TypeScript-Vite-3178C6?style=for-the-badge&logo=typescript&logoColor=white" alt="TypeScript + Vite">
</p>
<p align="center">
  <img src="https://img.shields.io/badge/Gemini-Live%20%2B%20outils-8E75B2?style=flat-square&logo=googlegemini&logoColor=white" alt="Gemini Live + outils">
  <img src="https://img.shields.io/badge/Ollama-local-000000?style=flat-square&logo=ollama&logoColor=white" alt="Ollama">
  <img src="https://img.shields.io/badge/permissions-appliqu%C3%A9es%20en%20Rust-0284c7?style=flat-square" alt="Permissions appliquées en Rust">
  <a href="LICENSE"><img src="https://img.shields.io/badge/licence-MIT-22c55e?style=flat-square" alt="Licence MIT"></a>
</p>

<p align="center">
  <a href="#-démarrage-rapide">Démarrage rapide</a> ·
  <a href="#-fonctionnalités">Fonctionnalités</a> ·
  <a href="#-le-contrat-de-confiance">Contrat de confiance</a> ·
  <a href="docs/architecture.md">Architecture</a> ·
  <a href="#️-feuille-de-route">Feuille de route</a>
</p>

---

## 🥚 Pourquoi Oz

Les assistants vivent dans une fenêtre qu'il faut ouvrir. Oz, lui, **habite le bureau** : un petit œuf animé, en surcouche transparente, qui se balade, s'assoit, écoute quand on lui parle et répond de vive voix.

Il comprend ce qu'on lui dit **avec un vrai modèle de langage, sans mots-clés** : « qu'est-ce qui est ouvert ? », « ferme-moi Spotify », « qu'est-ce que tu vois sur mon écran ? ». Mais il ne peut **rien** faire que tu n'aies autorisé : chaque permission est vérifiée dans le code Rust, jamais dans un prompt.

## ✨ Fonctionnalités

| | |
|---|---|
| 🎙️ **Voix temps réel** | Micro en streaming vers Gemini Live, transcription en direct dans la bulle, réponse lue à voix haute. Une nouvelle question coupe l'ancienne réponse. |
| 🧠 **Compréhension par LLM** | Gemini comprend la demande, quelle que soit la formulation, et appelle des outils (function calling). Aucun routeur par mots-clés. |
| 👀 **Vision** | Oz capture une fenêtre autorisée et la décrit, ou t'aide sur ce que tu fais. |
| 🪟 **Actions sur les fenêtres** | Lister, fermer, mettre au premier plan, toujours derrière le broker de permissions. |
| 🐣 **Un œuf vivant** | Animations, poses (assis, debout), balades, glisser-déposer, zones de l'écran interdites. |
| 🗂️ **Mémoire** | Faits durables tirés des conversations, notes sur les fenêtres observées, effaçables un par un. |
| 🕒 **Contexte** | Heure locale injectée à chaque message. |
| 🎛️ **Panneau d'administration** | Conversation, permissions (avec les fenêtres ouvertes en direct), zones, modèles, mémoire, journal d'activité. |

### Outils dont dispose le cerveau

| Outil | Ce qu'il fait | Permission requise |
|---|---|---|
| `lister_fenetres` | Applications et fenêtres ouvertes | aucune (les sanctuaires restent invisibles) |
| `regarder_fenetre` | Capture et décrit une fenêtre | *Lecture* |
| `fermer_fenetre` | Ferme une fenêtre | *Actions* |
| `mettre_au_premier_plan` | Ramène une fenêtre devant | *Actions* |
| `se_deplacer` · `changer_pose` | Déplace l'œuf, l'assoit ou le relève | — |
| `zone_interdite` | Interdit ou autorise l'endroit où se trouve Oz | — |

## 🔐 Le contrat de confiance

> [!IMPORTANT]
> Le modèle **propose**, le code **dispose**. Un LLM ne peut contourner aucune permission : elles sont appliquées par le *broker*, en Rust.

- **Rien sans accord.** Une fenêtre inconnue déclenche une demande d'accès sur l'œuf : *Regarder*, *Agir*, *Pas maintenant* ou *Sanctuaire*.
- **Les sanctuaires sont inviolables.** Une fenêtre en sanctuaire n'existe pas pour Oz : ni listée, ni vue, ni touchée. Elle est « introuvable ».
- **Cinq niveaux.** Sanctuaire · Aucun accès · Lecture · Actions · Confiance, par application ou par fenêtre.
- **Quittances.** Tout ce qu'Oz a vu, fait ou obtenu est consigné dans le journal d'activité.
- **Secrets au coffre.** La clé Gemini est dans le Gestionnaire d'identifiants Windows, jamais dans un fichier.

Détails : [docs/permissions.md](docs/permissions.md).

## 🚀 Démarrage rapide

**Prérequis :** Windows 10 ou 11, [Node.js](https://nodejs.org), [Rust](https://rustup.rs) (stable) et les [prérequis Tauri 2 pour Windows](https://v2.tauri.app/start/prerequisites/).

```bash
git clone https://github.com/julienawonga/oz.git
cd oz/app          # le package.json est dans app/, pas à la racine
npm install
npm run tauri dev
```

Au premier lancement, ouvre le panneau d'administration, onglet **Modèles**, choisis le cerveau et saisis ta clé Gemini.

> [!TIP]
> Pour tester la voix sans l'app : `cd app/src-tauri && cargo run --example voice_probe [fichier.wav]`.

## 🤖 Modèles

| Fournisseur | Conversation | Outils | Voix | Vision |
|---|:---:|:---:|:---:|:---:|
| **Gemini** | ✅ streaming | ✅ function calling | ✅ Gemini Live | ✅ |
| **Ollama** (local) | ✅ streaming | 🚧 bientôt | — | — |

## 🏗️ Architecture

```
  micro ──► Gemini Live (écoute) ──► transcription
                                          │
                                          ▼
  œuf (TS) ◄── réponse en streaming ◄── cerveau Gemini ──► outils ──► broker (Rust) ──► Win32
     │                                                                   ▲
     └──► Gemini Live (parole) ──► voix d'Oz                permissions ─┘
```

```
app/
├── src/egg/        l'œuf : fenêtre, bulle, saisie, voix, mouvement, demandes d'accès
├── src/panel/      le panneau d'administration, un module par onglet
└── src-tauri/src/  Rust : broker, brain, tools, voice, vision, memory, zones, journal
docs/               architecture, permissions, voix, développement
```

## 🧪 Tests

```bash
cd app/src-tauri && cargo test     # broker, routeur, nettoyage des noms
cd .. && npx tsc --noEmit          # typage du front
```

## 🗺️ Feuille de route

- [x] Voix temps réel de bout en bout (deux sessions Gemini Live)
- [x] Outils Gemini à la place des mots-clés
- [x] Permissions par fenêtre, sanctuaires, journal
- [ ] Modes de déplacement : libre, position fixe, derrière les fenêtres, ancré au bord, concentration
- [ ] Outils pour les modèles locaux (Ollama)
- [ ] Personnalisation de l'œuf (couleurs, textures)
- [ ] Appuyer-pour-parler (touche globale) et mot d'éveil

## 📚 Documentation

- [Architecture](docs/architecture.md) : modules, flux d'un message, événements.
- [Permissions](docs/permissions.md) : niveaux, règles, comportement du broker.
- [Voix](docs/voix.md) : pipeline micro, transcription, lecture.
- [Développement](docs/developpement.md) : conventions, tests, données locales, dépannage.

<details>
<summary>Ancien nom : Nid</summary>

Le projet s'appelait Nid. Au premier lancement, Oz reprend les données de l'ancien dossier et la clé API de l'ancienne entrée du Gestionnaire d'identifiants.
</details>

## 📄 Licence

[MIT](LICENSE) © 2026 Julien Awonga

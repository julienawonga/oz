<p align="center">
  <img src="docs/assets/oz.svg" alt="Oz, the little egg" width="120">
</p>

<h1 align="center">Oz</h1>

<p align="center">
  <strong>A little egg that lives on your Windows desktop.</strong><br>
  It listens, talks back out loud, looks at your screen when you ask it to,<br>
  and only touches your windows with your permission.
</p>

<p align="center">
  <img src="https://img.shields.io/badge/Windows-10%20%7C%2011-0078D4?style=for-the-badge&logo=windows11&logoColor=white" alt="Windows 10 | 11">
  <img src="https://img.shields.io/badge/Tauri-2-24C8DB?style=for-the-badge&logo=tauri&logoColor=white" alt="Tauri 2">
  <img src="https://img.shields.io/badge/Rust-stable-B7410E?style=for-the-badge&logo=rust&logoColor=white" alt="Rust">
  <img src="https://img.shields.io/badge/TypeScript-Vite-3178C6?style=for-the-badge&logo=typescript&logoColor=white" alt="TypeScript + Vite">
</p>
<p align="center">
  <img src="https://img.shields.io/badge/Gemini-Live%20%2B%20tools-8E75B2?style=flat-square&logo=googlegemini&logoColor=white" alt="Gemini Live + tools">
  <img src="https://img.shields.io/badge/Ollama-local-000000?style=flat-square&logo=ollama&logoColor=white" alt="Ollama">
  <img src="https://img.shields.io/badge/permissions-enforced%20in%20Rust-0284c7?style=flat-square" alt="Permissions enforced in Rust">
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-22c55e?style=flat-square" alt="MIT license"></a>
</p>

<p align="center">
  <a href="#-quick-start">Quick start</a> ·
  <a href="#-features">Features</a> ·
  <a href="#-the-trust-contract">Trust contract</a> ·
  <a href="docs/architecture.md">Architecture</a> ·
  <a href="#️-roadmap">Roadmap</a>
</p>

---

## 🥚 Why Oz

Assistants live in a window you have to open. Oz **lives on your desktop**: a small animated egg in a transparent overlay that wanders around, sits down, listens when you talk to it and answers out loud.

It understands what you say **with a real language model, no keywords**: "what's open right now?", "close Spotify for me", "what do you see on my screen?". But it can do **nothing** you haven't allowed: every permission is checked in Rust code, never in a prompt.

## ✨ Features

| | |
|---|---|
| 🎙️ **Real-time voice** | Mic streamed to Gemini Live, live transcription in the speech bubble, answer read out loud. A new question cuts off the previous answer. |
| 🧠 **LLM understanding** | Gemini understands the request however it's phrased and calls tools (function calling). No keyword router. |
| 👀 **Vision** | Oz captures an allowed window and describes it, or helps with what you're doing. |
| 🪟 **Window actions** | List, close, bring to front, always behind the permission broker. |
| 🐣 **A living egg** | Animations, poses (sitting, standing), strolls, drag and drop, no-go zones on the screen. |
| 🗂️ **Memory** | Lasting facts drawn from conversations, notes on observed windows, deletable one by one. |
| 🕒 **Context** | Local time injected into every message. |
| 🎛️ **Admin panel** | Chat, permissions (with live list of open windows), zones, models, memory, activity log. |

### Tools available to the brain

| Tool | What it does | Required permission |
|---|---|---|
| `lister_fenetres` | Lists open apps and windows | none (sanctuaries stay invisible) |
| `regarder_fenetre` | Captures and describes a window | *Look* |
| `fermer_fenetre` | Closes a window | *Act* |
| `mettre_au_premier_plan` | Brings a window to the front | *Act* |
| `se_deplacer` · `changer_pose` | Moves the egg, sits it down or stands it up | — |
| `zone_interdite` | Forbids or allows the spot where Oz stands | — |

Tool names are in French: Oz talks to its user in French.

## 🔐 The trust contract

> [!IMPORTANT]
> The model **proposes**, the code **decides**. No LLM can bypass a permission: they are enforced by the *broker*, in Rust.

- **Nothing without consent.** An unknown window triggers an access request on the egg: *Look*, *Act*, *Not now* or *Sanctuary*.
- **Sanctuaries are inviolable.** A sanctuary window doesn't exist for Oz: never listed, never seen, never touched. It is simply "not found".
- **Five levels.** Sanctuary · No access · Look · Act · Trust, per app or per window.
- **Receipts.** Everything Oz has seen, done or been granted is recorded in the activity log.
- **Secrets in the vault.** The Gemini key lives in Windows Credential Manager, never in a file.

Details: [docs/permissions.md](docs/permissions.md).

## 🚀 Quick start

**Requirements:** Windows 10 or 11, [Node.js](https://nodejs.org), [Rust](https://rustup.rs) (stable) and the [Tauri 2 prerequisites for Windows](https://v2.tauri.app/start/prerequisites/).

```bash
git clone https://github.com/julienawonga/oz.git
cd oz/app          # package.json lives in app/, not at the root
npm install
npm run tauri dev
```

On first launch, open the admin panel, **Models** tab, pick the brain and enter your Gemini key.

> [!TIP]
> To test the voice without the app: `cd app/src-tauri && cargo run --example voice_probe [file.wav]`.

## 🤖 Models

| Provider | Chat | Tools | Voice | Vision |
|---|:---:|:---:|:---:|:---:|
| **Gemini** | ✅ streaming | ✅ function calling | ✅ Gemini Live | ✅ |
| **Ollama** (local) | ✅ streaming | 🚧 coming soon | — | — |

## 🏗️ Architecture

```
  mic ──► Gemini Live (listen) ──► transcription
                                        │
                                        ▼
  egg (TS) ◄── streamed answer ◄── Gemini brain ──► tools ──► broker (Rust) ──► Win32
     │                                                              ▲
     └──► Gemini Live (speak) ──► Oz's voice           permissions ─┘
```

```
app/
├── src/egg/        the egg: window, bubble, input, voice, movement, access requests
├── src/panel/      the admin panel, one module per tab
└── src-tauri/src/  Rust: broker, brain, tools, voice, vision, memory, zones, journal
docs/               architecture, permissions, voice, development
```

## 🧪 Tests

```bash
cd app/src-tauri && cargo test     # broker, router, name cleanup
cd .. && npx tsc --noEmit          # front-end type check
```

## 🗺️ Roadmap

- [x] End-to-end real-time voice (two Gemini Live sessions)
- [x] Gemini tools instead of keywords
- [x] Per-window permissions, sanctuaries, activity log
- [ ] Movement modes: free roam, fixed position, behind windows, docked to an edge, focus
- [ ] Tools for local models (Ollama)
- [ ] Egg customization (colors, textures)
- [ ] Push-to-talk (global hotkey) and wake word

## 📚 Documentation

The detailed docs are in French.

- [Architecture](docs/architecture.md): modules, message flow, events.
- [Permissions](docs/permissions.md): levels, rules, broker behavior.
- [Voice](docs/voix.md): mic pipeline, transcription, playback.
- [Development](docs/developpement.md): conventions, tests, local data, troubleshooting.

<details>
<summary>Former name: Nid</summary>

The project used to be called Nid. On first launch, Oz picks up the data from the old folder and the API key from the old Credential Manager entry.
</details>

## 📄 License

[MIT](LICENSE) © 2026 Julien Awonga

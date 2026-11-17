# 🪺 Oz

> *A tiny dweller for your desktop. It knocks before entering.*

**Oz** is an open-source desktop AI companion: a 3D avatar (VRM) that lives on your screen, talks to you with a fully local voice stack, acts on your windows **only with your permission**, and respects the zones you forbid it to enter.

```
🪺 A dweller, not a gadget
🚪 A door that knocks before entering
🔐 Hands that act only with your consent
⚡ A fully local voice, answered in seconds
```

## The trust contract

- **Nothing without your consent**: every new window triggers a knock (👀 Look / 🚫 Not now / 🔒 Sanctuary)
- **Sanctuaries are unbreakable**: permissions are enforced in Rust code (the broker), not in prompts — an LLM can never bypass them
- **Receipts**: everything it sees and does can be logged and audited

## Stack

- **Tauri + Rust** — ~33 MB RAM, no Python in the real-time loop
- **Avatar**: 2D egg (CSS) — a VRM 3D avatar may return later
- **Brain**: LLM-agnostic (trait → [Ollama](https://ollama.com) local by default; Gemini/OpenAI pluggable) — see `brain.json`
- **Voice**: [whisper.cpp](https://github.com/ggerganov/whisper.cpp) (local STT) + system voice (TTS)
- **Security**: per-window permission broker (capability-based)

## Structure

```
app/
├── src/egg/        # The egg: window, bubble, chat bar, voice, motion, access requests, intents
├── src/panel/      # Admin panel: one module per tab
└── src-tauri/src/  # Rust: broker (permissions + intent router), brain, voice, memory, vision, zones
docs/               # State of the art, concept, roadmap
```

## Run

```bash
cd app
npm install
npm run tauri dev
```

First run — download the local STT model (one time, ~148 MB):

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/setup-models.ps1
```

> **Voice**: the microphone streams to Gemini Live for transcription only. What you say goes through the same path as typed text (intent router → permission broker → memory → brain), and Live then reads the brain's answer aloud. The Gemini API key is stored in the Windows Credential Manager, never in a file.

## Status

Working MVP — see `docs/roadmap.md` for what's next (vision, advanced zones, house 🏠).


## Tests

```bash
cd app/src-tauri && cargo test   # broker, intent router, name cleaning
```

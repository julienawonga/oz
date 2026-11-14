# 🪺 Nid

> *A tiny dweller for your desktop. It knocks before entering.*

**Nid** is an open-source desktop AI companion: a 3D avatar (VRM) that lives on your screen, talks to you with a fully local voice stack, acts on your windows **only with your permission**, and respects the zones you forbid it to enter.

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
- **Avatar**: VRM 3D via [three-vrm](https://github.com/pixiv/three-vrm) (Three.js)
- **Brain**: LLM-agnostic (trait → [Ollama](https://ollama.com) local by default; Gemini/OpenAI pluggable) — see `brain.json`
- **Voice**: [whisper.cpp](https://github.com/ggerganov/whisper.cpp) (local STT) + system voice (TTS)
- **Security**: per-window permission broker (capability-based)

## Structure

```
app/
├── src/            # Frontend: 3D avatar, drag, hit-testing, door knock
└── src-tauri/      # Rust backend: broker, Win32 actions, providers, voice
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

> **Avatar model**: Nid loads any VRM avatar. Drop your `.vrm` file at `app/public/models/robot.vrm` (the file is not committed — pick one on [VRoid Hub](https://hub.vroid.com) respecting its license, or create one with [VRoid Studio](https://vroid.com/studio)).

## Status

Working MVP — see `docs/roadmap.md` for what's next (vision, advanced zones, house 🏠).

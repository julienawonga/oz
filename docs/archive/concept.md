# 🏠 Oz — Concept document

> Product brainstorming — "the agent harness with a body"

## One-line definition

> A dweller on your desktop. A house that holds its rules. A door that knocks before entering. Hands that act only with your consent. And a voice faster than anything else running locally.

## Positioning

**Oz is not a pet, it's an agent harness whose pet is the face.**

```
┌─────────────────────────────────────────────┐
│           OZ = HARNESS                     │
│  🧠 Brain (LLM-agnostic: Ollama,            │
│      Gemini Live, OpenAI...)                │
│  🛠️  Tool registry (windows, screen,        │
│      files, apps, voice)                    │
│  🔐 Permission broker      ← THE CORE       │
│  🐣 Body (3D VRM avatar) = the harness UI   │
└─────────────────────────────────────────────┘
```

## The 4 unique pillars (no existing project combines them)

1. **🗺️ Territory by window** — per-window permissions, not per screen region
2. **👁️ Vision = revocable privilege with receipts** — it asks before looking, everything is logged and auditable
3. **🧠 Spatial memory** — memories are tied to the windows/places where they happened
4. **⚡ Rust-native voice < 500 ms** — whisper.cpp + Ollama + kokoro, zero Python in the real-time loop

## The trust contract: HARD CONSENT by default

- It does **nothing** without explicit consent
- It asks **out loud** + bubble: "May I close Spotify?" → Yes / No / Always for this window
- **Silence = refusal** (15 s timeout → it politely gives up)
- Unhandled requests are stored in its journal, never re-pushed
- Permissions are enforced **in Rust code** (the broker), not in the prompt → a "wall with legs": it can never capture a sanctuary window even if the LLM asks
- Trust is earned window by window: 🚫 sanctuary → 👁️ look → 🤝 act (confirmed) → ✅ trusted

## 🏠 The house = a control center disguised as a cozy home

| Element | Functional role |
|---|---|
| 🛏️ Bed | It sleeps there when idle; you can "whisper" a task for later |
| 📔 Journal | Receipts: auditable history of everything it saw/did |
| 🖼️ Album | Spatial memory: memories tied to windows/places |
| 🍽️ Fridge | Resources: API keys, Ollama models, LLM config |
| 🚪 Door | The per-window permissions board — granted/denied at first contact ("it knocks") |

The house **grows with the relationship**: new tool discovered → new furniture; window learned → photo in the album. Care IS gamified security.

## Agent actions (harness tools)

- 📋 List open windows/apps
- ❌ Close a window (confirmation required)
- 🎯 Bring an app to the foreground
- 📸 Look into an allowed window (capture + vision LLM)
- 🚀 Launch an application
- 📝 Write to files (notes, Obsidian)

Action stack: Win32 API via `windows-rs` (EnumWindows, SetForegroundWindow, WM_CLOSE...), the permission broker filters every call.

## Mood & care (functional, not decorative)

- Its curiosity = social energy gauge: idle too long → it gets bored, takes a nap
- Mood affects the tone of its answers
- Caring for it = furnishing its house, filling its fridge, its album

## Decided tech stack

- **Tauri + Rust** (lightweight, ~33 MB RAM, proof: BongoCat 24k ⭐)
- **Avatar: VRM in Three.js** via `pixiv/three-vrm` (emotions, look-at, viseme lip-sync)
- **LLM-agnostic**: Rust `Provider` trait → Ollama (local default), Gemini Live (real-time), OpenAI/Anthropic
- **Local voice**: whisper.cpp (STT) → Provider → kokoro-onnx or piper (TTS), target < 500 ms
- **Overlay**: transparent + always_on_top + skip_taskbar + `set_ignore_cursor_events` window
- **Security**: Rust permission broker, applied to every tool call

## MVP (build order)

1. Tauri overlay + draggable avatar
2. Per-window permission broker + "door knock"
3. 3 tools: list windows, focus, close (confirmed) → already useful
4. Local voice < 1 s (STT + Ollama + TTS)
5. On-demand window vision
6. House (journal, door, album) + mood

## State-of-the-art references

See `state-of-the-art.md` (AIRI 51k⭐, Open-LLM-VTuber 14.7k⭐, B 5.2k⭐, BongoCat 24.6k⭐, three-vrm, piper, kokoro...).

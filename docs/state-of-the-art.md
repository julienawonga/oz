# 📋 State of the art — Desktop AI companion ("Nid")

> Research via GitHub API / official READMEs — November 2025

---

## 1. The landscape in one sentence

The concept already exists and is **very active** (several projects above 10,000 ⭐), but **no major project combines**: lightweight Tauri/Rust + 3D VRM avatar + LLM-agnostic + restricted-access zones. There is real room to take.

---

## 2. Classic desktop pets (no AI)

| Project | ⭐ | Stack | What we learn from it |
|---|---|---|---|
| **BongoCat** (ayangweb) | 24,620 | **Tauri** | Proof #1 that Tauri makes excellent cross-platform pets (Win/Mac/Linux), performant transparent overlay |
| **Shimeji-ee / Shijima** | ~2,000 | Java / Web | Behaviors defined in XML (walk, climb, fall) → basis of "zones" logic |
| **VPet-Simulator** | ~3,000 | C# | State animations, needs bars |
| **Desktop Goose / eSheep** | ~7,000 | C# / C | Drag & drop and "liveliness" above all |

**Takeaway**: transparent always-on-top overlay rendering is a **solved** problem, including in Tauri.

---

## 3. AI desktop companions (the core of the subject)

### 🥇 AIRI (moeru-ai) — 51,400 ⭐
- TypeScript, self-hosted, "souls of waifu", aiming for a Neuro-sama-like
- **Real-time voice**, can play Minecraft/Factorio, Web/Win/Mac
- The biggest in the field. Limit: no heavy VRM avatar on the desktop, rather web-page/container oriented

### 🥈 Open-LLM-VTuber — 14,700 ⭐
- Python + web frontend, **transparent desktop pet mode** explicitly supported
- Real-time voice conversation + **visual perception**, fully offline capable
- Modular pipeline: ASR (faster-whisper) → LLM multi-provider → TTS
- Technically the closest to our spec (in Python)

### 🥉 B — "The Soul-Wired Desktop Companion" — 5,200 ⭐
- Python/PyQt6, **semantic screen vision**, active window tracking
- 60 fps event bus architecture, emotional states, **proactive** ("only intervenes when it has something valuable to say")
- Glassmorphic overlay, local ONNX synthesized voice
- The closest to our "watches my screen if I allow it" vision

### Other notable references

| Project | ⭐ | Highlights |
|---|---|---|
| **Soul-of-Waifu** | 1,458 | **Live2D AND VRM** avatars, voice + local LLM, persistent identity |
| **Amica** (semperai) | 1,626 | Open-source web interface for 3D VRM + STT/TTS, great reference code for three-vrm |
| **my-neuro** | 1,454 | Voice response in **1 second**, long-term memory, voice cloning |
| **LiraVtuber** | 27 | Multi-provider LLM **and** TTS, hybrid memory, desktop tooling |
| **CoPet / tiny-roommate / neoxider-agent-deck** | 2-46 | 2025 trend: the pet that **reacts to your AI agents** (Claude Code, Cursor...) |
| **puck-mac/windows/linux** | 1 | Pet = **AI agent** acting on the OS with an *approval gate* (Linux port in **Rust + GTK4**) |

---

## 4. Validated technical building blocks

### 3D rendering (VRM)
- **`pixiv/three-vrm`** (2,290 ⭐): THE standard for loading VRM in Three.js. Emotion blend shapes, look-at, spring bones for hair/clothes.
- **`pixi-live2d-display`** (1,517 ⭐): the Live2D alternative.
- Lip-sync: viseme analysis or audio amplitude → VRM "aa" blend shape. Doable without an extra model.

### Voice — 3 observed strategies
| Strategy | Tools | Latency | Cost |
|---|---|---|---|
| **Local -stage pipeline** | whisper.cpp (STT) → LLM → piper (11,300 ⭐) or kokoro-onnx (2,900 ⭐) | 1-3 s | Free |
| **Real-time cloud voice** | **Gemini Live API** (native bidirectional audio), OpenAI Realtime | < 500 ms | Paid |
| **Hybrid** | Local STT + LLM-agnostic + TTS of choice | ~1 s | Flexible |

→ The standout projects (my-neuro at 1 s, AIRI) use either chunk streaming or native real-time. The "provider-agnostic" architecture with a Rust trait supports both.

### Tauri overlay (Windows)
Solved configuration: `transparent + decorations: false + always_on_top + skip_taskbar` window, and **click-through of empty zones** via `set_ignore_cursor_events()`. BongoCat proves it in production with 24k+ users.

---

## 5. Gap analysis → our differentiation space

1. **Allowed / forbidden zones**: almost no project implements it (Shimeji have screen edges, not user-defined restriction zones). This idea is **original**.
2. **Explicit screen-vision consent**: B "watches" continuously (with focus-state respect), Open-LLM-VTuber on demand — but nobody offers **granular privacy control** (toggle per zone/app, a virtual physical button).
3. **Native Rust/Tauri stack**: the big projects are Python (OLV, B) or TypeScript (AIRI). A Tauri + Rust backend with a `Provider` trait (Ollama / Gemini Live / OpenAI...) is **lighter than anything existing**.
4. **"Max" 3D VRM on the desktop**: Soul-of-Waifu and Amica do it in VRM, but never combined with everything else.

---

## 6. Conclusion

✅ Feasibility confirmed by 3 independent major projects (AIRI, OLV, B).
✅ All building blocks exist in open source and are proven.
🎯 Our proposal stands apart on: lightness (Tauri/Rust), strict LLM-agnosticism, controlled access zones, and privacy-first screen vision.

**Main references**:
- https://github.com/moeru-ai/airi
- https://github.com/Open-LLM-VTuber/Open-LLM-VTuber
- https://github.com/AhmadHassan-BTed/B
- https://github.com/ayangweb/BongoCat
- https://github.com/pixiv/three-vrm
- https://github.com/jofizcd/Soul-of-Waifu
- https://github.com/semperai/amica
- https://github.com/rhasspy/piper · https://github.com/thewh1teagle/kokoro-onnx

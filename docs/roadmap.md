# 🗺️ Nid — Roadmap

## Concept pillars (validated during brainstorming)

1. 🗺️ Territory by window (permissions)
2. 👁️ Vision = revocable privilege, with receipts
3. 🧠 Spatial memory
4. ⚡ Fast local voice

## Progress — November 2025

### Infrastructure
- [x] Tauri + Rust operational (Rust 1.100, MSVC Build Tools, Windows SDK, CMake, LLVM)
- [x] Ollama installed, LFM3-1.7B wired in (provider-agnostic via `brain.json`)
- [x] Test VRM model in place

### Step 1 — The body ✅
- [x] Transparent overlay, always-on-top, no taskbar
- [x] Pixel-perfect DPI-aware drag (no limits)
- [x] Pixel-perfect click-through: clickable area hugs the silhouette (3D raycast, 80 Hz)

### Step 2 — The avatar ✅
- [x] 3D VRM via three-vrm: gaze follows global cursor, blinking, breathing, arms/forearms/hands animated as a phase chain
- [ ] Final VRM model of user's choice (one file to swap)

### Step 3 — The harness (broker) ✅ (almost)
- [x] Win32 window enumeration + system noise filtering
- [x] Door knock (new window → 👀 / 🚫 / 🔒)
- [x] Persisted permissions (`permissions.json`), enforced in Rust code
- [x] Actions: list / focus / close — via deterministic intent router
- [x] Sanctuaries never revealed (not even to `list`)
- [x] Brain: LFM3-1.7B + `think:false` (~3 s per response)
- [ ] "🤝 Act" button at the door (grant act level directly)
- [ ] Response streaming (word by word in the bubble)
- [ ] Window titles: cleanup polish (minor, postponed)

### Step 4 — The voice ✅ (MVP)
- [x] Microphone → whisper.cpp local (ggml-base FR) → transcription
- [x] Spoken response: French system voice (Hortense/SAPI)
- [x] Mouth animation while speaking
- [ ] Nicer TTS (kokoro local) or Gemini Live (voice provider)
- [ ] Automatic speech detection (VAD, no button)
- [ ] Latency: whisper `small` if accuracy is insufficient

### Step 5 — The vision ⬜ (upcoming)
- [ ] Look into an allowed window on demand ("look at my editor")
- [ ] Capture + vision LLM (LFM3-VL or cloud)
- [ ] Receipts: log of what was seen and why

### Step 6 — Advanced zones ⬜
- [ ] Permanent mode per window + per screen
- [ ] Robot visually bounced/refused in forbidden zones

### Step 7 — The house 🏠 ⬜
- [ ] Physical house on the desktop (bed, journal, album, fridge, door)
- [ ] Journal = auditable receipts of actions/vision
- [ ] Album = spatial memory (memories tied to windows)
- [ ] Fridge = resource management (models, keys)
- [ ] Gamified mood/trust (earned furniture)
- [ ] **Memory lifecycle inspired by AutoHarness** (learn / merge / prune): memories are distilled from real sessions, duplicates fold into one, unused ones die — the library stays clean on its own
- [ ] **Per-memory ledger**: each creation/update logs its scenario and decision
- [ ] **Session-open index**: every session starts with a recall of what Nid knows (memory is useless if never surfaced)

## Key technical decisions
- **Tauri/Rust**: 33 MB RAM, real-time loop with no Python
- **Provider-agnostic**: `brain.json` (provider/model/url) — Gemini Live ready to plug
- **Capability-based security**: the Rust broker filters every action; the LLM decides nothing
- **Deterministic router**: actions go through Rust keywords, not the LLM (3B reliability)

## Related projects to watch
- **[AutoHarness](https://github.com/tigerless-labs/autoharness)** (11k ⭐): self-learning skill layer for Claude Code. Validates our core thesis ("the harness does much of the work": same model 42% → 78% on CORE-Bench by changing the harness). Not a competitor (headless, Linux/macOS, no body/voice/permissions) — but its skill lifecycle (learn/merge/prune), per-skill ledger and session-open index are the blueprint for Nid's memory system (step 7).

## Models available in Ollama
- `lfm3:latest` ← current (1.7B q4, ~3 s/response)
- `lfm3-local` (BF16 imported from HF cache — slow on CPU)
- `lfm2.5:latest` (8B-A1B, more knowledge, 8 s)
- `llama3.2:3b` (fallback)

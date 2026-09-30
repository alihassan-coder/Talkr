# Talkr

Free, private, offline desktop app for text-to-speech and speech-to-text. Run Whisper and Kokoro locally on your GPU or CPU. No account, no cloud, no telemetry.

## Features

- **Speak (Text → Speech):** Type or paste text, pick a voice, generate natural audio. Play it, save as WAV.
- **Transcribe (Speech → Text):** Record from microphone or drop audio files (MP3, WAV, FLAC, OGG, M4A). Get text with timestamps.
- **Model Library:** Browse open-source models, download with one click, delete anytime.
- **100% Local:** Everything runs on your machine. Speech to text uses the GPU (Vulkan on Windows/Linux, Metal on macOS) and falls back to the CPU on its own.
- **Crash-proof engines:** Whisper and sherpa-onnx run in a separate process. If a model runs out of memory or a GPU driver fails, the app stays open, explains what happened and restarts the engine.
- **Low-memory friendly:** compressed (quantized) Whisper models and a free-memory check before loading.
- **History:** Unlimited searchable history with favorites, export (TXT, SRT, WAV).

## Quick Start

### Prerequisites

- **Node.js 20+** (use `nvm` or `fnm`)
- **pnpm 9+** (`corepack enable pnpm`)
- **Rust stable** (`rustup default stable`)
- **System dependencies:**
  - **Linux:** `libwebkit2gtk-4.1-dev libayatana-appindicator3-dev librsvg2-dev libssl-dev libudev-dev`
  - **macOS:** Xcode Command Line Tools
  - **Windows:** Visual Studio Build Tools

### Development

```bash
# Install dependencies
pnpm install

# Run desktop app (dev mode)
pnpm dev:desktop

# Run web landing page
pnpm dev:web

# Type checking
pnpm typecheck

# Linting
pnpm lint

# Build everything
pnpm build
```

### Tests

```bash
pnpm test                                   # all JS unit tests (desktop + web, Vitest)
pnpm --filter web test:e2e                  # website end-to-end (Playwright)
pnpm --filter @talkr/model-catalog validate --offline

cd apps/desktop
node scripts/build-engine.mjs --debug       # the engine sidecar (the app's build needs it)
node scripts/fetch-test-models.mjs ../../.test-models   # optional: real-model tests
cd src-tauri
TALKR_TEST_MODELS=../../../.test-models cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

On Windows, put `src-tauri/target/debug` on `PATH` for the tests (sherpa-onnx DLLs); on Linux,
`LD_LIBRARY_PATH`. CI runs all of this on Windows, macOS and Linux, plus the Vulkan engine
against a software GPU, on every push and before every release.

### Building Installers

```bash
cd apps/desktop
pnpm tauri build    # builds the engine sidecars too (scripts/build-engine.mjs)
# Output in apps/desktop/src-tauri/target/*/release/bundle/
```

The GPU (Vulkan) engine is built when the Vulkan SDK is installed (`VULKAN_SDK`); otherwise the
app ships CPU-only and says so in Settings. Pushing a `v*` tag builds and drafts a release on
GitHub, after the full test suite passes.

## Architecture

```
Talkr app (Tauri, React UI)  ──JSON lines over stdin/stdout──▶  talkr-engine (sidecar process)
  commands, downloads, history,                                  whisper.cpp  (speech to text)
  engine_host.rs (supervisor)                                    sherpa-onnx  (text to speech)
```

- `engine_host.rs` starts the engine on demand, stops it after 5 idle minutes to free memory,
  turns a crash or out-of-memory kill into a readable message, and restarts it.
- A job that crashes the GPU engine is retried on the CPU, and the GPU stays off until the
  compute setting changes or Talkr updates.
- `talkr-engine-gpu` (Vulkan) ships next to the CPU `talkr-engine` on Windows and Linux; on
  macOS one engine drives Metal.

## Project Structure

```
talkr/
├── apps/
│   ├── desktop/          # Tauri v2 + React app
│   │   ├── src/          # React frontend
│   │   └── src-tauri/    # Rust backend (app crate)
│   │       ├── engine/   # talkr-engine: whisper.cpp + sherpa-onnx process
│   │       └── protocol/ # messages between app and engine
│   └── web/              # Next.js landing page
├── packages/
│   ├── ui/               # Shared design system (React + Tailwind)
│   ├── model-catalog/    # Model definitions (single source of truth)
│   └── config/           # Shared TS/ESLint config
└── .github/workflows/    # CI/CD
```

## Data Directory

All data lives in `~/.talkr/` (or `%USERPROFILE%\.talkr\` on Windows):

```
~/.talkr/
├── config.json           # User settings
├── models/
│   ├── stt/              # Whisper models
│   └── tts/              # Kokoro/Piper models
├── audio/                # Generated speech & recordings
├── history/
│   └── talkr.db          # SQLite history
├── cache/downloads/      # Resumable downloads
└── logs/                 # App logs
```

## Model Catalog

Models defined in `packages/model-catalog/catalog.json`. Validate with:

```bash
pnpm catalog:validate
pnpm catalog:validate:fill  # Fetch real SHA256/sizes
```

### Included Models

**STT (Whisper.cpp GGML):**
- Tiny/Base/Small/Medium/Large-v3-turbo (English & Multilingual), plus compressed q5 versions for 4 GB machines

**TTS (Sherpa-onnx):**
- Kokoro Multi-language / English
- Piper voices (English US/UK, German, French)

Every download is checked against the SHA-256 in the catalog, from a pinned URL.

## Tech Stack

- **Frontend:** React 19, TypeScript, Vite, Tailwind CSS, Zustand, React Router
- **Backend:** Rust, Tauri v2, Tokio, Rusqlite, Reqwest
- **Audio:** CPAL (recording), Symphonia (decoding), Rubato (streaming resampling), Hound (WAV)
- **STT:** whisper-rs (whisper.cpp bindings)
- **TTS:** sherpa-rs (Kokoro, Piper/VITS)
- **Monorepo:** pnpm workspaces, Turborepo

## License

MIT License - see [LICENSE](LICENSE) for details.

Models have their own licenses (MIT, Apache-2.0, GPL-3.0 for espeak-ng-data) — displayed in-app before download.
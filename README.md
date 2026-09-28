# Talkr

Free, private, offline desktop app for text-to-speech and speech-to-text. Run Whisper and Kokoro locally on your GPU or CPU. No account, no cloud, no telemetry.

## Features

- **Speak (Text → Speech):** Type or paste text, pick a voice, generate natural audio. Play it, save as WAV.
- **Transcribe (Speech → Text):** Record from microphone or drop audio files (MP3, WAV, FLAC, OGG, M4A). Get text with timestamps.
- **Model Library:** Browse open-source models, download with one click, delete anytime.
- **100% Local:** Everything runs on your machine. GPU acceleration (Metal/CUDA/Vulkan) with CPU fallback.
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

### Building Installers

```bash
# Build desktop installers (requires Rust toolchain for target platforms)
pnpm build
# Output in apps/desktop/src-tauri/target/*/release/bundle/
```

## Project Structure

```
talkr/
├── apps/
│   ├── desktop/          # Tauri v2 + React app
│   │   ├── src/          # React frontend
│   │   └── src-tauri/    # Rust backend
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
- Tiny/Base/Small/Medium/Large-v3-turbo (English & Multilingual)

**TTS (Sherpa-onnx):**
- Kokoro Multi-language / English
- Piper voices (English US/UK, German, French)

## Tech Stack

- **Frontend:** React 19, TypeScript, Vite, Tailwind CSS, Zustand, React Router
- **Backend:** Rust, Tauri v2, Tokio, Rusqlite, Reqwest
- **Audio:** CPAL (recording), Symphonia (decoding), Rubato (resampling), Hound (WAV)
- **STT:** whisper-rs (whisper.cpp bindings)
- **TTS:** sherpa-rs (Kokoro, Piper/VITS)
- **Monorepo:** pnpm workspaces, Turborepo

## License

MIT License - see [LICENSE](LICENSE) for details.

Models have their own licenses (MIT, Apache-2.0, GPL-3.0 for espeak-ng-data) — displayed in-app before download.
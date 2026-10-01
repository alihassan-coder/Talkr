# Talkr

> Speech tools that never phone home.

[![MIT License](https://img.shields.io/badge/license-MIT-f5f5f5.svg)](LICENSE)
[![CI](https://github.com/alihassan-coder/Talkr/actions/workflows/ci.yml/badge.svg)](https://github.com/alihassan-coder/Talkr/actions/workflows/ci.yml)
[![Website](https://img.shields.io/badge/website-visit-8b5cf6.svg)](https://talkr-three.vercel.app/)

Talkr is a free, open-source desktop app for turning speech into text and text into speech. Whisper, Kokoro, and Piper run on your own hardware—no account, API key, cloud upload, or monthly bill.

**[Visit the website](https://talkr-three.vercel.app/) · [Download Talkr](https://talkr-three.vercel.app/download) · [Report a bug](https://github.com/alihassan-coder/Talkr/issues/new?template=bug_report.yml)**

## Why Talkr?

- 🔒 **Private by design** — audio, text, and history stay on your computer.
- 🎙️ **Two-way speech tools** — transcribe recordings or create natural-sounding audio.
- ⚡ **Hardware aware** — uses Vulkan or Metal when available and falls back to the CPU.
- 🧠 **Bring your own models** — choose from the built-in catalog or import local models.
- 🧰 **Useful exports** — save transcripts as TXT/SRT and generated speech as WAV.
- 🆓 **Actually open source** — MIT licensed, with no paid tier or telemetry.

## Get Talkr

Grab the latest installer for Windows, macOS, or Linux from the **[download page](https://talkr-three.vercel.app/download)**. Each release includes SHA-256 checksums so you can verify what you downloaded.

Talkr works offline after you download a model. It only connects when you ask it to download a model, and once at startup to check GitHub Releases for a new version.

**Updates.** When a new version is out, Talkr shows a small banner; *Install and restart* downloads it, verifies its signature, and restarts into the new version. Nothing is installed without that click. (On Windows the installer uses the WebView2 runtime that ships with Windows 10 and 11; on an older system without it, the installer downloads it once.)

## Releases

Releases are cut from tags, in this order:

1. Bump the version everywhere (`node scripts/check-versions.mjs` lists the files and must pass), merge to `master`.
2. Push the tag: `git tag v0.1.5 && git push origin v0.1.5`. The release workflow runs CI, then builds and signs installers for every platform into a **draft** GitHub Release, together with `latest.json` for the updater and SHA-256 checksums.
3. Check the draft, then press **Publish**. Only now do installed copies see the update (they read `releases/latest/download/latest.json`).
4. Only then deploy the website (Vercel). Its download links point at this version's files, which do not exist until the release is published.

## Run from source

You will need [Node.js 20+](https://nodejs.org/), [pnpm 9+](https://pnpm.io/), [Rust](https://rustup.rs/), and the [Tauri system dependencies](https://v2.tauri.app/start/prerequisites/) for your platform.

```bash
git clone https://github.com/alihassan-coder/Talkr.git
cd Talkr
corepack enable
pnpm install
pnpm dev:desktop
```

To run only the website:

```bash
pnpm dev:web
```

## Under the hood

```text
Tauri + React app ── JSON lines ──▶ isolated Rust engine
     UI & history                  whisper.cpp · sherpa-onnx
```

The speech engine runs as a sidecar process, so a model or GPU failure does not take down the app. Talkr can retry work on the CPU and keeps all app data in `~/.talkr` (or `%USERPROFILE%\.talkr` on Windows).

This monorepo contains:

- `apps/desktop` — Tauri desktop app and Rust speech engine
- `apps/web` — Next.js website
- `packages/ui` — shared React components
- `packages/model-catalog` — model metadata and checksums
- `packages/config` — shared TypeScript and ESLint configuration

Found a security issue? Please report it privately, see [SECURITY.md](SECURITY.md).

## Contributing

Ideas, bug reports, docs, and code are all welcome. Read [CONTRIBUTING.md](CONTRIBUTING.md) to get started and [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md) for the community ground rules.

If Talkr saves you a trip to the cloud, consider leaving a ⭐. It helps more people find the project.

## License

Talkr is available under the [MIT License](LICENSE). Downloadable models and their data keep their own licenses, which Talkr shows before download.

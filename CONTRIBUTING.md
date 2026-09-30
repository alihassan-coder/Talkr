# Contributing to Talkr

Thanks for helping Talkr speak a little clearer. 🎙️

Bug fixes, features, documentation, translations, design ideas, and thoughtful feedback are all welcome. You do not need to be a speech-AI expert.

## Before you start

- Search the [existing issues](https://github.com/alihassan-coder/Talkr/issues) first.
- For a bug, include your OS, Talkr version, hardware, model, and steps to reproduce it.
- For a large change, open an issue before writing code so we can agree on the direction.
- Never include private audio, transcripts, model files, tokens, or other secrets in an issue.

## Local setup

Install Node.js 20+, pnpm 9+, Rust stable, and the [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/) for your operating system.

```bash
git clone https://github.com/alihassan-coder/Talkr.git
cd Talkr
corepack enable
pnpm install
pnpm dev:desktop
```

Useful commands:

```bash
pnpm typecheck
pnpm lint
pnpm test
pnpm build
pnpm catalog:validate
pnpm --filter web test:e2e
```

Rust lives in `apps/desktop/src-tauri`. Run its checks from that directory:

```bash
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

The full engine tests need native libraries and test models; CI runs them on Windows, macOS, and Linux.

## Pull requests

1. Create a focused branch from `main`.
2. Keep the change small and explain the _why_, not just the _what_.
3. Add or update tests when behavior changes.
4. Run the relevant checks and mention anything you could not run.
5. Update docs when users or contributors will notice the change.

By contributing, you agree that your work will be released under Talkr's [MIT License](LICENSE). Please follow our [Code of Conduct](CODE_OF_CONDUCT.md), be kind in review, and remember: behind every GitHub avatar is a human (probably holding a slightly cold coffee).

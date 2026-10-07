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

### Building installers

You don't need a powerful computer: the **Test build** workflow builds installers for Windows, macOS and Linux on GitHub's machines. Open the repository's **Actions** tab, pick **Test build**, press **Run workflow**, choose the platforms, and download the installers from the finished run's **Artifacts** (kept for 7 days). It uses the same steps as a release, so a green test build means the release will build too.

To build on your own machine instead: release builds create signed updater artifacts (`bundle.createUpdaterArtifacts`), which needs the project's private signing key. Without it, `pnpm build:desktop` stops with a signing error; turn the updater artifacts off for a local build instead:

```bash
pnpm --filter desktop tauri build --config '{"bundle":{"createUpdaterArtifacts":false}}'
```

The Tauri plugins exist twice, as Rust crates (`apps/desktop/src-tauri/Cargo.toml`) and npm packages (`apps/desktop/package.json`). Keep each pair on the same major.minor version and update them together; `node scripts/check-versions.mjs` checks the locked versions.

Rust is pinned in `apps/desktop/src-tauri/rust-toolchain.toml` (rustup installs it on first use). To move to a newer Rust, change it there and in the `toolchain:` lines of `.github/workflows/*.yml` in one PR, and fix any new clippy warnings in the same PR.

## Branches

- `master` is the latest stable release. It only moves when a release is merged into it.
- `dev` is where work lands first. Betas are tagged from it, and it is merged into `master` for each stable release.

CI runs on every push and pull request to either branch.

## Releasing

Maintainers only. The order matters, because installed copies and the website both follow the release:

1. Move the **Unreleased** notes in `CHANGELOG.md` under a `## X.Y.Z - date` heading. They become the release notes and the "What's new" text the app shows with the update; the release stops if the section is missing (`node scripts/release-notes.mjs X.Y.Z` prints it).
2. Bump the version in `apps/desktop/src-tauri/tauri.conf.json`, `apps/desktop/package.json`, the three `Cargo.toml` files under `apps/desktop/src-tauri` and `apps/web/lib/releases.ts` (update the download sizes there after the build). `node scripts/check-versions.mjs` must pass; CI runs it too.
3. Merge `dev` into `master` and let CI finish, then tag and push: `git tag vX.Y.Z && git push origin vX.Y.Z`. The tag must equal `v` + the version, or the release workflow stops.
4. The workflow reuses that commit's CI run when every job passed (otherwise it runs CI itself), then builds, signs (`TAURI_SIGNING_PRIVATE_KEY` secret) and uploads every installer, its updater signature, `latest.json` and `SHA256SUMS-*.txt` to a **draft** release.
5. Test the draft's installers, then **Publish** it. Publishing makes `releases/latest/download/latest.json` point at it, which is when installed copies offer the update.
6. Deploy the website last. Its links point at `releases/download/vX.Y.Z/…`, which 404 while the release is still a draft.

### Betas

A beta lets testers try new features before a stable release. Its version is `X.Y.Z-N`, the next stable version with a beta number after a dash, e.g. `0.1.8-1`, then `0.1.8-2`. The part after the dash must be a plain number, because Windows' MSI installer format does not accept names like `-beta.1`.

1. On `dev`, add a `## X.Y.Z-N - date` section to `CHANGELOG.md` and bump the same files as for a release, except `apps/web/lib/releases.ts`. The website stays on the last stable version, and `node scripts/check-versions.mjs` enforces that.
2. Let CI pass on `dev`, then tag the `dev` commit and push the tag: `git tag vX.Y.Z-N && git push origin vX.Y.Z-N`.
3. The workflow builds a **draft pre-release**. Test it, then **Publish** it. A pre-release never becomes `releases/latest`, so installed copies are not offered it and the website ignores it. Testers download it from the Releases page.
4. People on a beta are offered the stable `X.Y.Z` when it ships, because `X.Y.Z` counts as newer than `X.Y.Z-N`.

## Pull requests

1. Create a focused branch from `dev` and open the pull request against `dev`.
2. Keep the change small and explain the _why_, not just the _what_.
3. Add or update tests when behavior changes.
4. Run the relevant checks and mention anything you could not run.
5. Update docs when users or contributors will notice the change.

By contributing, you agree that your work will be released under Talkr's [MIT License](LICENSE). Please follow our [Code of Conduct](CODE_OF_CONDUCT.md), be kind in review, and remember: behind every GitHub avatar is a human (probably holding a slightly cold coffee).

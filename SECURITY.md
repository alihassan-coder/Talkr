# Security policy

## Supported versions

Only the latest release of Talkr gets security fixes. Installed copies offer new versions on their own (see the README), so updating is one click.

## Reporting a vulnerability

Please **do not** open a public issue for a security problem.

Report it privately through GitHub: [Report a vulnerability](https://github.com/alihassan-coder/Talkr/security/advisories/new) (the *Security* tab of the repository, then *Report a vulnerability*). Include:

- the Talkr version and your operating system,
- what an attacker can do, and the steps or a proof of concept to reproduce it,
- any idea you have for a fix.

You will get a reply within a week. Once a fix is released, the advisory is published with credit to you, unless you would rather stay anonymous.

## Scope

In scope: the desktop app (`apps/desktop`), its speech engine, the update and model download paths (signature and checksum checks), and the website (`apps/web`).

Out of scope: vulnerabilities in the speech models themselves or in third-party projects Talkr bundles (whisper.cpp, sherpa-onnx, Tauri) — please report those upstream, and tell us if Talkr needs to ship an update.

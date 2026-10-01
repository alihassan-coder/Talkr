#!/usr/bin/env node
// Every place that carries Talkr's version must agree, and a release tag (vX.Y.Z) must match it.
// The updater compares the running app's version with latest.json, and the website links to
// files named after the version, so a mismatch ships a broken release.
//
//   node scripts/check-versions.mjs
//
// In GitHub Actions, GITHUB_REF_NAME is the tag on tag pushes; it is only checked when it looks
// like a release tag (starts with "v").
import { readdirSync, readFileSync } from 'node:fs'
import { fileURLToPath } from 'node:url'

const root = fileURLToPath(new URL('..', import.meta.url))
const read = (path) => readFileSync(new URL(path, new URL('..', import.meta.url)), 'utf8')

/** The `version` of a Cargo.toml's [package] table (not a dependency's). */
function cargoVersion(path) {
  const text = read(path)
  const pkg = text.match(/^\[package\]\s*$([\s\S]*?)(?=^\[)/m)
  const version = pkg?.[1].match(/^version\s*=\s*"([^"]+)"/m)?.[1]
  if (!version) throw new Error(`${path}: no [package] version`)
  return version
}

function webVersion(path) {
  const version = read(path).match(/^export const VERSION = '([^']+)'/m)?.[1]
  if (!version) throw new Error(`${path}: no \`export const VERSION = '...'\``)
  return version
}

const sources = {
  'apps/desktop/src-tauri/tauri.conf.json': JSON.parse(read('apps/desktop/src-tauri/tauri.conf.json')).version,
  'apps/desktop/package.json': JSON.parse(read('apps/desktop/package.json')).version,
  'apps/desktop/src-tauri/Cargo.toml': cargoVersion('apps/desktop/src-tauri/Cargo.toml'),
  'apps/desktop/src-tauri/engine/Cargo.toml': cargoVersion('apps/desktop/src-tauri/engine/Cargo.toml'),
  'apps/desktop/src-tauri/protocol/Cargo.toml': cargoVersion('apps/desktop/src-tauri/protocol/Cargo.toml'),
  'apps/web/lib/releases.ts': webVersion('apps/web/lib/releases.ts'),
}

const errors = []
const expected = sources['apps/desktop/src-tauri/tauri.conf.json']
if (!/^\d+\.\d+\.\d+$/.test(expected ?? '')) errors.push(`tauri.conf.json version "${expected}" is not X.Y.Z`)
for (const [file, version] of Object.entries(sources)) {
  if (version !== expected) errors.push(`${file} has ${version}, tauri.conf.json has ${expected}`)
}

// `tauri build` refuses to run when a Tauri npm package and its Rust crate differ in major.minor
// (it cost the v0.1.3 release a full build matrix). Compare the locked versions up front.
const lockedCrates = new Map(
  [...read('apps/desktop/src-tauri/Cargo.lock').matchAll(/^name = "(tauri(?:-plugin-[a-z-]+)?)"\r?\nversion = "([^"]+)"/gm)].map(
    (m) => [m[1], m[2]],
  ),
)
const lockedNpm = new Map(
  [
    ...read('pnpm-lock.yaml').matchAll(
      /^\s+'(@tauri-apps\/(?:api|plugin-[a-z-]+))':\r?\n\s+specifier: [^\n]+\r?\n\s+version: (\d+\.\d+\.\d+)/gm,
    ),
  ].map((m) => [m[1], m[2]]),
)
const majorMinor = (v) => v.split('.').slice(0, 2).join('.')
for (const [pkg, npmVersion] of lockedNpm) {
  const crate = pkg === '@tauri-apps/api' ? 'tauri' : `tauri-${pkg.slice('@tauri-apps/'.length)}`
  const crateVersion = lockedCrates.get(crate)
  if (crateVersion && majorMinor(crateVersion) !== majorMinor(npmVersion)) {
    errors.push(`${pkg} ${npmVersion} and the ${crate} crate ${crateVersion} must share major.minor`)
  }
}
if (!lockedNpm.has('@tauri-apps/api') || !lockedCrates.has('tauri')) {
  errors.push('could not find @tauri-apps/api in pnpm-lock.yaml or tauri in Cargo.lock')
}

// Every workflow installs the Rust version pinned in rust-toolchain.toml.
const toolchain = read('apps/desktop/src-tauri/rust-toolchain.toml').match(/^channel = "([^"]+)"/m)?.[1]
if (!toolchain) errors.push('apps/desktop/src-tauri/rust-toolchain.toml: no channel')
for (const workflow of readdirSync(new URL('.github/workflows/', new URL('..', import.meta.url)))) {
  for (const [, version] of read(`.github/workflows/${workflow}`).matchAll(/^\s+toolchain: (\S+)/gm)) {
    if (version !== toolchain) errors.push(`.github/workflows/${workflow} installs Rust ${version}, rust-toolchain.toml pins ${toolchain}`)
  }
}

const ref = process.env.GITHUB_REF_NAME ?? ''
if (ref.startsWith('v') && ref !== `v${expected}`) {
  errors.push(`tag ${ref} does not match the app version (expected v${expected})`)
}

if (errors.length) {
  console.error(`Version mismatch in ${root}:\n${errors.map((e) => `  - ${e}`).join('\n')}`)
  console.error('\nBump every file listed above to the same version.')
  process.exit(1)
}
console.log(`All versions are ${expected}${ref.startsWith('v') ? ` and match tag ${ref}` : ''}.`)
console.log(`Tauri packages match their crates: ${[...lockedNpm.keys()].join(', ')}.`)

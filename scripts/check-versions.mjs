#!/usr/bin/env node
// Every place that carries Talkr's version must agree, and a release tag (vX.Y.Z) must match it.
// The updater compares the running app's version with latest.json, and the website links to
// files named after the version, so a mismatch ships a broken release.
//
//   node scripts/check-versions.mjs
//
// In GitHub Actions, GITHUB_REF_NAME is the tag on tag pushes; it is only checked when it looks
// like a release tag (starts with "v").
import { readFileSync } from 'node:fs'
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

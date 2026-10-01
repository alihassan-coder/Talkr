#!/usr/bin/env node
// Print the CHANGELOG.md section of a version: the release page shows it, and installed copies
// show it as "What's new" when they offer the update (tauri-action puts it in latest.json).
//
//   node scripts/release-notes.mjs [X.Y.Z]     (default: the version in tauri.conf.json)
//
// Fails when CHANGELOG.md has no section for the version, so a release never ships without notes.
import { readFileSync } from 'node:fs'

const read = (path) => readFileSync(new URL(path, new URL('..', import.meta.url)), 'utf8')

const version = process.argv[2] ?? JSON.parse(read('apps/desktop/src-tauri/tauri.conf.json')).version
const changelog = read('CHANGELOG.md')

// "## 1.2.3", "## [1.2.3]" or "## v1.2.3", optionally followed by a date.
const heading = new RegExp(`^## \\[?v?${version.replace(/\./g, '\\.')}\\]?(?:\\s.*)?$`, 'm')
const match = heading.exec(changelog)
const rest = match ? changelog.slice(match.index + match[0].length) : ''
const next = rest.search(/^## /m)
const notes = (next === -1 ? rest : rest.slice(0, next)).trim()

if (!notes) {
  console.error(`CHANGELOG.md has no "## ${version}" section. Add one before tagging v${version}.`)
  process.exit(1)
}
console.log(notes)

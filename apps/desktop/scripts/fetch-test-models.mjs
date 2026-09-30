// Downloads catalog models for the engine's real-model tests, verifying each file's SHA-256,
// into the layout those tests expect: <dir>/<model id>/<model files>.
//   node scripts/fetch-test-models.mjs <dir> [model id ...]
// Defaults to the two smallest models the tests use. Already-present models are skipped.
import { execFileSync } from 'node:child_process'
import { createHash } from 'node:crypto'
import { createWriteStream, existsSync, mkdirSync, readdirSync, readFileSync, renameSync, rmSync, statSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { Readable } from 'node:stream'
import { pipeline } from 'node:stream/promises'
import { fileURLToPath } from 'node:url'

const root = join(dirname(fileURLToPath(import.meta.url)), '..', '..', '..')
const catalog = JSON.parse(readFileSync(join(root, 'packages', 'model-catalog', 'catalog.json'), 'utf8'))
const [dir, ...ids] = process.argv.slice(2)
if (!dir) {
  console.error('usage: node scripts/fetch-test-models.mjs <dir> [model id ...]')
  process.exit(2)
}
const wanted = ids.length ? ids : ['whisper-tiny-en', 'piper-en_US-lessac-medium']

async function download(url, dest, sha256) {
  const res = await fetch(url)
  if (!res.ok) throw new Error(`${url}: HTTP ${res.status}`)
  const hash = createHash('sha256')
  const hashing = new TransformStream({
    transform(chunk, controller) {
      hash.update(chunk)
      controller.enqueue(chunk)
    },
  })
  await pipeline(Readable.fromWeb(res.body.pipeThrough(hashing)), createWriteStream(dest))
  const got = hash.digest('hex')
  if (got !== sha256) throw new Error(`${url}: sha256 ${got}, expected ${sha256}`)
}

for (const id of wanted) {
  const model = catalog.models.find((m) => m.id === id)
  if (!model) throw new Error(`unknown model ${id}`)
  const modelDir = join(dir, id)
  if (existsSync(modelDir)) {
    console.log(`${id}: present`)
    continue
  }
  const scratch = join(dir, `${id}.tmp`)
  rmSync(scratch, { recursive: true, force: true })
  mkdirSync(scratch, { recursive: true })
  for (const file of model.files) {
    const name = new URL(file.url).pathname.split('/').pop()
    const dest = join(scratch, name)
    console.log(`${id}: downloading ${name}`)
    await download(file.url, dest, file.sha256)
    if (file.archive) {
      // bsdtar on Windows and macOS, GNU tar on Linux: both read .tar.bz2.
      execFileSync('tar', ['-xjf', name], { cwd: scratch, stdio: 'inherit' })
      rmSync(dest)
    }
  }
  // Archives unpack into a single folder; the tests want the model files at <dir>/<id>/.
  const entries = readdirSync(scratch)
  const inner = entries.length === 1 ? join(scratch, entries[0]) : null
  renameSync(inner && statSync(inner).isDirectory() ? inner : scratch, modelDir)
  rmSync(scratch, { recursive: true, force: true })
  console.log(`${id}: ready in ${modelDir}`)
}

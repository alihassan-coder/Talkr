import { readFileSync, writeFileSync } from 'fs'
import { fileURLToPath } from 'url'
import { dirname, join } from 'path'
import { createHash } from 'crypto'
import { CatalogSchema, SHA256_RE } from '../schema.js'

// Usage:
//   tsx scripts/validate.ts           check schema, hashes, pinned URLs, reachability and sizes
//   tsx scripts/validate.ts --fill    stream-download every file whose sha256 is not a real
//                                     hash, fill in sha256 and sizeBytes, then validate
//   tsx scripts/validate.ts --offline skip the network checks (schema/hash/pinning only)

const __dirname = dirname(fileURLToPath(import.meta.url))
const CATALOG_PATH = join(__dirname, '..', 'catalog.json')

const args = process.argv.slice(2)
const shouldFill = args.includes('--fill')
const offline = args.includes('--offline')

type RawFile = { url: string; sha256: string; archive?: string }
type RawModel = { id: string; sizeBytes: number; files: RawFile[] }

// A Hugging Face `resolve/<ref>/` must be a 40-char commit sha, never a branch like `main`,
// otherwise the bytes behind the URL can change and the sha256 would stop matching.
const HF_RESOLVE_RE = /^https:\/\/huggingface\.co\/.+\/resolve\/([^/]+)\//

function unpinnedReason(url: string): string | null {
  const m = HF_RESOLVE_RE.exec(url)
  if (m && !/^[0-9a-f]{40}$/.test(m[1])) {
    return `Hugging Face URL uses mutable ref "${m[1]}"; pin it to a commit sha`
  }
  return null
}

async function hashUrl(url: string): Promise<{ sha256: string; size: number }> {
  const res = await fetch(url, { redirect: 'follow' })
  if (!res.ok || !res.body) throw new Error(`HTTP ${res.status}`)
  const hash = createHash('sha256')
  let size = 0
  for await (const chunk of res.body as unknown as AsyncIterable<Uint8Array>) {
    hash.update(chunk)
    size += chunk.byteLength
  }
  return { sha256: hash.digest('hex'), size }
}

async function remoteSize(url: string): Promise<number | null> {
  const res = await fetch(url, { method: 'HEAD', redirect: 'follow' })
  if (!res.ok) throw new Error(`HTTP ${res.status}`)
  const len = res.headers.get('content-length')
  return len ? parseInt(len, 10) : null
}

async function validate() {
  const catalog = JSON.parse(readFileSync(CATALOG_PATH, 'utf-8')) as { models: RawModel[] }
  let hasErrors = false
  const fail = (msg: string) => {
    console.error(`  ERROR ${msg}`)
    hasErrors = true
  }

  if (shouldFill) {
    let changed = false
    for (const model of catalog.models) {
      const needsFill = model.files.some((f) => !SHA256_RE.test(f.sha256))
      if (!needsFill) continue
      let total = 0
      for (const file of model.files) {
        console.log(`Hashing ${file.url} ...`)
        try {
          const { sha256, size } = await hashUrl(file.url)
          file.sha256 = sha256
          total += size
          console.log(`  ${sha256} (${size} bytes)`)
        } catch (e) {
          fail(`${model.id}: ${file.url}: ${e}`)
        }
      }
      if (total > 0) model.sizeBytes = total
      changed = true
    }
    if (changed) {
      writeFileSync(CATALOG_PATH, JSON.stringify(catalog, null, 2) + '\n')
      console.log('Catalog updated with computed SHA-256 hashes and sizes.')
    }
  }

  const result = CatalogSchema.safeParse(catalog)
  if (!result.success) {
    console.error('Catalog schema validation failed:')
    for (const issue of result.error.issues) {
      const path = issue.path.join('.')
      const idx = issue.path[1]
      const id = typeof idx === 'number' ? catalog.models[idx]?.id : undefined
      console.error(`  ${id ? `${id}: ` : ''}${path}: ${issue.message}`)
    }
    process.exit(1)
  }

  console.log(`Validating ${catalog.models.length} models...`)

  const ids = new Set<string>()
  for (const model of catalog.models) {
    if (ids.has(model.id)) fail(`duplicate model id ${model.id}`)
    ids.add(model.id)
    for (const file of model.files) {
      const reason = unpinnedReason(file.url)
      if (reason) fail(`${model.id}: ${reason}`)
    }
  }

  if (!offline) {
    await Promise.all(
      catalog.models.map(async (model) => {
        let total = 0
        let known = true
        for (const file of model.files) {
          try {
            const size = await remoteSize(file.url)
            if (size === null) known = false
            else total += size
          } catch (e) {
            fail(`${model.id}: ${file.url}: ${e}`)
            known = false
          }
        }
        if (known && total !== model.sizeBytes) {
          fail(`${model.id}: sizeBytes is ${model.sizeBytes} but the remote files total ${total}`)
        } else if (known) {
          console.log(`  ok ${model.id} (${total} bytes)`)
        }
      }),
    )
  }

  if (hasErrors) {
    console.error('\nValidation completed with errors.')
    process.exit(1)
  }
  console.log('\nAll models validated successfully!')
}

validate().catch((e) => {
  console.error(e)
  process.exit(1)
})

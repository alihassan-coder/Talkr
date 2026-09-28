import { readFileSync, writeFileSync } from 'fs'
import { fileURLToPath } from 'url'
import { dirname, join } from 'path'
import { CatalogSchema } from '../schema.js'
import { createHash } from 'crypto'
import { fetch } from 'undici'

const __filename = fileURLToPath(import.meta.url)
const __dirname = dirname(__filename)
const CATALOG_PATH = join(__dirname, '..', 'catalog.json')

const args = process.argv.slice(2)
const shouldFill = args.includes('--fill')

async function validate() {
  const raw = readFileSync(CATALOG_PATH, 'utf-8')
  const catalog = JSON.parse(raw)

  const result = CatalogSchema.safeParse(catalog)
  if (!result.success) {
    console.error('Catalog validation failed:')
    console.error(result.error.format())
    process.exit(1)
  }

  console.log(`Validating ${catalog.models.length} models...`)

  let hasErrors = false

  for (const model of catalog.models) {
    for (const file of model.files) {
      if (file.sha256 === 'PLACEHOLDER_SHA256') {
        if (shouldFill) {
          console.log(`Fetching ${file.url} to compute SHA256...`)
          try {
            const response = await fetch(file.url, { method: 'HEAD' })
            if (!response.ok) {
              console.error(`  Failed: HTTP ${response.status}`)
              hasErrors = true
              continue
            }

            const contentLength = response.headers.get('content-length')
            if (contentLength) {
              model.sizeBytes = parseInt(contentLength, 10)
            }

            const streamResponse = await fetch(file.url)
            const arrayBuffer = await streamResponse.arrayBuffer()
            const hash = createHash('sha256').update(Buffer.from(arrayBuffer)).digest('hex')
            file.sha256 = hash
            console.log(`  SHA256: ${hash}`)
          } catch (e) {
            console.error(`  Error: ${e}`)
            hasErrors = true
          }
        } else {
          console.warn(`  ${model.id}: ${file.url} has placeholder SHA256`)
          hasErrors = true
        }
      } else {
        try {
          const response = await fetch(file.url, { method: 'HEAD' })
          if (!response.ok) {
            console.error(`  ${model.id}: ${file.url} returned HTTP ${response.status}`)
            hasErrors = true
          }
        } catch (e) {
          console.error(`  ${model.id}: ${file.url} failed to fetch: ${e}`)
          hasErrors = true
        }
      }
    }
  }

  if (shouldFill) {
    writeFileSync(CATALOG_PATH, JSON.stringify(catalog, null, 2))
    console.log('Catalog updated with real SHA256 hashes and sizes.')
  }

  if (hasErrors) {
    console.error('\nValidation completed with errors.')
    process.exit(1)
  }

  console.log('\nAll models validated successfully!')
}

validate()
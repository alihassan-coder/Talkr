// Tiny static server for the `next build` export in ./out, resolving clean URLs the way
// Vercel does: /download -> download.html or download/index.html. Used by playwright.config.ts.
import { createReadStream, existsSync, statSync } from 'node:fs'
import { createServer } from 'node:http'
import { extname, join, normalize, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

const root = resolve(fileURLToPath(new URL('../out', import.meta.url)))
const port = Number(process.env.PORT ?? 4173)

const TYPES = {
  '.html': 'text/html; charset=utf-8',
  '.js': 'text/javascript; charset=utf-8',
  '.css': 'text/css; charset=utf-8',
  '.json': 'application/json',
  '.txt': 'text/plain; charset=utf-8',
  '.svg': 'image/svg+xml',
  '.png': 'image/png',
  '.ico': 'image/x-icon',
  '.woff2': 'font/woff2',
  '.webmanifest': 'application/manifest+json',
}

if (!existsSync(join(root, 'index.html'))) {
  console.error(`No static export in ${root}. Run \`pnpm --filter web build\` first.`)
  process.exit(1)
}

const isFile = (path) => existsSync(path) && statSync(path).isFile()

function resolveFile(pathname) {
  const clean = normalize(decodeURIComponent(pathname)).replace(/^([/\\])+/, '')
  const base = join(root, clean)
  if (!base.startsWith(root)) return null
  for (const candidate of [base, `${base}.html`, join(base, 'index.html')]) {
    if (isFile(candidate)) return candidate
  }
  // Next's segment prefetch files: the client asks for `__next.download.__PAGE__.txt`,
  // the export writes `__next.download/__PAGE__.txt`.
  const name = clean.split(/[/\\]/).pop() ?? ''
  if (name.startsWith('__next.') && name.endsWith('.txt')) {
    const parts = name.slice(0, -'.txt'.length).split('.')
    for (let i = parts.length - 1; i > 0; i--) {
      const nested = join(base, '..', [parts.slice(0, i).join('.'), ...parts.slice(i)].join('/') + '.txt')
      if (nested.startsWith(root) && isFile(nested)) return nested
    }
  }
  return null
}

createServer((req, res) => {
  const { pathname } = new URL(req.url ?? '/', 'http://localhost')
  const file = resolveFile(pathname)
  if (!file) {
    res.writeHead(404, { 'content-type': TYPES['.html'] })
    createReadStream(join(root, '404.html')).pipe(res)
    return
  }
  res.writeHead(200, { 'content-type': TYPES[extname(file)] ?? 'application/octet-stream' })
  createReadStream(file).pipe(res)
}).listen(port, '127.0.0.1', () => console.log(`Serving ${root} on http://127.0.0.1:${port}`))

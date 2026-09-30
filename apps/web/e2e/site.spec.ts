import { expect, test, type Page } from '@playwright/test'
import catalog from '../../../packages/model-catalog/catalog.json' with { type: 'json' }
import { VERSION, platforms } from '../lib/releases'

const RELEASE_PREFIX = `https://github.com/alihassan-coder/Talkr/releases/download/v${VERSION}/`

/** Collects console errors and uncaught page errors while a test runs. */
function watchErrors(page: Page) {
  const errors: string[] = []
  page.on('console', (msg) => {
    if (msg.type() === 'error') errors.push(msg.text())
  })
  page.on('pageerror', (err) => errors.push(err.message))
  page.on('response', (res) => {
    if (res.url().startsWith('http://127.0.0.1') && res.status() >= 400) errors.push(`${res.status()} ${res.url()}`)
  })
  return errors
}

test.describe('pages render cleanly', () => {
  for (const [path, heading] of [
    ['/', /Speech tools that/],
    ['/download', /Download Talkr/],
    ['/privacy', /Your voice, your data/],
  ] as const) {
    test(`${path} has no console errors`, async ({ page }) => {
      const errors = watchErrors(page)
      await page.goto(path)
      await expect(page.getByRole('heading', { level: 1 })).toContainText(heading)
      await page.waitForLoadState('networkidle')
      expect(errors).toEqual([])
    })
  }

  test('the models section lists the whole catalog', async ({ page }) => {
    const errors = watchErrors(page)
    await page.goto('/#models')
    const section = page.locator('section#models')
    await expect(section).toBeVisible()
    await expect(section.getByRole('heading', { name: 'Speech to text' })).toBeVisible()
    await expect(section.getByRole('listitem')).toHaveCount(catalog.models.length)
    const compressed = catalog.models.filter((m) => m.tags.includes('quantized')).length
    await expect(section.getByText('Compressed · less memory')).toHaveCount(compressed)
    expect(errors).toEqual([])
  })
})

test.describe('download links', () => {
  test('every file on the download page points at this version on GitHub', async ({ page }) => {
    await page.goto('/download')
    const expected = platforms.flatMap((p) => p.files.map((f) => `${RELEASE_PREFIX}${f.name}`))
    const hrefs = await page.locator(`a[href^="https://github.com/alihassan-coder/Talkr/releases/download/"]`).evaluateAll(
      (links) => links.map((a) => a.getAttribute('href')),
    )
    expect(hrefs.length).toBeGreaterThanOrEqual(expected.length)
    for (const href of hrefs) expect(href?.startsWith(RELEASE_PREFIX)).toBe(true)
    for (const url of expected) expect(hrefs).toContain(url)
  })

  for (const [ua, file] of [
    ['Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 Chrome/130 Safari/537.36', `Talkr_${VERSION}_x64-setup.exe`],
    ['Mozilla/5.0 (Macintosh; Intel Mac OS X 14_0) AppleWebKit/537.36 Chrome/130 Safari/537.36', `Talkr_${VERSION}_aarch64.dmg`],
    ['Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 Chrome/130 Safari/537.36', `Talkr_${VERSION}_amd64.AppImage`],
  ] as const) {
    test(`the main download button picks ${file}`, async ({ browser }) => {
      const context = await browser.newContext({ userAgent: ua })
      const page = await context.newPage()
      await page.goto('/')
      const button = page.getByRole('link', { name: /^Download for / }).first()
      await expect(button).toHaveAttribute('href', `${RELEASE_PREFIX}${file}`)
      await context.close()
    })
  }

  test('every download button on every page uses this version', async ({ page }) => {
    for (const path of ['/', '/download', '/privacy']) {
      await page.goto(path)
      await page.waitForLoadState('networkidle')
      const hrefs = await page.locator('a[href*="/releases/download/"]').evaluateAll((links) =>
        links.map((a) => a.getAttribute('href')),
      )
      for (const href of hrefs) expect(href, `${path}: ${href}`).toMatch(new RegExp(`^${RELEASE_PREFIX.replace(/[.]/g, '\\.')}`))
    }
  })
})

test('no broken internal links', async ({ page, request }) => {
  const seen = new Set<string>()
  const queue = ['/']
  const broken: string[] = []

  while (queue.length) {
    const path = queue.shift()!
    if (seen.has(path)) continue
    seen.add(path)
    const res = await page.goto(path)
    if (!res || res.status() >= 400) {
      broken.push(`${path} -> ${res?.status()}`)
      continue
    }
    const links = await page.locator('a[href^="/"]').evaluateAll((as) => as.map((a) => a.getAttribute('href')!))
    for (const href of links) {
      const url = new URL(href, 'http://x')
      // In-page anchors must exist on the target page.
      if (url.hash) {
        const target = url.pathname
        const html = await (await request.get(target)).text()
        const id = decodeURIComponent(url.hash.slice(1))
        if (!html.includes(`id="${id}"`)) broken.push(`${path}: ${href} (missing #${id})`)
      }
      if (!seen.has(url.pathname)) queue.push(url.pathname)
    }
  }

  expect(seen.size).toBeGreaterThanOrEqual(3)
  expect(broken).toEqual([])
})

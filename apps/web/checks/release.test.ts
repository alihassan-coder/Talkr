import { describe, expect, it } from 'vitest'
import { VERSION, downloadUrl, platforms } from '@/lib/releases'

// Run before every deploy (vercel.json's buildCommand, through `pnpm check:release`): the site
// links to files named after VERSION, so publishing it before that release is out on GitHub
// would turn every download button into a 404. Needs the network, so it is not part of `test`.

type Asset = { name: string; size: number; browser_download_url: string }
type Release = { tag_name: string; draft: boolean; prerelease: boolean; assets: Asset[] }

const MiB = 1024 * 1024

async function release(): Promise<Release> {
  const res = await fetch(`https://api.github.com/repos/alihassan-coder/Talkr/releases/tags/v${VERSION}`, {
    headers: { Accept: 'application/vnd.github+json' },
  })
  // Drafts are invisible without a token, so they answer 404 as well.
  if (res.status === 404) throw new Error(`Release v${VERSION} is not published on GitHub yet. Publish it before deploying the site.`)
  if (!res.ok) throw new Error(`GitHub answered ${res.status} for release v${VERSION}`)
  return (await res.json()) as Release
}

describe(`release v${VERSION}`, () => {
  it('is published, and every file the site offers is in it at about the size shown', async () => {
    const published = await release()
    expect(published.draft).toBe(false)
    expect(published.prerelease).toBe(false)

    const problems: string[] = []
    for (const file of platforms.flatMap((p) => p.files)) {
      const asset = published.assets.find((a) => a.name === file.name)
      if (!asset) {
        problems.push(`${file.name}: missing from the release`)
        continue
      }
      if (asset.browser_download_url !== downloadUrl(file.name)) {
        problems.push(`${file.name}: GitHub serves it at ${asset.browser_download_url}`)
      }
      // The site shows MiB as "MB", rounded; allow for the rounding, not for a stale number.
      const shown = Number.parseFloat(file.size)
      const actual = asset.size / MiB
      if (!(Math.abs(shown - actual) < 1)) {
        problems.push(`${file.name}: the site says ${file.size}, the file is ${actual.toFixed(1)} MB`)
      }
    }
    expect(problems).toEqual([])
  }, 30_000)
})

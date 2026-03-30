import { net } from 'electron'
import semver from 'semver'

export interface ChangelogEntry {
  version: string
  date: string
  body: string
}

interface GitHubRelease {
  tag_name: string
  body: string | null
  prerelease: boolean
  published_at: string
  draft: boolean
}

const GITHUB_RELEASES_URL =
  'https://api.github.com/repos/itsoltech/canopy-desktop/releases?per_page=100'

export async function fetchChangelogRange(
  fromVersion: string,
  toVersion: string,
  channel: 'stable' | 'next',
): Promise<ChangelogEntry[] | null> {
  try {
    const response = await net.fetch(GITHUB_RELEASES_URL, {
      headers: {
        Accept: 'application/vnd.github.v3+json',
        'User-Agent': 'Canopy-Desktop',
      },
    })

    if (!response.ok) return null

    const releases = (await response.json()) as GitHubRelease[]

    return releases
      .filter((r) => {
        if (r.draft || !r.body) return false
        if (channel === 'stable' && r.prerelease) return false

        const version = semver.valid(semver.clean(r.tag_name))
        if (!version) return false

        return semver.gt(version, fromVersion) && semver.lte(version, toVersion)
      })
      .sort((a, b) => {
        const va = semver.clean(a.tag_name)!
        const vb = semver.clean(b.tag_name)!
        return semver.rcompare(va, vb)
      })
      .map((r) => ({
        version: semver.clean(r.tag_name)!,
        date: r.published_at.split('T')[0],
        body: r.body!,
      }))
  } catch {
    return null
  }
}

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

async function fetchGitHubReleases(): Promise<GitHubRelease[]> {
  const response = await net.fetch(GITHUB_RELEASES_URL, {
    headers: {
      Accept: 'application/vnd.github.v3+json',
      'User-Agent': 'Canopy-Desktop',
    },
  })
  if (!response.ok) throw new Error(`GitHub API ${response.status}`)
  return (await response.json()) as GitHubRelease[]
}

/**
 * For the "next" update channel, determine whether electron-updater should
 * look for `latest-mac.yml` (stable) or `next-mac.yml` (pre-release) by
 * finding whichever pool contains the newest version above `currentVersion`.
 */
export async function resolveUpdateChannel(currentVersion: string): Promise<'latest' | 'next'> {
  try {
    const releases = await fetchGitHubReleases()

    let latestStable: string | null = null
    let latestPrerelease: string | null = null

    for (const r of releases) {
      if (r.draft) continue
      const v = semver.valid(semver.clean(r.tag_name))
      if (!v || !semver.gt(v, currentVersion)) continue

      if (r.prerelease) {
        if (!latestPrerelease || semver.gt(v, latestPrerelease)) latestPrerelease = v
      } else {
        if (!latestStable || semver.gt(v, latestStable)) latestStable = v
      }
    }

    if (latestStable && (!latestPrerelease || semver.gte(latestStable, latestPrerelease))) {
      return 'latest'
    }
    return 'next'
  } catch {
    return 'next'
  }
}

export async function fetchChangelogRange(
  fromVersion: string,
  toVersion: string,
  channel: 'stable' | 'next',
): Promise<ChangelogEntry[] | null> {
  try {
    const releases = await fetchGitHubReleases()

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

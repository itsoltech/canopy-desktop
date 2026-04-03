import { net } from 'electron'
import semver from 'semver'
import type { ChangelogError } from './errors'
import { fromExternalCall, errorMessage, type ResultAsyncType } from '../errors'

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

function fetchGitHubReleases(): ResultAsyncType<GitHubRelease[], ChangelogError> {
  return fromExternalCall(
    (async () => {
      const response = await net.fetch(GITHUB_RELEASES_URL, {
        headers: {
          Accept: 'application/vnd.github.v3+json',
          'User-Agent': 'Canopy-Desktop',
        },
      })
      if (!response.ok) throw new Error(`GitHub API ${response.status}`)
      return (await response.json()) as GitHubRelease[]
    })(),
    (e): ChangelogError => ({ _tag: 'FetchFailed', message: errorMessage(e) }),
  )
}

export function resolveUpdateChannel(
  currentVersion: string,
): ResultAsyncType<'latest' | 'next', ChangelogError> {
  return fetchGitHubReleases().map((releases) => {
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
  })
}

export function fetchChangelogRange(
  fromVersion: string,
  toVersion: string,
  channel: 'stable' | 'next',
): ResultAsyncType<ChangelogEntry[], ChangelogError> {
  return fetchGitHubReleases().map((releases) =>
    releases
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
      })),
  )
}

import { describe, expect, it } from 'vitest'
import { githubTokenCreationUrl } from './githubToken'

describe('githubTokenCreationUrl', () => {
  it('prefills the least permissions required by the GitHub Actions provider', () => {
    const url = new URL(githubTokenCreationUrl('itsoltech/canopy-desktop'))

    expect(url.origin + url.pathname).toBe('https://github.com/settings/personal-access-tokens/new')
    expect(url.searchParams.get('actions')).toBe('write')
    expect(url.searchParams.get('contents')).toBe('read')
    expect(url.searchParams.get('target_name')).toBe('itsoltech')
  })

  it('leaves the resource owner for the user when the repository is not known yet', () => {
    const url = new URL(githubTokenCreationUrl())

    expect(url.searchParams.has('target_name')).toBe(false)
  })
})

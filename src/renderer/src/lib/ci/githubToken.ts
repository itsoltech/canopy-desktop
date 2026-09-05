export function githubTokenCreationUrl(repository?: string): string {
  const url = new URL('https://github.com/settings/personal-access-tokens/new')
  url.searchParams.set('name', 'Canopy CI/CD')
  url.searchParams.set(
    'description',
    'Dispatch and monitor configured GitHub Actions workflows from Canopy',
  )
  url.searchParams.set('actions', 'write')
  url.searchParams.set('contents', 'read')

  const owner = repository?.split('/')[0]
  if (owner) url.searchParams.set('target_name', owner)
  return url.toString()
}

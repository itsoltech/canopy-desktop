const LABELS: Record<string, string> = {
  jira: 'Jira',
  youtrack: 'YouTrack',
  github: 'GitHub',
  'github-actions': 'GitHub Actions',
  teamcity: 'TeamCity',
}

export function providerLabel(provider: string): string {
  return LABELS[provider] ?? provider
}

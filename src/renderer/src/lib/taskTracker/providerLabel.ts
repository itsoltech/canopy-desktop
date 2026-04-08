const LABELS: Record<string, string> = {
  jira: 'Jira',
  youtrack: 'YouTrack',
  github: 'GitHub',
}

export function providerLabel(provider: string): string {
  return LABELS[provider] ?? provider
}

const TRACKER_BINDING_PREFIX = 'tracker:'

export function trackerBindingKey(trackerId: string): string {
  return `${TRACKER_BINDING_PREFIX}${trackerId}`
}

export function parseTrackerBindingKey(bindingKey: string): string | null {
  if (!bindingKey.startsWith(TRACKER_BINDING_PREFIX)) return null
  const trackerId = bindingKey.slice(TRACKER_BINDING_PREFIX.length)
  return trackerId && trackerId.length <= 256 && !/[\r\n]/.test(trackerId) ? trackerId : null
}

export function githubActionsCredentialBaseUrl(repository: string): string {
  return `https://github.com/${repository.toLowerCase()}`
}

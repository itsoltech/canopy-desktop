let connections: IssueTrackerConnectionInfo[] = $state([])
let loading = $state(false)

export function getIssueTrackerConnections(): IssueTrackerConnectionInfo[] {
  return connections
}

export function isIssueTrackerLoading(): boolean {
  return loading
}

export async function loadConnections(): Promise<void> {
  loading = true
  try {
    connections = await window.api.issueTrackerGetConnections()
  } catch {
    connections = []
  } finally {
    loading = false
  }
}

export async function removeConnection(connectionId: string): Promise<void> {
  await window.api.issueTrackerRemoveConnection(connectionId)
  connections = connections.filter((c) => c.id !== connectionId)
}

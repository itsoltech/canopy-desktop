import type { IssueTrackerProvider, IssueTrackerProviderClient } from '../types'
import { jiraClient } from './jira'
import { youtrackClient } from './youtrack'

const clients: Record<IssueTrackerProvider, IssueTrackerProviderClient> = {
  jira: jiraClient,
  youtrack: youtrackClient,
}

export function createProviderClient(provider: IssueTrackerProvider): IssueTrackerProviderClient {
  return clients[provider]
}

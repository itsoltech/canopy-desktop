import type { TaskTrackerProvider, TaskTrackerProviderClient } from '../types'
import { jiraClient } from './jira'
import { youtrackClient } from './youtrack'

const clients: Record<TaskTrackerProvider, TaskTrackerProviderClient> = {
  jira: jiraClient,
  youtrack: youtrackClient,
}

export function createProviderClient(provider: TaskTrackerProvider): TaskTrackerProviderClient {
  return clients[provider]
}

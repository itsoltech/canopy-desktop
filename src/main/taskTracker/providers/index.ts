import type { TaskTrackerProvider, TaskTrackerProviderClient } from '../types'
import { jiraClient } from './jira'
import { youtrackClient } from './youtrack'
import { githubClient } from './github'

const clients: Record<TaskTrackerProvider, TaskTrackerProviderClient> = {
  jira: jiraClient,
  youtrack: youtrackClient,
  github: githubClient,
}

export function createProviderClient(provider: TaskTrackerProvider): TaskTrackerProviderClient {
  return clients[provider]
}

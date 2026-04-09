import type { ResultAsync } from 'neverthrow'
import type { TaskTrackerError } from './errors'

export type TaskTrackerProvider = 'jira' | 'youtrack' | 'github'

export interface TaskTrackerConnection {
  id: string
  provider: TaskTrackerProvider
  name: string
  baseUrl: string
  projectKey: string
  boardId?: string
  authPrefKey: string
  username?: string
}

export interface TrackerTask {
  key: string
  summary: string
  description: string
  status: string
  priority: string
  type: 'task' | 'story' | 'subtask' | 'bug' | 'epic' | string
  parentKey?: string
  sprintName?: string
  sprintNumber?: number
  assignee?: string
  url?: string
}

export interface TrackerBoard {
  id: string
  name: string
  projectKey?: string
}

export interface TrackerStatus {
  id: string
  name: string
}

export interface TrackerComment {
  id: string
  author: string
  body: string
  created: string
}

export interface TrackerAttachment {
  id: string
  name: string
  mimeType: string
  size: number
  url: string
}

export interface TrackerSprint {
  id: string
  name: string
  number?: number
  state: 'active' | 'closed' | 'future'
}

export interface BranchTemplateConfig {
  template: string
  customVars: Record<string, string>
}

export interface PRTargetRule {
  taskType: string
  targetPattern: string
}

export interface PRTemplateConfig {
  titleTemplate: string
  bodyTemplate: string
  defaultTargetBranch: string
  targetRules: PRTargetRule[]
}

export interface TaskFilterConfig {
  assignedToMe: boolean
  statuses: string[]
}

export interface TaskTrackerConfig {
  connections: TaskTrackerConnection[]
  branchTemplate: BranchTemplateConfig
  prTemplate: PRTemplateConfig
  filters: Record<string, TaskFilterConfig>
}

// --- Repo-level config types ---

export interface TrackerConfig {
  id: string
  provider: TaskTrackerProvider
  baseUrl: string
  projectKey?: string
}

export interface BoardOverride {
  branchTemplate?: Partial<BranchTemplateConfig & { typeMapping: Record<string, string> }>
  prTemplate?: Partial<PRTemplateConfig>
}

export interface RepoConfig {
  version: 1
  trackers: TrackerConfig[]
  branchTemplate?: BranchTemplateConfig & { typeMapping?: Record<string, string> }
  prTemplate?: PRTemplateConfig
  boardOverrides: Record<string, BoardOverride>
  filters: TaskFilterConfig
}

// --- Resolved config (merged global + repo) ---

export type ConfigSource = 'global' | 'repo'

export interface ResolvedConfig {
  config: RepoConfig
  source: {
    branchTemplate: ConfigSource | 'default'
    prTemplate: ConfigSource | 'default'
    filters: ConfigSource | 'default'
  }
  hasGlobal: boolean
  hasRepo: boolean
}

export interface TaskTrackerExportData {
  version: number
  exportedAt: string
  connections: Omit<TaskTrackerConnection, 'authPrefKey'>[]
  branchTemplate: BranchTemplateConfig
  prTemplate: PRTemplateConfig
  filters: Record<string, TaskFilterConfig>
}

export interface FetchTasksParams {
  connectionId: string
  statuses?: string[]
  assignedToMe?: boolean
  boardId?: string
}

export interface TaskTrackerProviderClient {
  testConnection(
    connection: TaskTrackerConnection,
    token: string,
  ): ResultAsync<boolean, TaskTrackerError>
  getCurrentUserDisplayName(
    connection: TaskTrackerConnection,
    token: string,
  ): ResultAsync<string, TaskTrackerError>
  fetchTaskByKey(
    connection: TaskTrackerConnection,
    token: string,
    taskKey: string,
  ): ResultAsync<TrackerTask | null, TaskTrackerError>
  fetchBoards(
    connection: TaskTrackerConnection,
    token: string,
  ): ResultAsync<TrackerBoard[], TaskTrackerError>
  fetchStatuses(
    connection: TaskTrackerConnection,
    token: string,
    boardId?: string,
  ): ResultAsync<TrackerStatus[], TaskTrackerError>
  fetchTasks(
    connection: TaskTrackerConnection,
    token: string,
    params: { statuses?: string[]; assignedToMe?: boolean; boardId?: string },
  ): ResultAsync<TrackerTask[], TaskTrackerError>
  getCurrentSprint(
    connection: TaskTrackerConnection,
    token: string,
    boardId?: string,
  ): ResultAsync<TrackerSprint | null, TaskTrackerError>
  fetchTaskComments(
    connection: TaskTrackerConnection,
    token: string,
    taskKey: string,
  ): ResultAsync<TrackerComment[], TaskTrackerError>
  fetchTaskAttachments(
    connection: TaskTrackerConnection,
    token: string,
    taskKey: string,
  ): ResultAsync<TrackerAttachment[], TaskTrackerError>
}

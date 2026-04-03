export type TaskTrackerProvider = 'jira' | 'youtrack'

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
  testConnection(connection: TaskTrackerConnection, token: string): Promise<boolean>
  getCurrentUserDisplayName(connection: TaskTrackerConnection, token: string): Promise<string>
  fetchTaskByKey(
    connection: TaskTrackerConnection,
    token: string,
    taskKey: string,
  ): Promise<TrackerTask | null>
  fetchBoards(connection: TaskTrackerConnection, token: string): Promise<TrackerBoard[]>
  fetchStatuses(
    connection: TaskTrackerConnection,
    token: string,
    boardId?: string,
  ): Promise<TrackerStatus[]>
  fetchTasks(
    connection: TaskTrackerConnection,
    token: string,
    params: { statuses?: string[]; assignedToMe?: boolean; boardId?: string },
  ): Promise<TrackerTask[]>
  getCurrentSprint(
    connection: TaskTrackerConnection,
    token: string,
    boardId?: string,
  ): Promise<TrackerSprint | null>
  fetchTaskComments(
    connection: TaskTrackerConnection,
    token: string,
    taskKey: string,
  ): Promise<TrackerComment[]>
  fetchTaskAttachments(
    connection: TaskTrackerConnection,
    token: string,
    taskKey: string,
  ): Promise<TrackerAttachment[]>
}

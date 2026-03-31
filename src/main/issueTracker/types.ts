export type IssueTrackerProvider = 'jira' | 'youtrack'

export interface IssueTrackerConnection {
  id: string
  provider: IssueTrackerProvider
  name: string
  baseUrl: string
  projectKey: string
  boardId?: string
  authPrefKey: string
  username?: string
}

export interface TrackerIssue {
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
  issueType: string
  targetPattern: string
}

export interface PRTemplateConfig {
  titleTemplate: string
  bodyTemplate: string
  defaultTargetBranch: string
  targetRules: PRTargetRule[]
}

export interface IssueFilterConfig {
  assignedToMe: boolean
  statuses: string[]
}

export interface IssueTrackerConfig {
  connections: IssueTrackerConnection[]
  branchTemplate: BranchTemplateConfig
  prTemplate: PRTemplateConfig
  filters: Record<string, IssueFilterConfig>
}

export interface IssueTrackerExportData {
  version: number
  exportedAt: string
  connections: Omit<IssueTrackerConnection, 'authPrefKey'>[]
  branchTemplate: BranchTemplateConfig
  prTemplate: PRTemplateConfig
  filters: Record<string, IssueFilterConfig>
}

export interface FetchIssuesParams {
  connectionId: string
  statuses?: string[]
  assignedToMe?: boolean
  boardId?: string
}

export interface IssueTrackerProviderClient {
  testConnection(connection: IssueTrackerConnection, token: string): Promise<boolean>
  getCurrentUserDisplayName(connection: IssueTrackerConnection, token: string): Promise<string>
  fetchIssueByKey(
    connection: IssueTrackerConnection,
    token: string,
    issueKey: string,
  ): Promise<TrackerIssue | null>
  fetchBoards(connection: IssueTrackerConnection, token: string): Promise<TrackerBoard[]>
  fetchStatuses(
    connection: IssueTrackerConnection,
    token: string,
    boardId?: string,
  ): Promise<TrackerStatus[]>
  fetchIssues(
    connection: IssueTrackerConnection,
    token: string,
    params: { statuses?: string[]; assignedToMe?: boolean; boardId?: string },
  ): Promise<TrackerIssue[]>
  getCurrentSprint(
    connection: IssueTrackerConnection,
    token: string,
    boardId?: string,
  ): Promise<TrackerSprint | null>
}

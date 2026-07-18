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

/** Normalized status category — drives status chip colors in the UI. Jira reports it directly
 *  (statusCategory); other providers approximate (e.g. YouTrack resolved states → 'done'). */
export type TrackerStatusCategory = 'todo' | 'in-progress' | 'done'

export interface TrackerTask {
  key: string
  summary: string
  description: string
  status: string
  statusCategory?: TrackerStatusCategory
  priority: string
  type: 'task' | 'story' | 'subtask' | 'bug' | 'epic' | string
  /** The tracker's OWN name for the type (e.g. Jira "User Story") — `type` is normalized. */
  typeName?: string
  /** Tracker-hosted icon for the task type (authenticated URL — proxy before rendering). */
  typeIconUrl?: string
  parentKey?: string
  sprintName?: string
  sprintNumber?: number
  assignee?: string
  /** Assignee avatar — may live on the tracker origin (authenticated) or a public CDN. */
  assigneeAvatarUrl?: string
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
  statusCategory?: TrackerStatusCategory
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

/** Assignable user. `id` is the provider-native identifier used when creating a task:
 *  Jira accountId, YouTrack login, GitHub GraphQL user node id. */
export interface TrackerUser {
  id: string
  displayName: string
}

export interface CreateTaskInput {
  /** Jira/YouTrack project key. Absent for GitHub. */
  projectKey?: string
  /** Tracker's own type name (from fetchCreateTaskTypes). Absent for GitHub. */
  typeName?: string
  title: string
  description?: string
  /** TrackerUser.id from fetchAssignableUsers. */
  assigneeId?: string
  /** Board hosting the sprint (YouTrack sprint command needs the board; Jira/GitHub ignore it). */
  boardId?: string
  /** TrackerSprint.id from fetchSprints — Jira agile sprint id, YouTrack sprint id,
   *  GitHub milestone GraphQL node id (NOT the numeric milestone number). */
  sprintId?: string
}

export interface CreatedTask {
  key: string
  url?: string
  /** Post-create steps that failed AFTER the task itself was created (partial state —
   *  surfaced to the user, never a hard failure: retrying the create would duplicate it). */
  warnings: string[]
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
  /**
   * Tracker projects (task-key prefixes) that belong to THIS repository. When non-empty it acts
   * as a whitelist for the task pickers and for which projects can carry template overrides.
   * Empty/absent = all projects the credentials can see.
   */
  projects?: string[]
}

export interface ProjectOverride {
  branchTemplate?: Partial<BranchTemplateConfig & { typeMapping: Record<string, string> }>
  prTemplate?: Partial<PRTemplateConfig>
}

/** A tracker project (Jira project / YouTrack project) — `key` is the task-key prefix. */
export interface TrackerProject {
  key: string
  name: string
}

export interface RepoConfig {
  version: 1
  trackers: TrackerConfig[]
  branchTemplate?: BranchTemplateConfig & { typeMapping?: Record<string, string> }
  prTemplate?: PRTemplateConfig
  /**
   * Template overrides keyed by the tracker PROJECT key — the task-key prefix (`GAKKO-1` →
   * `GAKKO`), an intrinsic property of every task. Boards are only a browsing filter.
   */
  projectOverrides: Record<string, ProjectOverride>
  filters: TaskFilterConfig
  /**
   * Guidance for AI agents working in this repository, stored verbatim in config.json so any
   * agent reading the file sees it. Canopy itself never interprets these strings.
   */
  agents?: {
    instructions: string[]
  }
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
  /**
   * Ids of trackers declared by the REPO's own config. The merged `config.trackers` also carries
   * personal (global) connections for credential reuse — project-scoped UI (e.g. the sidebar
   * PROJECT MANAGEMENT section) must not present those as the project's trackers.
   */
  repoTrackerIds: string[]
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
  /** Tracker projects (task-key prefixes). Providers without the concept (GitHub) return []. */
  fetchProjects(
    connection: TaskTrackerConnection,
    token: string,
  ): ResultAsync<TrackerProject[], TaskTrackerError>
  /** Task type names as the tracker defines them (bug/story/…), for type-mapping editors. */
  fetchTaskTypes(
    connection: TaskTrackerConnection,
    token: string,
  ): ResultAsync<string[], TaskTrackerError>
  fetchStatuses(
    connection: TaskTrackerConnection,
    token: string,
    boardId?: string,
  ): ResultAsync<TrackerStatus[], TaskTrackerError>
  fetchTasks(
    connection: TaskTrackerConnection,
    token: string,
    params: { statuses?: string[]; assignedToMe?: boolean; boardId?: string; projectKey?: string },
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
  /** Transitions available from the task's CURRENT status, with workflow-required fields when the
   *  provider can introspect them (Jira). Providers without introspection return empty `fields`. */
  fetchTransitions(
    connection: TaskTrackerConnection,
    token: string,
    taskKey: string,
  ): ResultAsync<TrackerTransition[], TaskTrackerError>
  applyTransition(
    connection: TaskTrackerConnection,
    token: string,
    taskKey: string,
    transitionId: string,
    opts: { fields?: Record<string, string>; comment?: string },
  ): ResultAsync<void, TaskTrackerError>
  addComment(
    connection: TaskTrackerConnection,
    token: string,
    taskKey: string,
    body: string,
  ): ResultAsync<void, TaskTrackerError>
  /** Users a new task in `projectKey` can be assigned to (GitHub ignores the project). */
  fetchAssignableUsers(
    connection: TaskTrackerConnection,
    token: string,
    projectKey: string,
  ): ResultAsync<TrackerUser[], TaskTrackerError>
  /** Active + future sprints of a board (GitHub: open milestones, boardId ignored). */
  fetchSprints(
    connection: TaskTrackerConnection,
    token: string,
    boardId: string,
  ): ResultAsync<TrackerSprint[], TaskTrackerError>
  /** Type names valid for CREATING a task in `projectKey` (unlike the global fetchTaskTypes
   *  used by the type-mapping editors). Empty = the tracker has no type concept (GitHub). */
  fetchCreateTaskTypes(
    connection: TaskTrackerConnection,
    token: string,
    projectKey: string,
  ): ResultAsync<string[], TaskTrackerError>
  createTask(
    connection: TaskTrackerConnection,
    token: string,
    input: CreateTaskInput,
  ): ResultAsync<CreatedTask, TaskTrackerError>
}

/** A field the workflow requires/offers on a specific transition (e.g. Jira `resolution`). */
export interface TrackerTransitionField {
  key: string
  name: string
  required: boolean
  allowedValues?: { id: string; name: string }[]
}

export interface TrackerTransition {
  id: string
  name: string
  /** Status the task will be in after the transition. */
  toStatus: string
  toStatusCategory?: TrackerStatusCategory
  /** Empty when the provider cannot introspect workflow requirements (YouTrack, GitHub). */
  fields: TrackerTransitionField[]
}

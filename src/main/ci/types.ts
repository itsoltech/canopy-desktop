// --- Repo configuration (`.canopy/config.json` → `ci`) ---

export interface CiBuildTypeConfig {
  /** TeamCity build configuration id, e.g. `Gakko_Build`. */
  id: string
  /** Short label shown in the sidebar row. */
  label: string
}

export type CiProvider = 'teamcity' | 'github-actions'

interface CiConfigWarnings {
  /** Hand-edited entries that failed provider-specific validation. */
  droppedInvalid?: { count: number; ids: string[] }
  /** Valid entries beyond the parse-time cap. */
  droppedOverCap?: { count: number; ids: string[] }
}

export interface TeamCityCiConfig extends CiConfigWarnings {
  provider: 'teamcity'
  baseUrl: string
  buildTypes: CiBuildTypeConfig[]
}

export interface CiWorkflowConfig {
  /** Repository-relative workflow file path under `.github/workflows`. */
  path: string
  /** Short label shown in Canopy CI surfaces. */
  label: string
}

export interface GitHubActionsCiConfig extends CiConfigWarnings {
  provider: 'github-actions'
  baseUrl: 'https://github.com'
  /** Canonical `owner/repository` identity. */
  repository: string
  workflows: CiWorkflowConfig[]
}

export type CiConfig = TeamCityCiConfig | GitHubActionsCiConfig

export interface CiRef {
  name: string
  kind: 'branch' | 'tag'
  /** Provider-resolved commit at read/confirmation time. */
  commitSha?: string
}

export type CiInputValue = string | boolean

export interface CiParameterSet {
  parameters: CiParameter[]
  /** Provider-owned immutable workflow/config revision used to reject stale forms. */
  schemaRevision: string
}

export type CiRunState = 'queued' | 'running' | 'waiting' | 'finished' | 'unknown'
export type CiRunConclusion = 'success' | 'failure' | 'cancelled' | 'neutral' | 'unknown'

export interface CiRun {
  provider: CiProvider
  runId: string
  number: string | undefined
  jobId: string
  jobLabel: string
  state: CiRunState
  conclusion: CiRunConclusion
  statusText: string | undefined
  webUrl: string
  ref: CiRef | undefined
  queuedAt: number | undefined
  startedAt: number | undefined
  finishedAt: number | undefined
}

export interface CiJob {
  id: string
  label: string
  provider: CiProvider
}

export interface CiDiscoveredWorkflow {
  id: string
  path: string
  name: string
  webUrl: string
  available: boolean
  error?: string
}

export interface GitHubActionsSetupInfo {
  repository: string
  defaultBranch: string
  workflows: CiDiscoveredWorkflow[]
}

export interface CiJobStatus {
  jobId: string
  label: string
  provider: CiProvider
  run: CiRun | null
  error?: string
}

export interface CiRunActivity {
  running: CiRun[]
  queued: CiRun[]
  recent: CiRun[]
  /** Scoped provider queries that failed or reached a completeness cap. */
  partialErrors?: string[]
}

export interface CiTriggerRequest {
  jobId: string
  ref: CiRef
  schemaRevision?: string
  inputs: Record<string, CiInputValue>
}

export interface CiRunTriggerResult {
  provider: CiProvider
  runId: string
  webUrl: string
  ref: CiRef
}

// --- Normalized build state ---

export type CiBuildState = 'queued' | 'running' | 'finished'
// ERROR is TeamCity's infra/agent-failure outcome — a red state in its own UI,
// so it must stay distinguishable from UNKNOWN (which also covers cancelled).
export type CiBuildResult = 'SUCCESS' | 'FAILURE' | 'ERROR' | 'UNKNOWN'

export interface CiBuildStatus {
  id: number
  number: string
  state: CiBuildState
  status: CiBuildResult
  /** TeamCity's build-specific summary, for example "sylabusy-api-test deployed". */
  statusText: string | undefined
  percentageComplete: number | undefined
  webUrl: string
  branchName: string | undefined
  queuedAt: number | undefined
  startedAt: number | undefined
  finishedAt: number | undefined
}

/** Per-buildType row returned to the renderer: config label + latest build (if any). */
export interface CiBuildTypeStatus {
  buildTypeId: string
  label: string
  build: CiBuildStatus | null
  /** Set when THIS row's status fetch failed — null build then means "unknown", not "never built". */
  error?: string
}

export interface CiTriggerResult {
  buildId: number
  webUrl: string
  /** Branch TeamCity actually queued on — ground truth for the confirmation toast. */
  branchName: string | undefined
}

/** A build-configuration entry on the TeamCity server — source for the config picker. */
export interface CiServerBuildType {
  id: string
  name: string
  projectName: string
}

/**
 * One "Run custom build" prompt parameter, normalized from TeamCity's typed
 * parameter spec. Drives the dynamic form shown before triggering.
 */
export interface CiParameter {
  name: string
  kind: 'text' | 'password' | 'checkbox' | 'select'
  label: string
  description: string | undefined
  required: boolean
  defaultValue: string
  options: string[] | undefined
  multiple: boolean
  /** Joins the selected options of a multi-select into the property value. */
  valueSeparator: string
  checkedValue: string | undefined
  uncheckedValue: string | undefined
  /** Provider-neutral primitive expected by the trusted IPC validation layer. */
  valueType?: 'string' | 'boolean'
  /** Distinguishes an explicit empty workflow default from no default. */
  hasDefault?: boolean
}

/** A running, queued or recently finished build in repository-scoped activity. */
export interface CiActivityBuild {
  id: number
  number: string | undefined
  state: 'running' | 'queued' | 'finished'
  /** TeamCity outcome (SUCCESS/FAILURE/…) — set for running and finished builds. */
  status: string | undefined
  /** TeamCity's build-specific summary, for example "sylabusy-api-test deployed". */
  statusText: string | undefined
  percentageComplete: number | undefined
  webUrl: string
  branchName: string | undefined
  /** Epoch ms, when known — queued/start/finish timestamps from the server. */
  queuedAt: number | undefined
  startedAt: number | undefined
  finishedAt: number | undefined
  buildTypeId: string
  buildTypeName: string
}

export interface CiActivity {
  running: CiActivityBuild[]
  queued: CiActivityBuild[]
  /** Most recent finished builds for configured jobs, newest first. */
  recent: CiActivityBuild[]
  /** Independent TeamCity activity queries that failed while sibling data remained usable. */
  partialErrors?: string[]
}

/**
 * `ci:status` IPC contract. A read endpoint that never throws — auth and API
 * problems come back as fields so the sidebar can render a reconnect hint or an
 * error line instead of an exception toast.
 */
export interface CiStatusResponse {
  configured: boolean
  baseUrl?: string
  hasToken?: boolean
  rows: CiBuildTypeStatus[]
  error?: string
}

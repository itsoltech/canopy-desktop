// Renderer-side mirrors of the preload shapes — ambient declarations from
// index.d.ts are not visible to ESLint inside .svelte scripts, so components
// import these structurally-identical types instead.

/** A running, queued or recently finished build in the activity views. */
export interface CiActivityBuild {
  id: number
  number: string | undefined
  state: 'running' | 'queued' | 'finished'
  status: string | undefined
  statusText: string | undefined
  percentageComplete: number | undefined
  webUrl: string
  branchName: string | undefined
  queuedAt: number | undefined
  startedAt: number | undefined
  finishedAt: number | undefined
  buildTypeId: string
  buildTypeName: string
}

export interface CiActivity {
  running: CiActivityBuild[]
  queued: CiActivityBuild[]
  recent: CiActivityBuild[]
  partialErrors?: string[]
}

/** Newest build of a branch (per configured job) — the `ci:status` row shape. */
export interface CiBuildStatus {
  id: number
  number: string
  state: 'queued' | 'running' | 'finished'
  status: 'SUCCESS' | 'FAILURE' | 'ERROR' | 'UNKNOWN'
  statusText: string | undefined
  percentageComplete: number | undefined
  webUrl: string
  branchName: string | undefined
  queuedAt: number | undefined
  startedAt: number | undefined
  finishedAt: number | undefined
}

export interface CiBuildTypeStatus {
  buildTypeId: string
  label: string
  build: CiBuildStatus | null
  /** Set when this row's status fetch failed — null build then means "unknown". */
  error?: string
}

/** `ci:config`'s parsed-config shape (mirror of the preload `CiConfigInfo`). */
export interface TeamCityCiRepoConfigInfo {
  provider: 'teamcity'
  baseUrl: string
  buildTypes: Array<{ id: string; label: string }>
  /** Typo'd ids from a hand-edited file — recovery is correcting the FILE. */
  droppedInvalid?: { count: number; ids: string[] }
  /** Valid entries beyond the parse cap — recovery is re-ticking in the picker. */
  droppedOverCap?: { count: number; ids: string[] }
}

export interface GitHubActionsCiRepoConfigInfo {
  provider: 'github-actions'
  baseUrl: 'https://github.com'
  repository: string
  workflows: Array<{ path: string; label: string }>
  droppedInvalid?: { count: number; ids: string[] }
  droppedOverCap?: { count: number; ids: string[] }
}

export type CiRepoConfigInfo = TeamCityCiRepoConfigInfo | GitHubActionsCiRepoConfigInfo

export interface CiCredentialStatus {
  hasToken: boolean
  authenticationState: 'valid' | 'invalid' | 'unknown'
  authenticationCheckedAt?: string
}

export interface CiParameter {
  name: string
  kind: 'text' | 'password' | 'checkbox' | 'select'
  label: string
  description: string | undefined
  required: boolean
  defaultValue: string
  options: string[] | undefined
  multiple: boolean
  valueSeparator: string
  checkedValue: string | undefined
  uncheckedValue: string | undefined
  valueType?: 'string' | 'boolean'
  hasDefault?: boolean
}

export interface CiRef {
  name: string
  kind: 'branch' | 'tag'
  commitSha?: string
}

export interface CiParameterSet {
  parameters: CiParameter[]
  schemaRevision: string
}

export interface CiRun {
  provider: 'teamcity' | 'github-actions'
  runId: string
  number: string | undefined
  jobId: string
  jobLabel: string
  state: 'queued' | 'running' | 'waiting' | 'finished' | 'unknown'
  conclusion: 'success' | 'failure' | 'cancelled' | 'neutral' | 'unknown'
  statusText: string | undefined
  webUrl: string
  ref: CiRef | undefined
  queuedAt: number | undefined
  startedAt: number | undefined
  finishedAt: number | undefined
}

/** Provider-neutral shape of the ONE run the sidebar card shows. */
export interface CiLastStatusRow {
  id: string
  label: string
  number?: string
  timestamp?: number
  timestampLabel?: 'Queued' | 'Started' | 'Finished'
  statusText?: string
  statusTextClass?: string
  error?: string
  chip: { label: string; cls: string }
}

/**
 * Failure suffix on the sidebar's single CI element. Set only when an activity slice
 * failed — the running/queued counts it used to sit next to now live in the history
 * window, so failure is the one activity signal the sidebar still carries.
 */
export interface CiCardIssue {
  /** One word appended to the card heading: 'Error' or 'Incomplete'. */
  label: string
  /** Full reason, for the element's tooltip. */
  detail: string
}

export interface CiJobStatus {
  jobId: string
  label: string
  provider: 'teamcity' | 'github-actions'
  run: CiRun | null
  error?: string
}

export interface CiRunActivity {
  running: CiRun[]
  queued: CiRun[]
  recent: CiRun[]
  partialErrors?: string[]
}

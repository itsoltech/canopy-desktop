// --- Repo configuration (`.canopy/config.json` → `ci`) ---

export interface CiBuildTypeConfig {
  /** TeamCity build configuration id, e.g. `Gakko_Build`. */
  id: string
  /** Short label shown in the sidebar row. */
  label: string
}

export interface CiConfig {
  provider: 'teamcity'
  baseUrl: string
  buildTypes: CiBuildTypeConfig[]
  /** Entries beyond the parse-time cap, dropped from `buildTypes` — carried so
      the configurator can announce them before a Save deletes them for real. */
  droppedBuildTypes?: number
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
  percentageComplete: number | undefined
  webUrl: string
  branchName: string | undefined
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
}

/** A running, queued or recently finished build in the server-wide activity view. */
export interface CiActivityBuild {
  id: number
  number: string | undefined
  state: 'running' | 'queued' | 'finished'
  /** TeamCity outcome (SUCCESS/FAILURE/…) — set for running and finished builds. */
  status: string | undefined
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
  /** Most recent finished builds (server-wide history, newest first). */
  recent: CiActivityBuild[]
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

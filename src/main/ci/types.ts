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
}

// --- Normalized build state ---

export type CiBuildState = 'queued' | 'running' | 'finished'
export type CiBuildResult = 'SUCCESS' | 'FAILURE' | 'UNKNOWN'

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
}

export interface CiTriggerResult {
  buildId: number
  webUrl: string
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
  kind: 'text' | 'checkbox' | 'select'
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

/** A running or queued build in the server-wide activity view. */
export interface CiActivityBuild {
  id: number
  number: string | undefined
  state: 'running' | 'queued'
  percentageComplete: number | undefined
  webUrl: string
  branchName: string | undefined
  buildTypeId: string
  buildTypeName: string
}

export interface CiActivity {
  running: CiActivityBuild[]
  queued: CiActivityBuild[]
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

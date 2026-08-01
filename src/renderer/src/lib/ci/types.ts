// Renderer-side mirrors of the preload shapes — ambient declarations from
// index.d.ts are not visible to ESLint inside .svelte scripts, so components
// import these structurally-identical types instead.

/** A running, queued or recently finished build in the activity views. */
export interface CiActivityBuild {
  id: number
  number: string | undefined
  state: 'running' | 'queued' | 'finished'
  status: string | undefined
  percentageComplete: number | undefined
  webUrl: string
  branchName: string | undefined
  queuedAt: number | undefined
  startedAt: number | undefined
  finishedAt: number | undefined
  buildTypeId: string
  buildTypeName: string
}

/** Newest build of a branch (per configured job) — the `ci:status` row shape. */
export interface CiBuildStatus {
  id: number
  number: string
  state: 'queued' | 'running' | 'finished'
  status: 'SUCCESS' | 'FAILURE' | 'UNKNOWN'
  percentageComplete: number | undefined
  webUrl: string
  branchName: string | undefined
}

export interface CiBuildTypeStatus {
  buildTypeId: string
  label: string
  build: CiBuildStatus | null
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
}

import type { Result } from 'neverthrow'
import type { CiDispatchConfirmation, CiManager } from './CiManager'
import {
  BUILD_TYPE_ID_PATTERN,
  CI_MAX_BUILD_TYPES,
  CI_MAX_LABEL_LEN,
  CI_MAX_WORKFLOWS,
  parseCiConfig,
} from './config'
import type { CiConfig, CiInputValue, CiRef, CiStatusResponse, CiTriggerRequest } from './types'
import { ciErrorMessage } from './errors'
import { testConnection as ciTestConnection } from './teamcity'
import { CI_TOKEN_MAX, normalizeTeamCityToken } from './token'

// The CI IPC surface, extracted so the AUTHORIZATION contract is unit-testable:
// every repo-scoped channel resolves `payload.repoRoot` through the injected
// `validatePathAccess` (the same workspace gate repoConfig:* uses) and passes only
// the RESOLVED path downstream. A renderer must not be able to point `ci:*` at an
// arbitrary directory — ci:saveConfig writes `<path>/.canopy/config.json`, and the
// other channels would use the stored TeamCity token on a manufactured config.

interface CiIpcEvent {
  sender: { id: number }
}

export interface CiHandlerDeps {
  ipcMain: {
    // Electron's IpcMain.handle is (event, ...args: any[]) — the fake in tests
    // only needs the (event, payload) pair the CI channels actually use.
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    handle: (channel: string, listener: (event: any, payload: any) => unknown) => void
  }
  ciManager: CiManager
  /** Resolves the path and throws unless it belongs to the sender's workspaces. */
  validatePathAccess: (wcId: number, targetPath: string) => Promise<string>
  /** Trusted native confirmation, parented to the invoking window. */
  confirmGitHubDispatch?: (event: CiIpcEvent, details: CiDispatchConfirmation) => Promise<boolean>
}

function unwrapOrThrow<T, E>(result: Result<T, E>, toMessage: (e: E) => string): T {
  if (result.isErr()) throw new Error(toMessage(result.error))
  return result.value
}

// Build type ids are embedded in TeamCity locator expressions. Refs use the provider-neutral
// Git contract below; the TeamCity adapter enforces its stricter locator-safe subset at the sink.
// The id charset is shared with the config parser so the injection defence cannot drift.
const CI_BUILD_TYPE_ID_RE = BUILD_TYPE_ID_PATTERN

// Custom-build properties travel in the JSON body (never in a locator), so values
// only need sane size caps; names get a strict charset anyway.
const CI_PROPERTY_NAME_RE = /^[A-Za-z0-9._-]{1,255}$/
const CI_PROPERTY_VALUE_MAX = 4096
const CI_PROPERTIES_MAX = 100
const CI_JOB_ID_RE = /^[A-Za-z0-9._/-]{1,255}$/
const CI_RUN_ID_RE = /^\d{1,30}$/
const CI_SCHEMA_REVISION_RE = /^[A-Za-z0-9._:-]{1,200}$/

function isValidGitRefName(name: unknown): name is string {
  if (typeof name !== 'string' || name === '' || name.length > 255 || name !== name.trim()) {
    return false
  }
  if (
    name === '@' ||
    name.startsWith('-') ||
    name.startsWith('/') ||
    name.endsWith('/') ||
    name.endsWith('.') ||
    name.includes('//') ||
    name.includes('..') ||
    name.includes('@{')
  ) {
    return false
  }
  if (
    [...name].some((char) => {
      const code = char.charCodeAt(0)
      return code <= 0x20 || code === 0x7f || '~^:?*[\\'.includes(char)
    })
  ) {
    return false
  }
  return !name.split('/').some((part) => part.startsWith('.') || part.endsWith('.lock'))
}

function validateCiProperties(raw: unknown): Array<{ name: string; value: string }> | undefined {
  if (raw === undefined) return undefined
  if (!Array.isArray(raw) || raw.length > CI_PROPERTIES_MAX) {
    throw new Error('Invalid build properties')
  }
  return raw.map((entry) => {
    const { name, value } = (entry ?? {}) as { name?: unknown; value?: unknown }
    if (typeof name !== 'string' || !CI_PROPERTY_NAME_RE.test(name)) {
      throw new Error('Invalid build property name')
    }
    if (typeof value !== 'string' || value.length > CI_PROPERTY_VALUE_MAX) {
      throw new Error(`Invalid value for build property ${name}`)
    }
    return { name, value }
  })
}

function validateRef(raw: unknown): CiRef {
  if (!raw || typeof raw !== 'object') throw new Error('Invalid CI ref')
  const { name, kind } = raw as { name?: unknown; kind?: unknown }
  if (!isValidGitRefName(name)) throw new Error('Invalid CI ref name')
  if (kind !== 'branch' && kind !== 'tag') throw new Error('Invalid CI ref kind')
  return { name, kind }
}

function validateInputs(raw: unknown): Record<string, CiInputValue> {
  if (!raw || typeof raw !== 'object' || Array.isArray(raw)) throw new Error('Invalid CI inputs')
  const entries = Object.entries(raw as Record<string, unknown>)
  if (entries.length > 25 || JSON.stringify(raw).length > 60_000) {
    throw new Error('CI input payload is too large')
  }
  return Object.fromEntries(
    entries.map(([name, value]) => {
      if (!CI_PROPERTY_NAME_RE.test(name)) throw new Error('Invalid CI input name')
      if (typeof value === 'string') {
        if (value.length > 10_000) throw new Error(`Invalid value for CI input ${name}`)
        return [name, value]
      }
      if (typeof value === 'boolean') return [name, value]
      throw new Error(`Invalid value for CI input ${name}`)
    }),
  )
}

export function registerCiHandlers({
  ipcMain,
  ciManager,
  validatePathAccess,
  confirmGitHubDispatch,
}: CiHandlerDeps): void {
  /** Repo-scoped payloads: type-check, then authorize — the resolved path is the
      ONLY form that continues; the renderer-supplied string never reaches CiManager. */
  async function authorizedRepoRoot(event: CiIpcEvent, repoRoot: unknown): Promise<string> {
    if (typeof repoRoot !== 'string' || !repoRoot) {
      throw new Error('repoRoot is required')
    }
    return await validatePathAccess(event.sender.id, repoRoot)
  }

  // Validated CI config of a repo — drives the CI section and both CI modals.
  // Structured result: `invalid` carries the SCOPE so the renderer can gate the
  // recovery routes (file → fix by hand, block → re-save) without re-parsing the
  // formatted message. Not-configured is a normal state (config: null alone).
  ipcMain.handle('ci:config', async (event: CiIpcEvent, payload: { repoRoot: string }) => {
    const repoRoot = await authorizedRepoRoot(event, payload.repoRoot)
    const result = await ciManager.loadConfig(repoRoot)
    if (result.isOk()) return { config: result.value }
    if (result.error._tag === 'CiConfigInvalid') {
      return {
        config: null,
        invalid: {
          scope: result.error.scope,
          message: ciErrorMessage(result.error),
          provider: result.error.provider,
        },
      }
    }
    return { config: null }
  })

  // Read: never throws — the sidebar renders whatever state comes back.
  ipcMain.handle('ci:githubSetup', async (event: CiIpcEvent, payload: { repoRoot: string }) => {
    const repoRoot = await authorizedRepoRoot(event, payload.repoRoot)
    const result = await ciManager.githubSetup(repoRoot)
    return unwrapOrThrow(result, ciErrorMessage)
  })

  ipcMain.handle(
    'ci:testGitHubConnection',
    async (event: CiIpcEvent, payload: { repoRoot: string; token: string }) => {
      const repoRoot = await authorizedRepoRoot(event, payload.repoRoot)
      if (typeof payload.token !== 'string' || payload.token.length > CI_TOKEN_MAX) {
        throw new Error('Invalid GitHub token')
      }
      const token = payload.token.trim()
      if (token.length === 0) {
        throw new Error('Invalid GitHub token')
      }
      const result = await ciManager.testGitHubConnection(repoRoot, token)
      return unwrapOrThrow(result, ciErrorMessage)
    },
  )

  ipcMain.handle(
    'ci:setGitHubCredential',
    async (event: CiIpcEvent, payload: { repoRoot: string; token: string }) => {
      const repoRoot = await authorizedRepoRoot(event, payload.repoRoot)
      if (typeof payload.token !== 'string' || payload.token.length > CI_TOKEN_MAX) {
        throw new Error('Invalid GitHub token')
      }
      const token = payload.token.trim()
      if (token.length === 0) {
        throw new Error('Invalid GitHub token')
      }
      const result = await ciManager.saveGitHubCredential(repoRoot, token)
      return unwrapOrThrow(result, ciErrorMessage)
    },
  )

  ipcMain.handle(
    'ci:jobsStatus',
    async (event: CiIpcEvent, payload: { repoRoot: string; ref: CiRef }) => {
      const repoRoot = await authorizedRepoRoot(event, payload.repoRoot)
      const result = await ciManager.jobsStatus(repoRoot, validateRef(payload.ref))
      return unwrapOrThrow(result, ciErrorMessage)
    },
  )

  ipcMain.handle(
    'ci:jobRefs',
    async (event: CiIpcEvent, payload: { repoRoot: string; jobId: string }) => {
      const repoRoot = await authorizedRepoRoot(event, payload.repoRoot)
      if (typeof payload.jobId !== 'string' || !CI_JOB_ID_RE.test(payload.jobId)) {
        throw new Error('Invalid CI job id')
      }
      const result = await ciManager.jobRefs(repoRoot, payload.jobId)
      return unwrapOrThrow(result, ciErrorMessage)
    },
  )

  ipcMain.handle(
    'ci:jobParameters',
    async (event: CiIpcEvent, payload: { repoRoot: string; jobId: string; ref: CiRef }) => {
      const repoRoot = await authorizedRepoRoot(event, payload.repoRoot)
      if (typeof payload.jobId !== 'string' || !CI_JOB_ID_RE.test(payload.jobId)) {
        throw new Error('Invalid CI job id')
      }
      const result = await ciManager.jobParameters(
        repoRoot,
        payload.jobId,
        validateRef(payload.ref),
      )
      return unwrapOrThrow(result, ciErrorMessage)
    },
  )

  ipcMain.handle(
    'ci:triggerJob',
    async (
      event: CiIpcEvent,
      payload: {
        repoRoot: string
        jobId: string
        ref: CiRef
        schemaRevision?: string
        inputs: Record<string, CiInputValue>
      },
    ) => {
      const requestedRepoRoot = payload.repoRoot
      const repoRoot = await authorizedRepoRoot(event, requestedRepoRoot)
      if (typeof payload.jobId !== 'string' || !CI_JOB_ID_RE.test(payload.jobId)) {
        throw new Error('Invalid CI job id')
      }
      if (
        payload.schemaRevision !== undefined &&
        (typeof payload.schemaRevision !== 'string' ||
          !CI_SCHEMA_REVISION_RE.test(payload.schemaRevision))
      ) {
        throw new Error('Invalid workflow schema revision')
      }
      const request: CiTriggerRequest = {
        jobId: payload.jobId,
        ref: validateRef(payload.ref),
        schemaRevision: payload.schemaRevision,
        inputs: validateInputs(payload.inputs),
      }
      const confirm = confirmGitHubDispatch
        ? async (details: CiDispatchConfirmation): Promise<boolean> => {
            const rootBeforeConfirmation = await authorizedRepoRoot(event, requestedRepoRoot)
            if (rootBeforeConfirmation !== repoRoot)
              throw new Error('Repository authorization changed')
            const accepted = await confirmGitHubDispatch(event, details)
            if (!accepted) return false
            const rootAfterConfirmation = await authorizedRepoRoot(event, requestedRepoRoot)
            if (rootAfterConfirmation !== repoRoot)
              throw new Error('Repository authorization changed')
            return true
          }
        : undefined
      const result = await ciManager.triggerJob(repoRoot, request, confirm)
      return result.match(
        (value) => ({ ok: true as const, value }),
        (error) => ({
          ok: false as const,
          error: { code: error._tag, message: ciErrorMessage(error) },
        }),
      )
    },
  )

  ipcMain.handle('ci:runActivity', async (event: CiIpcEvent, payload: { repoRoot: string }) => {
    const repoRoot = await authorizedRepoRoot(event, payload.repoRoot)
    const result = await ciManager.runActivity(repoRoot)
    return unwrapOrThrow(result, ciErrorMessage)
  })

  ipcMain.handle(
    'ci:run',
    async (event: CiIpcEvent, payload: { repoRoot: string; runId: string }) => {
      const repoRoot = await authorizedRepoRoot(event, payload.repoRoot)
      if (typeof payload.runId !== 'string' || !CI_RUN_ID_RE.test(payload.runId)) {
        throw new Error('Invalid CI run id')
      }
      const result = await ciManager.runById(repoRoot, payload.runId)
      return unwrapOrThrow(result, ciErrorMessage)
    },
  )

  ipcMain.handle(
    'ci:status',
    async (event: CiIpcEvent, payload: { repoRoot: string; branch: string }) => {
      const repoRoot = await authorizedRepoRoot(event, payload.repoRoot)
      // An invalid EXISTING block surfaces through ci:config (which answers
      // { config: null, invalid: { scope, message } }) — the sidebar never polls
      // ci:status in that state, so this call treats every load failure alike.
      const config = await ciManager.loadConfig(repoRoot).unwrapOr(null)
      if (!config) return { configured: false, rows: [] } satisfies CiStatusResponse
      if (!isValidGitRefName(payload.branch)) {
        return {
          configured: true,
          baseUrl: config.baseUrl,
          rows: [],
          error: 'Invalid branch name',
        } satisfies CiStatusResponse
      }

      // Pass the loaded config down — statusFor doesn't re-read .canopy/config.json.
      const result = await ciManager.statusFor(config, payload.branch)
      return result.match(
        (rows): CiStatusResponse => ({
          configured: true,
          baseUrl: config.baseUrl,
          hasToken: true,
          rows,
        }),
        (error): CiStatusResponse => ({
          configured: true,
          baseUrl: config.baseUrl,
          hasToken: error._tag !== 'CiAuthMissing',
          rows: [],
          error: ciErrorMessage(error),
        }),
      )
    },
  )

  ipcMain.handle(
    'ci:trigger',
    async (
      event: CiIpcEvent,
      payload: {
        repoRoot: string
        buildTypeId: string
        branch: string
        properties?: Array<{ name: string; value: string }>
      },
    ) => {
      const repoRoot = await authorizedRepoRoot(event, payload.repoRoot)
      if (
        typeof payload.buildTypeId !== 'string' ||
        !CI_BUILD_TYPE_ID_RE.test(payload.buildTypeId)
      ) {
        throw new Error('Invalid build type id')
      }
      if (!isValidGitRefName(payload.branch)) {
        throw new Error('Invalid branch name')
      }
      const properties = validateCiProperties(payload.properties)
      const result = await ciManager.trigger(
        repoRoot,
        payload.buildTypeId,
        payload.branch,
        properties,
      )
      return unwrapOrThrow(result, ciErrorMessage)
    },
  )

  // Activity for build types selected in the repository's CI configuration.
  ipcMain.handle('ci:activity', async (event: CiIpcEvent, payload: { repoRoot: string }) => {
    const repoRoot = await authorizedRepoRoot(event, payload.repoRoot)
    const result = await ciManager.activity(repoRoot)
    return unwrapOrThrow(result, ciErrorMessage)
  })

  // Branch list of a configured build type — feeds the Run job dialog.
  ipcMain.handle(
    'ci:branches',
    async (event: CiIpcEvent, payload: { repoRoot: string; buildTypeId: string }) => {
      const repoRoot = await authorizedRepoRoot(event, payload.repoRoot)
      if (
        typeof payload.buildTypeId !== 'string' ||
        !CI_BUILD_TYPE_ID_RE.test(payload.buildTypeId)
      ) {
        throw new Error('Invalid build type id')
      }
      const result = await ciManager.branches(repoRoot, payload.buildTypeId)
      return unwrapOrThrow(result, ciErrorMessage)
    },
  )

  // "Run custom build" prompt parameters of a configured build type.
  ipcMain.handle(
    'ci:buildParameters',
    async (event: CiIpcEvent, payload: { repoRoot: string; buildTypeId: string }) => {
      const repoRoot = await authorizedRepoRoot(event, payload.repoRoot)
      if (
        typeof payload.buildTypeId !== 'string' ||
        !CI_BUILD_TYPE_ID_RE.test(payload.buildTypeId)
      ) {
        throw new Error('Invalid build type id')
      }
      const result = await ciManager.promptParameters(repoRoot, payload.buildTypeId)
      return unwrapOrThrow(result, ciErrorMessage)
    },
  )

  // Init flow: the URL comes from the Settings form (no `ci` block exists yet) — the
  // same trust level as taskTracker:testNewConnection. The token never leaves the
  // keychain; only http(s) origins are accepted. NOT repo-scoped: no path involved.
  ipcMain.handle('ci:listBuildTypes', async (_event: CiIpcEvent, payload: { baseUrl: string }) => {
    if (typeof payload.baseUrl !== 'string' || !payload.baseUrl) {
      throw new Error('baseUrl is required')
    }
    const parsed = new URL(payload.baseUrl)
    if (!['http:', 'https:'].includes(parsed.protocol)) {
      throw new Error('Base URL must use http:// or https://')
    }
    const result = await ciManager.listBuildTypes(payload.baseUrl.replace(/\/$/, ''))
    return unwrapOrThrow(result, ciErrorMessage)
  })

  // Write (or remove, with null) the repo's `ci` block from the Settings configurator.
  ipcMain.handle(
    'ci:saveConfig',
    async (
      event: CiIpcEvent,
      payload: {
        repoRoot: string
        ci:
          | {
              provider?: 'teamcity'
              baseUrl: string
              buildTypes: Array<{ id: string; label: string }>
            }
          | {
              provider: 'github-actions'
              baseUrl: string
              repository: string
              workflows: Array<{ path: string; label: string }>
            }
          | null
      },
    ) => {
      const repoRoot = await authorizedRepoRoot(event, payload.repoRoot)
      let ci: CiConfig | null = null
      if (payload.ci !== null) {
        if (payload.ci.provider === 'github-actions') {
          if (!Array.isArray(payload.ci.workflows) || payload.ci.workflows.length === 0) {
            throw new Error('Select at least one workflow')
          }
          if (payload.ci.workflows.length > CI_MAX_WORKFLOWS) {
            throw new Error('Too many workflows')
          }
          const parsedGitHub = parseCiConfig(payload.ci)
          if (
            parsedGitHub.config?.provider !== 'github-actions' ||
            parsedGitHub.config.workflows.length !== payload.ci.workflows.length ||
            parsedGitHub.config.droppedInvalid ||
            parsedGitHub.config.droppedOverCap
          ) {
            throw new Error('Invalid GitHub Actions configuration')
          }
          ci = parsedGitHub.config
        } else {
          if (typeof payload.ci?.baseUrl !== 'string') throw new Error('Invalid base URL')
          const parsed = new URL(payload.ci.baseUrl)
          if (!['http:', 'https:'].includes(parsed.protocol)) {
            throw new Error('Base URL must use http:// or https://')
          }
          if (!Array.isArray(payload.ci.buildTypes) || payload.ci.buildTypes.length === 0) {
            throw new Error('Select at least one build configuration')
          }
          if (payload.ci.buildTypes.length > CI_MAX_BUILD_TYPES) {
            throw new Error('Too many build configurations')
          }
          const buildTypes = payload.ci.buildTypes.map((bt) => {
            if (typeof bt?.id !== 'string' || !CI_BUILD_TYPE_ID_RE.test(bt.id)) {
              throw new Error('Invalid build type id')
            }
            const label =
              typeof bt.label === 'string' ? bt.label.trim().slice(0, CI_MAX_LABEL_LEN) : ''
            return { id: bt.id, label: label || bt.id }
          })
          ci = {
            provider: 'teamcity',
            baseUrl: payload.ci.baseUrl.replace(/\/$/, ''),
            buildTypes,
          }
        }
      }
      const result = await ciManager.saveConfig(repoRoot, ci)
      return unwrapOrThrow(result, ciErrorMessage)
    },
  )

  ipcMain.handle(
    'ci:build',
    async (event: CiIpcEvent, payload: { repoRoot: string; buildId: number }) => {
      const repoRoot = await authorizedRepoRoot(event, payload.repoRoot)
      if (typeof payload.buildId !== 'number' || !Number.isInteger(payload.buildId)) {
        throw new Error('Invalid build id')
      }
      const result = await ciManager.build(repoRoot, payload.buildId)
      return unwrapOrThrow(result, ciErrorMessage)
    },
  )

  // Settings configurator connection test: candidate URL and token both come from the
  // form (the `ci` block may not exist yet) — same trust level as
  // taskTracker:testNewConnection. Nothing is stored. NOT repo-scoped.
  ipcMain.handle(
    'ci:testNewConnection',
    async (_event: CiIpcEvent, payload: { baseUrl: string; token: string }) => {
      if (typeof payload.baseUrl !== 'string' || !payload.baseUrl) {
        throw new Error('baseUrl is required')
      }
      const parsed = new URL(payload.baseUrl)
      if (!['http:', 'https:'].includes(parsed.protocol)) {
        throw new Error('Base URL must use http:// or https://')
      }
      const token = normalizeTeamCityToken(payload.token)
      const result = await ciTestConnection(payload.baseUrl.replace(/\/$/, ''), token)
      return unwrapOrThrow(result, ciErrorMessage)
    },
  )
}

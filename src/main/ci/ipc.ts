import type { Result } from 'neverthrow'
import type { CiManager } from './CiManager'
import { BUILD_TYPE_ID_PATTERN } from './config'
import type { CiConfig, CiStatusResponse } from './types'
import { ciErrorMessage } from './errors'
import { testConnection as ciTestConnection } from './teamcity'

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
}

function unwrapOrThrow<T, E>(result: Result<T, E>, toMessage: (e: E) => string): T {
  if (result.isErr()) throw new Error(toMessage(result.error))
  return result.value
}

// Identifier charsets: build type ids and branch names are embedded in TeamCity
// locator expressions — reject anything that could escape the parenthesized value
// (`(`, `)`, `,`, `:`). The renderer is the untrusted boundary. The id charset is
// shared with the config parser — one definition, or the injection defence drifts.
const CI_BUILD_TYPE_ID_RE = BUILD_TYPE_ID_PATTERN
const CI_BRANCH_RE = /^[A-Za-z0-9._/-]{1,255}$/

// Custom-build properties travel in the JSON body (never in a locator), so values
// only need sane size caps; names get a strict charset anyway.
const CI_PROPERTY_NAME_RE = /^[A-Za-z0-9._-]{1,255}$/
const CI_PROPERTY_VALUE_MAX = 4096
const CI_PROPERTIES_MAX = 100

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

export function registerCiHandlers({
  ipcMain,
  ciManager,
  validatePathAccess,
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
        invalid: { scope: result.error.scope, message: ciErrorMessage(result.error) },
      }
    }
    return { config: null }
  })

  // Read: never throws — the sidebar renders whatever state comes back.
  ipcMain.handle(
    'ci:status',
    async (event: CiIpcEvent, payload: { repoRoot: string; branch: string }) => {
      const repoRoot = await authorizedRepoRoot(event, payload.repoRoot)
      if (typeof payload.branch !== 'string' || !CI_BRANCH_RE.test(payload.branch)) {
        return { configured: false, rows: [] } satisfies CiStatusResponse
      }
      // An invalid EXISTING block surfaces through ci:config (which answers
      // { config: null, invalid: { scope, message } }) — the sidebar never polls
      // ci:status in that state, so this call treats every load failure alike.
      const config = await ciManager.loadConfig(repoRoot).unwrapOr(null)
      if (!config) return { configured: false, rows: [] } satisfies CiStatusResponse

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
      if (typeof payload.branch !== 'string' || !CI_BRANCH_RE.test(payload.branch)) {
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

  // Server-wide activity (running + queued) of the repo's CI server.
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
        ci: { baseUrl: string; buildTypes: Array<{ id: string; label: string }> } | null
      },
    ) => {
      const repoRoot = await authorizedRepoRoot(event, payload.repoRoot)
      let ci: CiConfig | null = null
      if (payload.ci !== null) {
        if (typeof payload.ci?.baseUrl !== 'string') throw new Error('Invalid base URL')
        const parsed = new URL(payload.ci.baseUrl)
        if (!['http:', 'https:'].includes(parsed.protocol)) {
          throw new Error('Base URL must use http:// or https://')
        }
        if (!Array.isArray(payload.ci.buildTypes) || payload.ci.buildTypes.length === 0) {
          throw new Error('Select at least one build configuration')
        }
        if (payload.ci.buildTypes.length > 50) {
          throw new Error('Too many build configurations')
        }
        const buildTypes = payload.ci.buildTypes.map((bt) => {
          if (typeof bt?.id !== 'string' || !CI_BUILD_TYPE_ID_RE.test(bt.id)) {
            throw new Error('Invalid build type id')
          }
          const label = typeof bt.label === 'string' ? bt.label.trim().slice(0, 100) : ''
          return { id: bt.id, label: label || bt.id }
        })
        ci = {
          provider: 'teamcity',
          baseUrl: payload.ci.baseUrl.replace(/\/$/, ''),
          buildTypes,
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
      if (typeof payload.token !== 'string' || !payload.token) {
        throw new Error('Token is required')
      }
      const result = await ciTestConnection(payload.baseUrl.replace(/\/$/, ''), payload.token)
      return unwrapOrThrow(result, ciErrorMessage)
    },
  )
}

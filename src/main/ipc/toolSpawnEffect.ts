import os from 'os'
import { Data, Effect, Scope } from 'effect'
import type { BrowserWindow, IpcMainInvokeEvent, WebContents } from 'electron'
import type { PtyManager } from '../pty/PtyManager'
import type { WsBridge } from '../pty/WsBridge'
import type { WorkspaceStore } from '../db/WorkspaceStore'
import type { PreferencesStore } from '../db/PreferencesStore'
import type { ToolRegistry } from '../tools/ToolRegistry'
import type { AgentSessionManager } from '../agents/AgentSessionManager'
import type { WindowManager } from '../WindowManager'
import type { TmuxManager } from '../pty/TmuxManager'
import type { ProfileStore } from '../profiles/ProfileStore'
import type { AgentType, PreferencesReader } from '../agents/types'
import { profileToReader } from '../profiles/ProfileStore'
import { profileErrorMessage } from '../profiles/errors'
import { errorMessage } from '../errors'
import { tmuxSessionName, isCanopyTmuxSession } from '../pty/tmuxNaming'

// IPC tool-spawn Effect program. Extracted from handlers.ts so unit tests can
// import this module without dragging in the full Electron main process import
// graph (TmuxManager → @electron-toolkit/utils → electron.app, etc).

export interface ToolSpawnPayload {
  toolId: string
  worktreePath: string
  cols?: number
  rows?: number
  workspaceName?: string
  branch?: string
  resumeSessionId?: string
  profileId?: string
  tmuxSessionName?: string
}

export interface ToolSpawnResult {
  sessionId: string
  wsUrl: string
  toolId: string
  toolName: string
  tmuxSessionName?: string
}

export type ToolSpawnStage =
  | 'tool-resolution'
  | 'agent-profile'
  | 'agent-session'
  | 'agent-args'
  | 'tmux-availability'
  | 'tmux-session'
  | 'pty-spawn'
  | 'agent-rekey'
  | 'websocket-bridge'
  | 'window-tracking'

export class ToolSpawnError extends Data.TaggedError('ToolSpawnError')<{
  readonly stage: ToolSpawnStage
  readonly message: string
  readonly cause?: unknown
}> {}

function toolSpawnError(stage: ToolSpawnStage, message: string, cause?: unknown): ToolSpawnError {
  return new ToolSpawnError({ stage, message, cause })
}

function toolSpawnCause(stage: ToolSpawnStage, cause: unknown): ToolSpawnError {
  return toolSpawnError(stage, errorMessage(cause), cause)
}

export interface ToolSpawnEffectDeps {
  ptyManager: PtyManager
  wsBridge: WsBridge
  workspaceStore: WorkspaceStore
  preferencesStore: PreferencesStore
  toolRegistry: ToolRegistry
  agentSessionManager: AgentSessionManager
  windowManager: WindowManager
  tmuxManager: TmuxManager
  profileStore: ProfileStore
  /**
   * Resolves the BrowserWindow associated with a sender. Optional: when
   * omitted, callers may inject one. The production wrapper in handlers.ts
   * passes BrowserWindow.fromWebContents. Tests may inject a fake.
   */
  getSenderWindow?: (sender: WebContents) => BrowserWindow | null
}

interface ToolSpawnState {
  command: string
  args: string[]
  env?: Record<string, string>
  agentTempId?: string
  tmuxSessionName?: string
}

interface AgentSessionResource {
  args: string[]
  env: Record<string, string>
  agentTempId: string
  rekeyTo: (sessionId: string) => void
  commit: () => void
}

function resolveShellArgs(): string[] {
  if (os.platform() === 'win32') return []
  return ['--login']
}

function createAgentSessionResource(
  deps: ToolSpawnEffectDeps,
  toolId: AgentType,
  payload: ToolSpawnPayload,
  senderWindow: BrowserWindow,
  initialArgs: string[],
): Effect.Effect<AgentSessionResource, ToolSpawnError, Scope.Scope> {
  return Effect.gen(function* () {
    const prefsReader = yield* Effect.tryPromise({
      try: async (): Promise<PreferencesReader> => {
        if (!payload.profileId) return deps.preferencesStore

        const profileResult = await deps.profileStore.getInternal(payload.profileId)
        if (profileResult.isErr()) {
          throw new Error(profileErrorMessage(profileResult.error))
        }

        const profile = profileResult.value
        if (profile.agentType !== toolId) {
          throw new Error(`Profile ${profile.name} is for ${profile.agentType}, not ${toolId}`)
        }

        return profileToReader(profile, deps.preferencesStore)
      },
      catch: (cause) => toolSpawnCause('agent-profile', cause),
    })

    const settingsOverrides = yield* Effect.try({
      try: (): Record<string, unknown> | undefined => {
        const settingsJsonRaw = prefsReader.get(`${toolId}.settingsJson`)
        if (!settingsJsonRaw) return undefined

        try {
          return JSON.parse(settingsJsonRaw) as Record<string, unknown>
        } catch {
          return undefined
        }
      },
      catch: (cause) => toolSpawnCause('agent-profile', cause),
    })

    let committed = false
    let sessionId: string | undefined
    const agentSession = yield* Effect.acquireRelease(
      Effect.tryPromise({
        try: () =>
          deps.agentSessionManager.createSession(
            toolId,
            payload.worktreePath,
            payload.workspaceName ?? '',
            payload.branch ?? null,
            senderWindow,
            settingsOverrides,
          ),
        catch: (cause) => toolSpawnCause('agent-session', cause),
      }).pipe(
        Effect.tap((session) =>
          Effect.sync(() => {
            sessionId = session.tempId
          }),
        ),
      ),
      () =>
        Effect.sync(() => {
          if (!committed && sessionId) deps.agentSessionManager.destroySession(sessionId)
        }),
    )

    return yield* Effect.try({
      try: (): AgentSessionResource => {
        const args = [...agentSession.settingsArgs, ...initialArgs]
        if (payload.resumeSessionId) {
          args.push(...deps.agentSessionManager.getResumeArgs(toolId, payload.resumeSessionId))
        }
        args.push(...deps.agentSessionManager.getCliArgs(toolId, prefsReader))

        return {
          args,
          env: {
            CANOPY_HOOK_PORT: String(agentSession.hookPort),
            CANOPY_HOOK_PATH: agentSession.hookPath,
            CANOPY_HOOK_TOKEN: agentSession.hookAuthToken,
            ...agentSession.settingsEnv,
            ...deps.agentSessionManager.getEnvVars(toolId, prefsReader),
          },
          agentTempId: agentSession.tempId,
          rekeyTo: (nextSessionId: string) => {
            sessionId = nextSessionId
          },
          commit: () => {
            committed = true
          },
        }
      },
      catch: (cause) => toolSpawnCause('agent-args', cause),
    })
  })
}

interface TmuxSessionResource {
  name?: string
  commit: () => void
}

function createTmuxSessionResource(
  deps: ToolSpawnEffectDeps,
  payload: ToolSpawnPayload,
  state: ToolSpawnState,
): Effect.Effect<TmuxSessionResource, ToolSpawnError, Scope.Scope> {
  return Effect.gen(function* () {
    const tmuxEnabled = yield* Effect.try({
      try: () => deps.preferencesStore.get('tmux.enabled') === 'true',
      catch: (cause) => toolSpawnCause('tmux-availability', cause),
    })
    if (!tmuxEnabled) return { commit: () => undefined }

    const tmuxAvailable = yield* Effect.tryPromise({
      try: () => deps.tmuxManager.isAvailable(),
      catch: (cause) => toolSpawnCause('tmux-availability', cause),
    })
    if (!tmuxAvailable) return { commit: () => undefined }

    // If the renderer passed back a saved session name, check if it's still alive.
    // When it is, attach without taking ownership — no finalizer, no newSession.
    if (payload.tmuxSessionName != null) {
      const alive = yield* Effect.tryPromise({
        try: () => deps.tmuxManager.hasSession(payload.tmuxSessionName!),
        catch: (cause) => toolSpawnCause('tmux-session', cause),
      })
      if (alive) return { name: payload.tmuxSessionName!, commit: () => undefined }
    }

    const sessionInfo = yield* Effect.try({
      try: () => {
        const ws = deps.workspaceStore.getByPath(payload.worktreePath)
        const wsId = ws?.id ?? 'default'
        const name = tmuxSessionName(wsId)
        const mouse = deps.preferencesStore.get('tmux.mouse') === 'true'
        return { name, mouse }
      },
      catch: (cause) => toolSpawnCause('tmux-session', cause),
    })

    let committed = false
    yield* Effect.acquireRelease(
      Effect.tryPromise({
        try: () =>
          deps.tmuxManager.newSession({
            name: sessionInfo.name,
            cwd: payload.worktreePath,
            shell: state.command,
            shellArgs: state.args,
            cols: payload.cols,
            rows: payload.rows,
            mouse: sessionInfo.mouse,
            env: state.env,
          }),
        catch: (cause) => toolSpawnCause('tmux-session', cause),
      }),
      () => {
        // Finalizer must be infallible: a failure here would mask the primary
        // setup error and Effect would surface a defect from a release step.
        // We only kill canopy-owned sessions and only when we never committed.
        if (committed || !isCanopyTmuxSession(sessionInfo.name)) {
          return Effect.void
        }
        return Effect.tryPromise({
          try: () => deps.tmuxManager.killSession(sessionInfo.name),
          catch: (cause) => cause,
        }).pipe(Effect.catchAll(() => Effect.void))
      },
    )

    return {
      name: sessionInfo.name,
      commit: () => {
        committed = true
      },
    }
  })
}

export function createToolSpawnEffect(
  deps: ToolSpawnEffectDeps,
  event: IpcMainInvokeEvent,
  payload: ToolSpawnPayload,
): Effect.Effect<ToolSpawnResult, ToolSpawnError, never> {
  return Effect.scoped(
    Effect.gen(function* () {
      const sender = event.sender
      const tool = yield* Effect.try({
        try: () => deps.toolRegistry.get(payload.toolId),
        catch: (cause) => toolSpawnCause('tool-resolution', cause),
      })
      if (!tool) {
        return yield* Effect.fail(
          toolSpawnError('tool-resolution', `Unknown tool: ${payload.toolId}`),
        )
      }

      const isShell = tool.id === 'shell' || tool.command === 'shell'
      const isAgent = deps.agentSessionManager.isAgentTool(tool.id)
      const state = yield* Effect.try({
        try: (): ToolSpawnState => ({
          command: deps.toolRegistry.resolveCommand(tool),
          args: isShell ? resolveShellArgs() : [...tool.args],
        }),
        catch: (cause) => toolSpawnCause('tool-resolution', cause),
      })

      let agentResource: AgentSessionResource | undefined
      if (isAgent) {
        const senderWindow = deps.getSenderWindow ? deps.getSenderWindow(sender) : null
        if (!senderWindow) {
          return yield* Effect.fail(toolSpawnError('agent-session', 'No window for agent session'))
        }

        agentResource = yield* createAgentSessionResource(
          deps,
          tool.id as AgentType,
          payload,
          senderWindow,
          state.args,
        )
        state.args = agentResource.args
        state.env = agentResource.env
        state.agentTempId = agentResource.agentTempId
      }

      const tmuxResource = yield* createTmuxSessionResource(deps, payload, state)
      state.tmuxSessionName = tmuxResource.name
      if (state.tmuxSessionName) {
        const attach = yield* Effect.try({
          try: () => deps.tmuxManager.attachArgs(state.tmuxSessionName as string),
          catch: (cause) => toolSpawnCause('tmux-session', cause),
        })
        state.command = attach.command
        state.args = attach.args
      }

      let ptyCommitted = false
      const session = yield* Effect.acquireRelease(
        Effect.try({
          try: () =>
            deps.ptyManager.spawn({
              command: state.command,
              args: state.args,
              cwd: payload.worktreePath,
              cols: payload.cols,
              rows: payload.rows,
              env: state.env,
              tmuxSessionName: state.tmuxSessionName,
            }),
          catch: (cause) => toolSpawnCause('pty-spawn', cause),
        }),
        (session) =>
          Effect.sync(() => {
            if (!ptyCommitted) deps.ptyManager.kill(session.id)
          }),
      )

      if (isAgent && state.agentTempId) {
        yield* Effect.try({
          try: () => {
            deps.agentSessionManager.rekey(state.agentTempId as string, session.id)
            agentResource?.rekeyTo(session.id)
          },
          catch: (cause) => toolSpawnCause('agent-rekey', cause),
        })
      }

      let wsCommitted = false
      const wsUrl = yield* Effect.acquireRelease(
        Effect.tryPromise({
          try: () => deps.wsBridge.create(session.id, session.pty),
          catch: (cause) => toolSpawnCause('websocket-bridge', cause),
        }),
        () =>
          Effect.sync(() => {
            if (!wsCommitted) deps.wsBridge.destroy(session.id)
          }),
      )

      let trackingCommitted = false
      yield* Effect.acquireRelease(
        Effect.try({
          try: () => deps.windowManager.trackPtySession(sender.id, session.id),
          catch: (cause) => toolSpawnCause('window-tracking', cause),
        }),
        () =>
          Effect.sync(() => {
            if (!trackingCommitted) deps.windowManager.untrackPtySession(sender.id, session.id)
          }),
      )

      session.pty.onExit(({ exitCode, signal }) => {
        if (!sender.isDestroyed()) {
          sender.send('pty:exit', {
            sessionId: session.id,
            exitCode,
            signal,
            tmuxSessionName: session.tmuxSessionName,
          })
        }
        deps.windowManager.untrackPtySession(sender.id, session.id)
        if (isAgent) {
          deps.agentSessionManager.destroySession(session.id)
        }
      })

      ptyCommitted = true
      wsCommitted = true
      trackingCommitted = true
      agentResource?.commit()
      tmuxResource.commit()

      return {
        sessionId: session.id,
        wsUrl,
        toolId: tool.id,
        toolName: tool.name,
        tmuxSessionName: state.tmuxSessionName,
      }
    }),
  )
}

import { match, P } from 'ts-pattern'
import { copyFileSync, mkdirSync, rmSync } from 'fs'
import { join } from 'path'
import { randomUUID } from 'crypto'
import { is } from '@electron-toolkit/utils'
import type {
  AgentAdapter,
  NormalizedEventName,
  NormalizedHookEvent,
  NormalizedStatusData,
  PreferencesReader,
} from '../types'
import type { SessionStatusType } from '../../notch/types'
import { BLOCKED_ENV_VARS } from '../../security/envBlocklist'
import { summarizeToolInput } from '../utils'

const EVENT_MAP: Record<string, NormalizedEventName> = {
  SessionCreated: 'SessionStart',
  SessionBusy: 'PromptSubmit',
  SessionStatusIdle: 'Idle',
  SessionIdle: 'Idle',
  SessionError: 'IdleFailure',
  SessionDeleted: 'SessionEnd',
  SessionCompacting: 'BeforeCompact',
  SessionCompacted: 'AfterCompact',
  ToolExecuteBefore: 'BeforeToolUse',
  ToolExecuteAfter: 'AfterToolUse',
  PermissionAsked: 'PermissionRequest',
  TodoUpdated: 'Notification',
}

const INTERNAL_BLOCKED = new Set([
  'CANOPY_HOOK_PORT',
  'CANOPY_HOOK_TOKEN',
  'ELECTRON_RUN_AS_NODE',
  'OPENCODE_CONFIG_DIR',
])

export const opencodeAdapter: AgentAdapter = {
  agentType: 'opencode',
  toolId: 'opencode',

  busyEvents: new Set(['SessionBusy', 'ToolExecuteBefore', 'PermissionAsked', 'SessionCompacting']),
  idleEvents: new Set(['SessionStatusIdle', 'SessionIdle', 'SessionError', 'SessionDeleted']),

  // eslint-disable-next-line @typescript-eslint/no-unused-vars
  setupSettings(settingsPath, _worktreePath, _hookScriptPath, _statusLineScriptPath, _overrides) {
    // Create a per-session config dir inside Canopy's userData with the bridge plugin.
    // OPENCODE_CONFIG_DIR is an additive search path — OpenCode discovers plugins/
    // inside it alongside the user's own ~/.config/opencode/ config.
    // This avoids writing to the user's external OpenCode config directory.
    const configDir = join(settingsPath, '..', `opencode-config-${randomUUID()}`)
    const pluginsDir = join(configDir, 'plugins')
    mkdirSync(pluginsDir, { recursive: true })

    const bridgeSource = is.dev
      ? join(process.cwd(), 'resources', 'opencode-canopy-bridge.ts')
      : join(process.resourcesPath, 'app.asar.unpacked', 'resources', 'opencode-canopy-bridge.ts')
    copyFileSync(bridgeSource, join(pluginsDir, 'canopy-bridge.ts'))

    return {
      args: [],
      env: { OPENCODE_CONFIG_DIR: configDir },
      cleanup: () => {
        try {
          rmSync(configDir, { recursive: true, force: true })
        } catch {
          /* best-effort */
        }
      },
    }
  },

  normalizeEvent(raw: Record<string, unknown>): NormalizedHookEvent {
    const rawName = (raw.hook_event_name as string) ?? ''
    let event = EVENT_MAP[rawName] ?? 'Unknown'
    const toolName = raw.tool_name as string | undefined

    // OpenCode's "question" tool = user input needed → treat like permission request
    if (rawName === 'ToolExecuteBefore' && toolName === 'question') {
      event = 'PermissionRequest'
    }

    // Attach OpenCode todos as extra data for task list rendering
    let extra: Record<string, unknown> | undefined
    if (rawName === 'TodoUpdated' && Array.isArray(raw.todos)) {
      extra = { opencodeTodos: raw.todos }
    }

    return {
      agentType: 'opencode',
      sessionId: (raw.session_id as string) ?? '',
      event,
      rawEventName: rawName,
      toolName,
      toolInput: raw.tool_input as Record<string, unknown> | undefined,
      toolResponse: raw.tool_response as string | undefined,
      error: raw.error as string | undefined,
      errorDetails: raw.error_details as string | undefined,
      message: raw.message as string | undefined,
      title: raw.title as string | undefined,
      notificationType: raw.notification_type as string | undefined,
      agentId: raw.agent_id as string | undefined,
      agentSubtype: raw.agent_type as string | undefined,
      reason: raw.reason as string | undefined,
      model: raw.model as string | undefined,
      permissionMode: raw.permission_mode as string | undefined,
      compactSummary: raw.compact_summary as string | undefined,
      prompt: raw.prompt as string | undefined,
      taskId: raw.task_id as string | undefined,
      taskSubject: raw.task_subject as string | undefined,
      taskDescription: raw.task_description as string | undefined,
      teammateName: raw.teammate_name as string | undefined,
      extra,
      teamName: raw.team_name as string | undefined,
    }
  },

  // eslint-disable-next-line @typescript-eslint/no-unused-vars
  normalizeStatus(_raw: Record<string, unknown>): NormalizedStatusData {
    // OpenCode does not expose a status line mechanism
    return {}
  },

  buildCliArgs(prefs: PreferencesReader): string[] {
    const args: string[] = []
    const model = prefs.get('opencode.model')

    // OpenCode uses --model provider/model format
    if (model) args.push('--model', model)

    return args
  },

  buildEnvVars(prefs: PreferencesReader): Record<string, string> {
    const env: Record<string, string> = {}

    const apiKey = prefs.get('opencode.apiKey')
    const customEnv = prefs.get('opencode.customEnv')
    const settingsJson = prefs.get('opencode.settingsJson')

    // OpenCode is provider-agnostic; the API key env var depends on the model chosen.
    // Users can set the right env var via customEnv. If apiKey is set, default to ANTHROPIC_API_KEY.
    if (apiKey) env.ANTHROPIC_API_KEY = apiKey

    // Pass config overrides via OPENCODE_CONFIG_CONTENT
    if (settingsJson) env.OPENCODE_CONFIG_CONTENT = settingsJson

    if (customEnv) {
      try {
        const parsed = JSON.parse(customEnv)
        for (const [k, v] of Object.entries(parsed)) {
          if (
            typeof v === 'string' &&
            !BLOCKED_ENV_VARS.has(k.toUpperCase()) &&
            !INTERNAL_BLOCKED.has(k.toUpperCase())
          ) {
            env[k] = v
          }
        }
      } catch {
        // Invalid JSON
      }
    }

    return env
  },

  buildResumeArgs(resumeSessionId: string): string[] {
    return ['--continue', '--session', resumeSessionId]
  },

  buildSessionContext(worktreePath: string, workspaceName: string, branch: string | null): string {
    let ctx = `Working in canopy workspace '${workspaceName}'`
    if (branch) {
      ctx += `, worktree '${branch}' (branch: ${branch})`
    }
    ctx += `.\nProject root: ${worktreePath}.`
    return ctx
  },

  formatNotification(event: NormalizedHookEvent): { title: string; body: string } | null {
    if (event.event !== 'PermissionRequest') return null
    const body = event.toolName
      ? `${event.toolName}: ${summarizeToolInput(event.toolInput)}`
      : 'A tool requires your approval'
    return { title: 'OpenCode — Permission Required', body }
  },

  toNotchStatus(event: NormalizedHookEvent): { status: SessionStatusType; detail?: string } | null {
    const toolDetail = event.toolName
      ? `${event.toolName}: ${summarizeToolInput(event.toolInput)}`
      : undefined

    return match(event.event)
      .with(P.union('SessionStart', 'Idle'), () => ({ status: 'idle' as const }))
      .with(P.union('PromptSubmit', 'AfterToolUse', 'AfterCompact'), () => ({
        status: 'thinking' as const,
      }))
      .with('BeforeToolUse', () => ({ status: 'toolCalling' as const, detail: toolDetail }))
      .with('PermissionRequest', () => ({
        status: 'waitingPermission' as const,
        detail: toolDetail,
      }))
      .with('BeforeCompact', () => ({ status: 'compacting' as const }))
      .with('IdleFailure', () => ({ status: 'error' as const, detail: event.error }))
      .with('SessionEnd', () => ({ status: 'ended' as const, detail: event.reason }))
      .otherwise(() => null)
  },
}

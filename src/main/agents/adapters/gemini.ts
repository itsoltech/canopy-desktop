import { match, P } from 'ts-pattern'
import os from 'os'
import {
  readFileSync,
  writeFileSync,
  readdirSync,
  existsSync,
  mkdirSync,
  symlinkSync,
  rmSync,
} from 'fs'
import { join, dirname } from 'path'
import { randomUUID } from 'crypto'
import { is } from '@electron-toolkit/utils'
import type {
  AgentAdapter,
  NormalizedEventName,
  NormalizedHookEvent,
  NormalizedStatusData,
  PreferencesReader,
  SettingsSetup,
} from '../types'
import type { SessionStatusType } from '../../notch/types'
import { BLOCKED_ENV_VARS } from '../../security/envBlocklist'
import { asNumber, asRecord, asString, deepMerge, summarizeToolInput } from '../utils'

/** Model ID -> context window size in tokens (lazy-loaded) */
let geminiModelLimits: Record<string, number> | null = null

function getModelLimits(): Record<string, number> {
  if (geminiModelLimits !== null) return geminiModelLimits
  try {
    const limitsPath = is.dev
      ? join(process.cwd(), 'resources', 'gemini-models.json')
      : join(process.resourcesPath, 'app.asar.unpacked', 'resources', 'gemini-models.json')
    geminiModelLimits = JSON.parse(readFileSync(limitsPath, 'utf-8'))
  } catch {
    geminiModelLimits = {}
  }
  return geminiModelLimits!
}

function lookupContextLimit(model: string): number | undefined {
  const limits = getModelLimits()
  // Direct match
  if (limits[model]) return limits[model]
  // Try without version suffix (e.g. "gemini-2.5-flash" matches "gemini-2.5-flash-preview-04-17")
  for (const [id, limit] of Object.entries(limits)) {
    if (id.startsWith(model) || model.startsWith(id)) return limit
  }
  return undefined
}

const GEMINI_HOOK_EVENTS = [
  'SessionStart',
  'SessionEnd',
  'BeforeAgent',
  'AfterAgent',
  'BeforeModel',
  'AfterModel',
  'BeforeToolSelection',
  'BeforeTool',
  'AfterTool',
  'PreCompress',
  'Notification',
]

const EVENT_MAP: Record<string, NormalizedEventName> = {
  SessionStart: 'SessionStart',
  SessionEnd: 'SessionEnd',
  BeforeAgent: 'PromptSubmit',
  AfterAgent: 'Idle',
  BeforeModel: 'Unknown',
  AfterModel: 'Unknown',
  BeforeToolSelection: 'Unknown',
  BeforeTool: 'BeforeToolUse',
  AfterTool: 'AfterToolUse',
  PreCompress: 'Unknown',
  Notification: 'Notification',
}

const INTERNAL_BLOCKED = new Set([
  'CANOPY_HOOK_PORT',
  'CANOPY_HOOK_PATH',
  'CANOPY_HOOK_TOKEN',
  'ELECTRON_RUN_AS_NODE',
])

export const geminiAdapter: AgentAdapter = {
  agentType: 'gemini',
  toolId: 'gemini',

  busyEvents: new Set(['BeforeAgent', 'BeforeTool']),
  idleEvents: new Set(['SessionEnd', 'AfterAgent']),

  setupSettings(
    settingsPath: string,
    _worktreePath: string,
    hookScriptPath: string,
    _statusLineScriptPath: string | null,
    overrides?: Record<string, unknown>,
  ): SettingsSetup {
    // Create an isolated home dir so concurrent sessions don't collide
    const homeDir = join(dirname(settingsPath), `gemini-home-${randomUUID()}`)
    const geminiDir = join(homeDir, '.gemini')
    mkdirSync(geminiDir, { recursive: true })

    // Symlink user config files (except settings.json which we write ourselves)
    const userGeminiDir = join(os.homedir(), '.gemini')
    if (existsSync(userGeminiDir)) {
      for (const entry of readdirSync(userGeminiDir)) {
        if (entry === 'settings.json') continue
        try {
          symlinkSync(join(userGeminiDir, entry), join(geminiDir, entry))
        } catch {
          /* entry may already exist or be inaccessible */
        }
      }
    }

    // Build hooks config
    const hooks: Record<
      string,
      Array<{ matcher: string; hooks: Array<{ type: string; command: string }> }>
    > = {}
    for (const event of GEMINI_HOOK_EVENTS) {
      hooks[event] = [{ matcher: '', hooks: [{ type: 'command', command: hookScriptPath }] }]
    }

    // Read user settings and deep-merge our hooks in
    let userSettings: Record<string, unknown> = {}
    const userSettingsPath = join(userGeminiDir, 'settings.json')
    if (existsSync(userSettingsPath)) {
      try {
        userSettings = JSON.parse(readFileSync(userSettingsPath, 'utf-8'))
      } catch {
        /* corrupt file, start fresh */
      }
    }

    const merged = deepMerge(userSettings, { ...(overrides ?? {}), hooks })
    writeFileSync(join(geminiDir, 'settings.json'), JSON.stringify(merged, null, 2), 'utf-8')

    return {
      args: [],
      env: { GEMINI_CLI_HOME: homeDir },
      cleanup: () => {
        try {
          rmSync(homeDir, { recursive: true, force: true })
        } catch {
          /* best-effort */
        }
      },
    }
  },

  normalizeEvent(raw: Record<string, unknown>): NormalizedHookEvent {
    const rawName = asString(raw.hook_event_name) ?? ''

    let event = EVENT_MAP[rawName] ?? 'Unknown'
    let toolName = asString(raw.tool_name)
    let model: string | undefined

    // Gemini Notification with ToolPermission = permission request
    // details: { type: "ask_user", title: "Ask User" }
    if (rawName === 'Notification' && raw.notification_type === 'ToolPermission') {
      event = 'PermissionRequest'
      const details = asRecord(raw.details)
      if (details) {
        toolName = asString(details.type) ?? asString(details.title) ?? toolName
      }
    }

    // Extract model name from BeforeModel/BeforeToolSelection llm_request.
    // Empty string stays `undefined`: `lookupContextLimit('')` would match the first
    // entry via `id.startsWith('')` and report a bogus context limit.
    if (rawName === 'BeforeModel' || rawName === 'BeforeToolSelection') {
      model = asString(asRecord(raw.llm_request)?.model) || undefined
    }

    // Extract token usage from AfterModel llm_response.usageMetadata
    let extra: Record<string, unknown> | undefined
    if (rawName === 'AfterModel') {
      const usage = asRecord(asRecord(raw.llm_response)?.usageMetadata)
      const totalTokens = asNumber(usage?.totalTokenCount)
      if (totalTokens !== undefined) {
        extra = { totalTokenCount: totalTokens }
        // Also extract model from the request if available
        const reqModel = asString(asRecord(raw.llm_request)?.model)
        if (reqModel) {
          model = reqModel
          const contextLimit = lookupContextLimit(reqModel)
          if (contextLimit) {
            extra.contextPercent = (totalTokens / contextLimit) * 100
            extra.contextSize = contextLimit
          }
        }
      }
    }

    return {
      agentType: 'gemini',
      sessionId: asString(raw.session_id) ?? '',
      event,
      rawEventName: rawName,
      toolName,
      toolInput: asRecord(raw.tool_input),
      toolResponse: asString(raw.tool_response),
      error: asString(raw.error),
      errorDetails: asString(raw.error_details),
      message: asString(raw.message),
      title: asString(raw.title),
      notificationType: asString(raw.notification_type),
      agentId: asString(raw.agent_id),
      agentSubtype: asString(raw.agent_type),
      reason: asString(raw.reason),
      model,
      permissionMode: asString(raw.permission_mode),
      compactSummary: asString(raw.compact_summary),
      prompt: asString(raw.prompt),
      taskId: asString(raw.task_id),
      taskSubject: asString(raw.task_subject),
      taskDescription: asString(raw.task_description),
      teammateName: asString(raw.teammate_name),
      teamName: asString(raw.team_name),
      extra,
    }
  },

  // eslint-disable-next-line @typescript-eslint/no-unused-vars
  normalizeStatus(_raw: Record<string, unknown>): NormalizedStatusData {
    // Gemini CLI does not expose a status line
    return {}
  },

  buildCliArgs(prefs: PreferencesReader): string[] {
    const args: string[] = []
    const model = prefs.get('gemini.model')
    const approvalMode = prefs.get('gemini.approvalMode')

    if (model) args.push('--model', model)
    if (approvalMode) args.push('--approval-mode', approvalMode)

    return args
  },

  buildEnvVars(prefs: PreferencesReader): Record<string, string> {
    const env: Record<string, string> = {}

    const apiKey = prefs.get('gemini.apiKey')
    const customEnv = prefs.get('gemini.customEnv')

    if (apiKey) env.GEMINI_API_KEY = apiKey

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
    return ['--resume', resumeSessionId]
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
    let body = 'A tool requires confirmation'
    if (event.message) {
      body = event.message
    } else if (event.toolName) {
      body = `${event.toolName}: ${summarizeToolInput(event.toolInput)}`
    }
    return { title: 'Gemini CLI — Permission Required', body }
  },

  toNotchStatus(event: NormalizedHookEvent): { status: SessionStatusType; detail?: string } | null {
    const toolDetail = event.toolName
      ? `${event.toolName}: ${summarizeToolInput(event.toolInput)}`
      : undefined

    return match(event.event)
      .with(P.union('SessionStart', 'Idle'), () => ({ status: 'idle' as const }))
      .with(P.union('PromptSubmit', 'AfterToolUse'), () => ({ status: 'thinking' as const }))
      .with('BeforeToolUse', () => ({ status: 'toolCalling' as const, detail: toolDetail }))
      .with('PermissionRequest', () => ({
        status: 'waitingPermission' as const,
        detail: toolDetail,
      }))
      .with('SessionEnd', () => ({ status: 'ended' as const, detail: event.reason }))
      .otherwise(() => null)
  },
}

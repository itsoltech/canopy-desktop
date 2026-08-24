import { match, P } from 'ts-pattern'
import { writeFileSync, unlinkSync } from 'fs'
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
import { asNumber, asRecord, asString, summarizeToolInput } from '../utils'

const CLAUDE_HOOK_EVENTS = [
  'SessionStart',
  'UserPromptSubmit',
  'PreToolUse',
  'PostToolUse',
  'PostToolUseFailure',
  'PermissionRequest',
  'Stop',
  'StopFailure',
  'SubagentStart',
  'SubagentStop',
  'Notification',
  'TaskCompleted',
  'TeammateIdle',
  'PreCompact',
  'PostCompact',
  'SessionEnd',
]

const EVENT_MAP: Record<string, NormalizedEventName> = {
  SessionStart: 'SessionStart',
  UserPromptSubmit: 'PromptSubmit',
  PreToolUse: 'BeforeToolUse',
  PostToolUse: 'AfterToolUse',
  PostToolUseFailure: 'AfterToolUseFailure',
  PermissionRequest: 'PermissionRequest',
  Stop: 'Idle',
  StopFailure: 'IdleFailure',
  PreCompact: 'BeforeCompact',
  PostCompact: 'AfterCompact',
  SubagentStart: 'SubagentStart',
  SubagentStop: 'SubagentStop',
  Notification: 'Notification',
  TaskCompleted: 'TaskCompleted',
  TeammateIdle: 'TeammateIdle',
  SessionEnd: 'SessionEnd',
}

const INTERNAL_BLOCKED = new Set([
  'CANOPY_HOOK_PORT',
  'CANOPY_HOOK_PATH',
  'CANOPY_HOOK_TOKEN',
  'ELECTRON_RUN_AS_NODE',
])

export const claudeAdapter: AgentAdapter = {
  agentType: 'claude',
  toolId: 'claude',

  busyEvents: new Set(['UserPromptSubmit', 'PreToolUse', 'PreCompact', 'PermissionRequest']),
  idleEvents: new Set(['Stop', 'StopFailure', 'SessionEnd']),

  setupSettings(
    settingsPath: string,
    _worktreePath: string,
    hookScriptPath: string,
    statusLineScriptPath: string | null,
    overrides?: Record<string, unknown>,
  ): SettingsSetup {
    const hooks: Record<
      string,
      Array<{ matcher: string; hooks: Array<{ type: string; command: string }> }>
    > = {}
    for (const event of CLAUDE_HOOK_EVENTS) {
      hooks[event] = [{ matcher: '', hooks: [{ type: 'command', command: hookScriptPath }] }]
    }

    const settings: Record<string, unknown> = {
      ...(overrides ?? {}),
      hooks,
    }

    if (statusLineScriptPath) {
      settings.statusLine = { type: 'command', command: statusLineScriptPath }
    }

    writeFileSync(settingsPath, JSON.stringify(settings, null, 2), 'utf-8')

    return {
      args: ['--settings', settingsPath],
      cleanup: () => {
        try {
          unlinkSync(settingsPath)
        } catch {
          // File may already be deleted
        }
      },
    }
  },

  normalizeEvent(raw: Record<string, unknown>): NormalizedHookEvent {
    const rawName = asString(raw.hook_event_name) ?? ''
    return {
      agentType: 'claude',
      sessionId: asString(raw.session_id) ?? '',
      event: EVENT_MAP[rawName] ?? 'Unknown',
      rawEventName: rawName,
      toolName: asString(raw.tool_name),
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
      model: asString(raw.model),
      permissionMode: asString(raw.permission_mode),
      compactSummary: asString(raw.compact_summary),
      prompt: asString(raw.prompt),
      taskId: asString(raw.task_id),
      taskSubject: asString(raw.task_subject),
      taskDescription: asString(raw.task_description),
      teammateName: asString(raw.teammate_name),
      teamName: asString(raw.team_name),
    }
  },

  normalizeStatus(raw: Record<string, unknown>): NormalizedStatusData {
    const model = asRecord(raw.model)
    const ctx = asRecord(raw.context_window)
    const cost = asRecord(raw.cost)
    const rateLimits = asRecord(raw.rate_limits)

    const result: NormalizedStatusData = {
      version: asString(raw.version),
    }

    if (model) {
      result.model = {
        id: asString(model.id),
        displayName: asString(model.display_name),
      }
    }

    if (ctx) {
      result.contextWindow = {
        usedPercent: asNumber(ctx.used_percentage),
        size: asNumber(ctx.context_window_size),
      }
    }

    if (cost) {
      result.cost = {
        totalCostUsd: asNumber(cost.total_cost_usd),
        durationMs: asNumber(cost.total_duration_ms),
        linesAdded: asNumber(cost.total_lines_added),
        linesRemoved: asNumber(cost.total_lines_removed),
      }
    }

    if (rateLimits) {
      result.extra = { rateLimits }
    }

    return result
  },

  buildCliArgs(prefs: PreferencesReader): string[] {
    const args: string[] = []
    const model = prefs.get('claude.model')
    const permMode = prefs.get('claude.permissionMode')
    const effort = prefs.get('claude.effortLevel')
    const appendPrompt = prefs.get('claude.appendSystemPrompt')

    if (model) args.push('--model', model)
    if (permMode) args.push('--permission-mode', permMode)
    if (effort) args.push('--effort', effort)
    if (appendPrompt) args.push('--append-system-prompt', appendPrompt)

    return args
  },

  buildEnvVars(prefs: PreferencesReader): Record<string, string> {
    const env: Record<string, string> = {}

    const apiKey = prefs.get('claude.apiKey')
    const baseUrl = prefs.get('claude.baseUrl')
    const provider = prefs.get('claude.provider')
    const customEnv = prefs.get('claude.customEnv')

    if (apiKey) env.ANTHROPIC_API_KEY = apiKey
    if (baseUrl) env.ANTHROPIC_BASE_URL = baseUrl
    if (provider === 'bedrock') env.CLAUDE_CODE_USE_BEDROCK = '1'
    if (provider === 'vertex') env.CLAUDE_CODE_USE_VERTEX = '1'
    if (provider === 'foundry') env.CLAUDE_CODE_USE_FOUNDRY = '1'

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
    const body = event.toolName
      ? `${event.toolName}: ${summarizeToolInput(event.toolInput)}`
      : 'A tool requires your approval'
    return { title: 'Claude Code — Permission Required', body }
  },

  toNotchStatus(event: NormalizedHookEvent): { status: SessionStatusType; detail?: string } | null {
    const toolDetail = event.toolName
      ? `${event.toolName}: ${summarizeToolInput(event.toolInput)}`
      : undefined

    return match(event.event)
      .with(P.union('SessionStart', 'Idle'), () => ({ status: 'idle' as const }))
      .with(P.union('AfterToolUse', 'AfterToolUseFailure', 'PromptSubmit', 'AfterCompact'), () => ({
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

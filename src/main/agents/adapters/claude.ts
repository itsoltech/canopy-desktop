import { writeFileSync, unlinkSync } from 'fs'
import type {
  AgentAdapter,
  AgentType,
  NormalizedEventName,
  NormalizedHookEvent,
  NormalizedStatusData,
  PreferencesReader,
  SettingsSetup,
} from '../types'
import type { SessionStatusType } from '../../notch/types'
import { BLOCKED_ENV_VARS } from '../../security/envBlocklist'

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

const INTERNAL_BLOCKED = new Set(['CANOPY_HOOK_PORT', 'CANOPY_HOOK_TOKEN', 'ELECTRON_RUN_AS_NODE'])

function truncate(text: string, max: number): string {
  return text.length > max ? text.slice(0, max - 3) + '...' : text
}

export const claudeAdapter: AgentAdapter = {
  agentType: 'claude' as AgentType,
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
    const rawName = (raw.hook_event_name as string) ?? ''
    return {
      agentType: 'claude',
      sessionId: (raw.session_id as string) ?? '',
      event: EVENT_MAP[rawName] ?? 'Unknown',
      rawEventName: rawName,
      toolName: raw.tool_name as string | undefined,
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
      teamName: raw.team_name as string | undefined,
    }
  },

  normalizeStatus(raw: Record<string, unknown>): NormalizedStatusData {
    const model = raw.model as Record<string, unknown> | undefined
    const ctx = raw.context_window as Record<string, unknown> | undefined
    const cost = raw.cost as Record<string, unknown> | undefined
    const rateLimits = raw.rate_limits as Record<string, unknown> | undefined

    const result: NormalizedStatusData = {
      version: raw.version as string | undefined,
    }

    if (model) {
      result.model = {
        id: model.id as string | undefined,
        displayName: model.display_name as string | undefined,
      }
    }

    if (ctx) {
      result.contextWindow = {
        usedPercent: ctx.used_percentage as number | undefined,
        size: ctx.context_window_size as number | undefined,
      }
    }

    if (cost) {
      result.cost = {
        totalCostUsd: cost.total_cost_usd as number | undefined,
        durationMs: cost.total_duration_ms as number | undefined,
        linesAdded: cost.total_lines_added as number | undefined,
        linesRemoved: cost.total_lines_removed as number | undefined,
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
            !INTERNAL_BLOCKED.has(k)
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
      ? `${event.toolName}: ${this.summarizeToolInput(event.toolInput)}`
      : 'A tool requires your approval'
    return { title: 'Claude Code — Permission Required', body }
  },

  toNotchStatus(event: NormalizedHookEvent): { status: SessionStatusType; detail?: string } | null {
    switch (event.event) {
      case 'SessionStart':
      case 'Idle':
        return { status: 'idle' }

      case 'AfterToolUse':
      case 'AfterToolUseFailure':
      case 'PromptSubmit':
      case 'AfterCompact':
        return { status: 'thinking' }

      case 'BeforeToolUse': {
        const detail = event.toolName
          ? `${event.toolName}: ${this.summarizeToolInput(event.toolInput)}`
          : undefined
        return { status: 'toolCalling', detail }
      }

      case 'PermissionRequest': {
        const detail = event.toolName
          ? `${event.toolName}: ${this.summarizeToolInput(event.toolInput)}`
          : undefined
        return { status: 'waitingPermission', detail }
      }

      case 'BeforeCompact':
        return { status: 'compacting' }

      case 'IdleFailure':
        return { status: 'error', detail: event.error }

      case 'SessionEnd':
        return { status: 'ended', detail: event.reason }

      default:
        return null
    }
  },

  summarizeToolInput(input?: Record<string, unknown>): string {
    if (!input) return ''

    if (typeof input.command === 'string') {
      return truncate(input.command, 80)
    }
    if (typeof input.file_path === 'string') {
      return input.file_path as string
    }
    if (Array.isArray(input.questions) && input.questions.length > 0) {
      const first = input.questions[0] as Record<string, unknown> | undefined
      if (first && typeof first.question === 'string') {
        return truncate(first.question as string, 80)
      }
    }
    if (typeof input.query === 'string') {
      return truncate(input.query, 80)
    }
    if (typeof input.url === 'string') {
      return truncate(input.url, 80)
    }
    if (typeof input.pattern === 'string') {
      let summary = input.pattern as string
      if (typeof input.path === 'string') {
        summary += ` in ${input.path}`
      }
      return truncate(summary, 80)
    }
    if (typeof input.prompt === 'string') {
      return truncate(input.prompt, 80)
    }
    if (typeof input.description === 'string') {
      return truncate(input.description, 80)
    }
    if (typeof input.skill === 'string') {
      return input.skill as string
    }

    for (const val of Object.values(input)) {
      if (typeof val === 'string' && val.length > 0) {
        return truncate(val, 80)
      }
    }

    return ''
  },
}

import { match, P } from 'ts-pattern'
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
import { summarizeToolInput } from '../utils'
import { buildClaudeProviderEnv } from '../claudeProviderEnv'

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
    return buildClaudeProviderEnv(prefs.get('claude.apiKey'), {
      model: prefs.get('claude.model') ?? undefined,
      baseUrl: prefs.get('claude.baseUrl') ?? undefined,
      provider: prefs.get('claude.provider') ?? undefined,
      claudeProviderPreset: prefs.get('claude.claudeProviderPreset') ?? undefined,
      providerModel: prefs.get('claude.providerModel') ?? undefined,
      providerOpusModel: prefs.get('claude.providerOpusModel') ?? undefined,
      providerSonnetModel: prefs.get('claude.providerSonnetModel') ?? undefined,
      providerHaikuModel: prefs.get('claude.providerHaikuModel') ?? undefined,
      customEnv: prefs.get('claude.customEnv') ?? undefined,
    })
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

export interface SubagentRecord {
  agentId: string
  agentType: string
}

export interface TaskRecord {
  id: string
  subject: string
  description: string
  completedAt: number
}

export interface NotificationRecord {
  title: string
  message: string
  type: string
  timestamp: number
}

export type ClaudeStatus =
  | { type: 'inactive' }
  | { type: 'starting' }
  | { type: 'idle' }
  | { type: 'toolCalling'; toolName: string }
  | { type: 'waitingPermission'; toolName: string }
  | { type: 'error'; errorType: string; details: string }
  | { type: 'ended'; reason: string }

export interface ClaudeSessionState {
  status: ClaudeStatus
  claudeSessionId: string | null
  startTime: Date | null
  model: string | null
  modelId: string | null
  activeSubagents: SubagentRecord[]
  permissionMode: string | null
  tasks: TaskRecord[]
  notifications: NotificationRecord[]
  compactCount: number
  toolCallCount: number
  // Status line data
  contextPercent: number | null
  contextSize: number | null
  costUsd: number | null
  durationMs: number | null
  linesAdded: number | null
  linesRemoved: number | null
  rateLimitFiveHour: number | null
  rateLimitSevenDay: number | null
  version: string | null
}

export type BadgeType = 'none' | 'unread' | 'permission'

const MAX_NOTIFICATIONS = 20

export const claudeSessions: Record<string, ClaudeSessionState> = $state({})
export const claudeBadges: Record<string, BadgeType> = $state({})

export function initClaudeSession(ptySessionId: string): void {
  claudeSessions[ptySessionId] = {
    status: { type: 'inactive' },
    claudeSessionId: null,
    startTime: null,
    model: null,
    modelId: null,
    activeSubagents: [],
    permissionMode: null,
    tasks: [],
    notifications: [],
    compactCount: 0,
    toolCallCount: 0,
    contextPercent: null,
    contextSize: null,
    costUsd: null,
    durationMs: null,
    linesAdded: null,
    linesRemoved: null,
    rateLimitFiveHour: null,
    rateLimitSevenDay: null,
    version: null,
  }
  claudeBadges[ptySessionId] = 'none'
}

export function removeClaudeSession(ptySessionId: string): void {
  delete claudeSessions[ptySessionId]
  delete claudeBadges[ptySessionId]
}

interface HookEventData {
  session_id: string
  hook_event_name: string
  tool_name?: string
  tool_input?: Record<string, unknown>
  tool_response?: string
  error?: string
  error_details?: string
  agent_id?: string
  agent_type?: string
  reason?: string
  permission_mode?: string
  model?: string
  task_id?: string
  task_subject?: string
  task_description?: string
  teammate_name?: string
  team_name?: string
  compact_summary?: string
  message?: string
  title?: string
  notification_type?: string
  [key: string]: unknown
}

export function handleHookEvent(ptySessionId: string, event: HookEventData): void {
  const session = claudeSessions[ptySessionId]
  if (!session) return

  switch (event.hook_event_name) {
    case 'SessionStart':
      session.status = { type: 'starting' }
      session.claudeSessionId = event.session_id
      session.startTime = new Date()
      session.permissionMode = event.permission_mode ?? null
      session.model = event.model ?? null
      break

    case 'PreToolUse':
      session.status = {
        type: 'toolCalling',
        toolName: event.tool_name ?? 'unknown',
      }
      break

    case 'PermissionRequest':
      session.status = {
        type: 'waitingPermission',
        toolName: event.tool_name ?? 'unknown',
      }
      break

    case 'PostToolUse':
      session.toolCallCount++
      break

    case 'PostToolUseFailure':
      session.toolCallCount++
      break

    case 'Stop':
      session.status = { type: 'idle' }
      break

    case 'StopFailure':
      session.status = {
        type: 'error',
        errorType: event.error ?? 'unknown',
        details: event.error_details ?? '',
      }
      break

    case 'SubagentStart':
      session.activeSubagents = [
        ...session.activeSubagents,
        { agentId: event.agent_id ?? '', agentType: event.agent_type ?? '' },
      ]
      break

    case 'SubagentStop':
      session.activeSubagents = session.activeSubagents.filter((a) => a.agentId !== event.agent_id)
      break

    case 'TaskCompleted':
      session.tasks = [
        ...session.tasks,
        {
          id: event.task_id ?? '',
          subject: event.task_subject ?? '',
          description: event.task_description ?? '',
          completedAt: Date.now(),
        },
      ]
      break

    case 'Notification':
      session.notifications = [
        ...session.notifications.slice(-(MAX_NOTIFICATIONS - 1)),
        {
          title: event.title ?? '',
          message: event.message ?? '',
          type: event.notification_type ?? '',
          timestamp: Date.now(),
        },
      ]
      break

    case 'PostCompact':
      session.compactCount++
      break

    case 'SessionEnd':
      session.status = { type: 'ended', reason: event.reason ?? 'unknown' }
      break
  }
}

export function setBadge(ptySessionId: string, badge: BadgeType): void {
  claudeBadges[ptySessionId] = badge
}

export function clearBadge(ptySessionId: string): void {
  claudeBadges[ptySessionId] = 'none'
}

export function getClaudeBadge(ptySessionId: string): BadgeType {
  return claudeBadges[ptySessionId] ?? 'none'
}

interface StatusLineData {
  model?: { id?: string; display_name?: string }
  context_window?: {
    used_percentage?: number | null
    context_window_size?: number
  }
  cost?: {
    total_cost_usd?: number
    total_duration_ms?: number
    total_lines_added?: number
    total_lines_removed?: number
  }
  rate_limits?: {
    five_hour?: { used_percentage?: number }
    seven_day?: { used_percentage?: number }
  }
  version?: string
  [key: string]: unknown
}

export function handleStatusUpdate(ptySessionId: string, data: StatusLineData): void {
  const session = claudeSessions[ptySessionId]
  if (!session) return

  if (data.model) {
    session.model = data.model.display_name ?? session.model
    session.modelId = data.model.id ?? session.modelId
  }

  if (data.context_window) {
    session.contextPercent = data.context_window.used_percentage ?? session.contextPercent
    session.contextSize = data.context_window.context_window_size ?? session.contextSize
  }

  if (data.cost) {
    session.costUsd = data.cost.total_cost_usd ?? session.costUsd
    session.durationMs = data.cost.total_duration_ms ?? session.durationMs
    session.linesAdded = data.cost.total_lines_added ?? session.linesAdded
    session.linesRemoved = data.cost.total_lines_removed ?? session.linesRemoved
  }

  if (data.rate_limits) {
    session.rateLimitFiveHour =
      data.rate_limits.five_hour?.used_percentage ?? session.rateLimitFiveHour
    session.rateLimitSevenDay =
      data.rate_limits.seven_day?.used_percentage ?? session.rateLimitSevenDay
  }

  if (data.version) {
    session.version = data.version
  }
}

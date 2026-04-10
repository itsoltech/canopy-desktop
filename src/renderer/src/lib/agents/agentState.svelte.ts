import { match } from 'ts-pattern'
import { SvelteDate } from 'svelte/reactivity'

export type AgentType = 'claude' | 'gemini' | 'codex'

export interface SubagentRecord {
  agentId: string
  agentType: string
}

export interface TaskRecord {
  id: string
  subject: string
  status: 'pending' | 'in_progress' | 'completed' | 'deleted'
  activeForm: string | null
  owner: string | null
}

export interface NotificationRecord {
  title: string
  message: string
  type: string
  timestamp: number
}

export type AgentStatus =
  | { type: 'inactive' }
  | { type: 'starting' }
  | { type: 'idle' }
  | { type: 'thinking' }
  | { type: 'compacting' }
  | { type: 'toolCalling'; toolName: string }
  | { type: 'waitingPermission'; toolName: string }
  | { type: 'error'; errorType: string; details: string }
  | { type: 'ended'; reason: string }

export interface AgentSessionState {
  agentType: AgentType
  status: AgentStatus
  agentSessionId: string | null
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
  version: string | null
  // Agent-specific extension data (e.g. Claude rate limits)
  extra: Record<string, unknown>
}

export type BadgeType = 'none' | 'unread' | 'permission'

const MAX_NOTIFICATIONS = 20
const MAX_TASKS = 50

export const agentSessions: Record<string, AgentSessionState> = $state({})
export const agentBadges: Record<string, BadgeType> = $state({})
export const worktreeBadges: Record<string, BadgeType> = $state({})

export function initAgentSession(ptySessionId: string, agentType: AgentType): void {
  agentSessions[ptySessionId] = {
    agentType,
    status: { type: 'inactive' },
    agentSessionId: null,
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
    version: null,
    extra: {},
  }
  agentBadges[ptySessionId] = 'none'
}

export function removeAgentSession(ptySessionId: string): void {
  delete agentSessions[ptySessionId]
  delete agentBadges[ptySessionId]
}

/** Normalized hook event from the main process */
interface NormalizedHookEvent {
  agentType: string
  sessionId: string
  event: string
  rawEventName: string
  toolName?: string
  toolInput?: Record<string, unknown>
  toolResponse?: unknown
  error?: string
  errorDetails?: string
  agentId?: string
  agentSubtype?: string
  reason?: string
  permissionMode?: string
  model?: string
  taskId?: string
  taskSubject?: string
  taskDescription?: string
  teammateName?: string
  teamName?: string
  compactSummary?: string
  message?: string
  title?: string
  notificationType?: string
  [key: string]: unknown
}

export function handleHookEvent(ptySessionId: string, event: NormalizedHookEvent): void {
  const session = agentSessions[ptySessionId]
  if (!session) return

  if (event.sessionId) session.agentSessionId = event.sessionId
  // Update model from any event that carries it (Gemini sends it via BeforeModel)
  if (event.model) session.model = event.model

  // Update context window from extra (Gemini extracts from AfterModel.usageMetadata)
  const extra = event.extra as Record<string, unknown> | undefined
  if (extra) {
    if (typeof extra.contextPercent === 'number') session.contextPercent = extra.contextPercent
    if (typeof extra.contextSize === 'number') session.contextSize = extra.contextSize
    // Merge agent-specific extras (Codex: cwd, transcriptPath, turnId, etc.)
    for (const [k, v] of Object.entries(extra)) {
      if (k !== 'contextPercent' && k !== 'contextSize') {
        session.extra[k] = v
      }
    }
  }

  match(event.event)
    .with('SessionStart', () => {
      session.status = { type: 'idle' }
      session.startTime = new SvelteDate()
      session.permissionMode = event.permissionMode ?? null
      session.model = event.model ?? null
      session.tasks = []
      session.notifications = []
      session.activeSubagents = []
      session.compactCount = 0
      session.toolCallCount = 0
      session.contextPercent = null
      session.contextSize = null
      session.costUsd = null
      session.durationMs = null
      session.linesAdded = null
      session.linesRemoved = null
    })
    .with('PromptSubmit', () => {
      session.status = { type: 'thinking' }
      // Track turn count (Codex provides turn_id per turn)
      if (extra?.turnId) {
        const count = (session.extra.turnCount as number | undefined) ?? 0
        session.extra.turnCount = count + 1
      }
    })
    .with('BeforeToolUse', () => {
      session.status = {
        type: 'toolCalling',
        toolName: event.toolName ?? 'unknown',
      }
    })
    .with('PermissionRequest', () => {
      session.status = {
        type: 'waitingPermission',
        toolName: event.toolName ?? 'unknown',
      }
    })
    .with('AfterToolUse', () => {
      session.toolCallCount++
      handleTaskToolUse(session, event)
    })
    .with('AfterToolUseFailure', () => {
      session.toolCallCount++
    })
    .with('Idle', () => {
      session.status = { type: 'idle' }
    })
    .with('IdleFailure', () => {
      session.status = {
        type: 'error',
        errorType: event.error ?? 'unknown',
        details: event.errorDetails ?? '',
      }
    })
    .with('SubagentStart', () => {
      session.activeSubagents = [
        ...session.activeSubagents,
        { agentId: event.agentId ?? '', agentType: event.agentSubtype ?? '' },
      ]
    })
    .with('SubagentStop', () => {
      session.activeSubagents = session.activeSubagents.filter((a) => a.agentId !== event.agentId)
    })
    .with('TaskCompleted', () => {
      const taskId = event.taskId ?? ''
      const existing = session.tasks.find((t) => t.id === taskId)
      if (existing) {
        existing.status = 'completed'
        if (event.taskSubject) existing.subject = event.taskSubject
      } else {
        session.tasks = [
          ...session.tasks,
          {
            id: taskId,
            subject: event.taskSubject ?? '',
            status: 'completed',
            activeForm: null,
            owner: null,
          },
        ]
      }
    })
    .with('Notification', () => {
      session.notifications = [
        ...session.notifications.slice(-(MAX_NOTIFICATIONS - 1)),
        {
          title: event.title ?? '',
          message: event.message ?? '',
          type: event.notificationType ?? '',
          timestamp: Date.now(),
        },
      ]
    })
    .with('BeforeCompact', () => {
      session.status = { type: 'compacting' }
    })
    .with('AfterCompact', () => {
      session.compactCount++
      session.status = { type: 'thinking' }
    })
    .with('SessionEnd', () => {
      session.status = { type: 'ended', reason: event.reason ?? 'unknown' }
    })
    .otherwise(() => {})
}

function handleTaskToolUse(session: AgentSessionState, event: NormalizedHookEvent): void {
  const toolName = event.toolName
  if (!toolName) return

  const input = event.toolInput as Record<string, unknown> | undefined
  if (!input) return

  match(toolName)
    .with('TaskCreate', () => {
      const resp = event.toolResponse as { task?: { id?: string } } | undefined
      const id = resp?.task?.id ?? `t-${Date.now()}`
      let tasks = [
        ...session.tasks,
        {
          id,
          subject: (input.subject as string) ?? '',
          status: 'pending' as const,
          activeForm: (input.activeForm as string) ?? null,
          owner: null,
        },
      ]
      if (tasks.length > MAX_TASKS) {
        // Drop oldest completed tasks first to stay within limit
        const excess = tasks.length - MAX_TASKS
        let dropped = 0
        tasks = tasks.filter((t) => {
          if (dropped < excess && t.status === 'completed') {
            dropped++
            return false
          }
          return true
        })
        // If still over limit, drop from the front
        if (tasks.length > MAX_TASKS) {
          tasks = tasks.slice(-MAX_TASKS)
        }
      }
      session.tasks = tasks
    })
    .with('TaskUpdate', () => {
      const taskId = String(input.taskId ?? '')
      const existing = session.tasks.find((t) => t.id === taskId)
      if (existing) {
        if (input.status) existing.status = input.status as TaskRecord['status']
        if (input.subject) existing.subject = input.subject as string
        if (input.owner !== undefined) existing.owner = (input.owner as string) ?? null
        if (input.activeForm !== undefined)
          existing.activeForm = (input.activeForm as string) ?? null
        session.tasks = [...session.tasks]
      }
    })
    .otherwise(() => {})
}

/** Normalized status data from the main process */
interface NormalizedStatusData {
  model?: { id?: string; displayName?: string }
  contextWindow?: { usedPercent?: number; size?: number }
  cost?: {
    totalCostUsd?: number
    durationMs?: number
    linesAdded?: number
    linesRemoved?: number
  }
  version?: string
  extra?: Record<string, unknown>
}

export function handleStatusUpdate(ptySessionId: string, data: NormalizedStatusData): void {
  const session = agentSessions[ptySessionId]
  if (!session) return

  if (data.model) {
    session.model = data.model.displayName ?? session.model
    session.modelId = data.model.id ?? session.modelId
  }

  if (data.contextWindow) {
    session.contextPercent = data.contextWindow.usedPercent ?? session.contextPercent
    session.contextSize = data.contextWindow.size ?? session.contextSize
  }

  if (data.cost) {
    session.costUsd = data.cost.totalCostUsd ?? session.costUsd
    session.durationMs = data.cost.durationMs ?? session.durationMs
    session.linesAdded = data.cost.linesAdded ?? session.linesAdded
    session.linesRemoved = data.cost.linesRemoved ?? session.linesRemoved
  }

  if (data.version) {
    session.version = data.version
  }

  // Merge agent-specific extra data (e.g. Claude rate limits)
  if (data.extra) {
    const rateLimits = data.extra.rateLimits as Record<string, unknown> | undefined
    if (rateLimits) {
      const fiveHour = rateLimits.five_hour as Record<string, unknown> | undefined
      const sevenDay = rateLimits.seven_day as Record<string, unknown> | undefined
      if (fiveHour) {
        session.extra.rateLimitFiveHour = fiveHour.used_percentage as number | undefined
        if (fiveHour.resets_at != null) {
          session.extra.rateLimitFiveHourResetsAt = (fiveHour.resets_at as number) * 1000
        }
      }
      if (sevenDay) {
        session.extra.rateLimitSevenDay = sevenDay.used_percentage as number | undefined
        if (sevenDay.resets_at != null) {
          session.extra.rateLimitSevenDayResetsAt = (sevenDay.resets_at as number) * 1000
        }
      }
    }
  }
}

export function setBadge(ptySessionId: string, badge: BadgeType): void {
  agentBadges[ptySessionId] = badge
}

export function clearBadge(ptySessionId: string): void {
  agentBadges[ptySessionId] = 'none'
}

export function getAgentBadge(ptySessionId: string): BadgeType {
  return agentBadges[ptySessionId] ?? 'none'
}

export function setWorktreeBadge(worktreePath: string, badge: BadgeType): void {
  const current = worktreeBadges[worktreePath]
  if (current === 'permission' && badge === 'unread') return
  worktreeBadges[worktreePath] = badge
}

export function clearWorktreeBadge(worktreePath: string): void {
  worktreeBadges[worktreePath] = 'none'
}

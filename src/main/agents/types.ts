import type { SessionStatusType } from '../notch/types'

export type AgentType = 'claude' | 'gemini' | 'opencode' | 'codex'

export type NormalizedEventName =
  | 'SessionStart'
  | 'SessionEnd'
  | 'PromptSubmit'
  | 'BeforeToolUse'
  | 'AfterToolUse'
  | 'AfterToolUseFailure'
  | 'PermissionRequest'
  | 'Idle'
  | 'IdleFailure'
  | 'BeforeCompact'
  | 'AfterCompact'
  | 'SubagentStart'
  | 'SubagentStop'
  | 'Notification'
  | 'TaskCompleted'
  | 'TeammateIdle'
  | 'Unknown'

export interface NormalizedHookEvent {
  agentType: AgentType
  sessionId: string
  event: NormalizedEventName
  rawEventName: string
  toolName?: string
  toolInput?: Record<string, unknown>
  toolResponse?: string
  error?: string
  errorDetails?: string
  message?: string
  title?: string
  notificationType?: string
  agentId?: string
  agentSubtype?: string
  reason?: string
  model?: string
  permissionMode?: string
  compactSummary?: string
  prompt?: string
  taskId?: string
  taskSubject?: string
  taskDescription?: string
  teammateName?: string
  teamName?: string
  extra?: Record<string, unknown>
}

export interface NormalizedStatusData {
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

export interface SettingsSetup {
  /** Extra CLI args to add (e.g. ['--settings', '/path']) */
  args: string[]
  /** Extra env vars injected by the settings setup (e.g. GEMINI_CLI_HOME) */
  env?: Record<string, string>
  /** Cleanup on session destroy */
  cleanup: () => void
}

export interface PreferencesReader {
  get(key: string): string | null
}

export interface AgentAdapter {
  readonly agentType: AgentType
  readonly toolId: string
  readonly busyEvents: Set<string>
  readonly idleEvents: Set<string>

  setupSettings(
    settingsPath: string,
    worktreePath: string,
    hookScriptPath: string,
    statusLineScriptPath: string | null,
    overrides?: Record<string, unknown>,
  ): SettingsSetup

  normalizeEvent(raw: Record<string, unknown>): NormalizedHookEvent

  normalizeStatus(raw: Record<string, unknown>): NormalizedStatusData

  buildCliArgs(prefs: PreferencesReader): string[]

  buildEnvVars(prefs: PreferencesReader): Record<string, string>

  buildResumeArgs?(resumeSessionId: string): string[]

  buildSessionContext?(worktreePath: string, workspaceName: string, branch: string | null): string

  formatNotification?(event: NormalizedHookEvent): { title: string; body: string } | null

  toNotchStatus(event: NormalizedHookEvent): { status: SessionStatusType; detail?: string } | null
}

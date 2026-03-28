export type SessionStatusType =
  | 'idle'
  | 'thinking'
  | 'toolCalling'
  | 'compacting'
  | 'waitingPermission'
  | 'error'
  | 'ended'

export interface NotchSessionStatus {
  ptySessionId: string
  windowId: number
  workspaceName: string
  branch: string | null
  status: SessionStatusType
  toolName?: string
  detail?: string
  /** First user prompt, used to distinguish sessions on the same worktree */
  title?: string
}

export interface NotchOverlayState {
  sessions: NotchSessionStatus[]
  notchWidth: number
  notchHeight: number
  peekSessionIds?: string[]
}

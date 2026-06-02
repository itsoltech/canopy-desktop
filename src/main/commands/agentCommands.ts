import type { WebContents } from 'electron'
import type { AgentSessionManager } from '../agents/AgentSessionManager'
import type { PtyManager } from '../pty/PtyManager'
import type { WindowManager } from '../WindowManager'
import type { AgentCommandResult } from './types'

interface AgentCommandServiceDeps {
  ptyManager: PtyManager
  agentSessionManager: AgentSessionManager
  windowManager: WindowManager
}

export interface AgentContextPayload {
  text: string
  worktreePath?: string
  sessionId?: string
}

export interface AgentDrawingPayload {
  worktreePath?: string
  sessionId?: string
}

export class AgentCommandService {
  constructor(private deps: AgentCommandServiceDeps) {}

  sendTaskContext(sender: WebContents, payload: AgentContextPayload): AgentCommandResult {
    const sessionId = this.resolveTargetSession(sender, payload.sessionId)
    this.writeBracketedPaste(sessionId, this.contextText(payload))
    return { sessionId }
  }

  sendReviewContext(sender: WebContents, payload: AgentContextPayload): AgentCommandResult {
    const sessionId = this.resolveTargetSession(sender, payload.sessionId)
    this.writeBracketedPaste(sessionId, this.contextText(payload))
    return { sessionId }
  }

  sendDrawing(sender: WebContents, payload: AgentDrawingPayload): AgentCommandResult {
    const sessionId = this.resolveTargetSession(sender, payload.sessionId)
    this.deps.ptyManager.write(sessionId, '\x16')
    return { sessionId }
  }

  private resolveTargetSession(sender: WebContents, explicitSessionId?: string): string {
    if (explicitSessionId) {
      if (!this.isValidTarget(sender.id, explicitSessionId)) {
        throw new Error('Agent session is not available')
      }
      return explicitSessionId
    }

    const focusedSessionId = this.deps.windowManager.getFocusedAgentSession(sender.id)
    if (focusedSessionId && this.isValidTarget(sender.id, focusedSessionId)) {
      return focusedSessionId
    }

    throw new Error('No active agent session')
  }

  private isValidTarget(webContentsId: number, sessionId: string): boolean {
    return (
      this.deps.windowManager.ownsPtySession(webContentsId, sessionId) &&
      this.deps.agentSessionManager.isAgentSession(sessionId)
    )
  }

  private writeBracketedPaste(sessionId: string, text: string): void {
    this.deps.ptyManager.write(sessionId, `\x1b[200~${text}\x1b[201~\r`)
  }

  private contextText(payload: AgentContextPayload): string {
    if (typeof payload.text !== 'string') throw new Error('Invalid agent context payload')
    return payload.text
  }
}

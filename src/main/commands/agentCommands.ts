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

// Context text is third-party content (tracker descriptions/comments, diffs).
// Strip ANSI CSI sequences and control characters (keeping \t and \n) so an
// embedded `\x1b[201~` cannot end the bracketed paste early and turn the rest
// of the text into typed input — same policy as the renderer's sanitizePtyInput.
// eslint-disable-next-line no-control-regex
const ANSI_CSI = /\x1b\[[0-?]*[ -/]*[@-~]/g
// eslint-disable-next-line no-control-regex
const UNSAFE_CONTROL_CHARS = /[\x00-\x08\x0b-\x1f\x7f-\x9f]/g

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
    // No trailing \r after the paste: a bare CR is a real Enter and would SUBMIT whatever draft
    // is already sitting in the agent's input. Sent contexts stack in the input instead — the
    // user confirms explicitly in the agent panel. The newline INSIDE the paste is literal
    // (bracketed), separating this message from the next stacked one.
    this.deps.ptyManager.write(sessionId, `\x1b[200~${text}\n\x1b[201~`)
  }

  private contextText(payload: AgentContextPayload): string {
    if (typeof payload.text !== 'string') throw new Error('Invalid agent context payload')
    return payload.text.replace(ANSI_CSI, '').replace(UNSAFE_CONTROL_CHARS, '')
  }
}

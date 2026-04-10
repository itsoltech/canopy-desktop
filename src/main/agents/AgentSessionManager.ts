import { app, BrowserWindow, Notification } from 'electron'
import { join } from 'path'
import { mkdirSync, readdirSync, unlinkSync, rmSync, existsSync, chmodSync, statSync } from 'fs'
import { randomUUID } from 'crypto'
import { EventEmitter } from 'events'
import { is } from '@electron-toolkit/utils'
import { AgentHookRouter } from './AgentHookServer'
import type { AgentAdapter, AgentType, NormalizedHookEvent, SettingsSetup } from './types'
import type { NotchSessionStatus } from '../notch/types'
import { getAdapter, registerAdapter, isAgentTool as isRegistered } from './registry'
import { claudeAdapter } from './adapters/claude'
import { geminiAdapter } from './adapters/gemini'
import { codexAdapter } from './adapters/codex'

interface AgentSession {
  agentType: AgentType
  adapter: AgentAdapter
  agentSessionId: string
  ptySessionId: string
  settingsSetup: SettingsSetup
  hookPort: number
  hookPath: string
  worktreePath: string
  workspaceName: string
  branch: string | null
  sessionRef: { ptySessionId: string }
  ownerWindow: BrowserWindow
  processTitle?: string
}

export interface AgentSessionManagerEvents {
  statusChange: [status: NotchSessionStatus]
  sessionDestroyed: [ptySessionId: string]
}

export class AgentSessionManager extends EventEmitter {
  private sessions = new Map<string, AgentSession>()
  private busySessions = new Set<string>()
  private hooksDir: string
  private router = new AgentHookRouter()

  constructor() {
    super()
    this.hooksDir = join(app.getPath('userData'), 'canopy', 'agent-hooks')
    mkdirSync(this.hooksDir, { recursive: true })

    // Register built-in adapters
    registerAdapter(claudeAdapter)
    registerAdapter(geminiAdapter)
    registerAdapter(codexAdapter)
  }

  get sessionCount(): number {
    return this.sessions.size
  }

  getSession(ptySessionId: string): AgentSession | undefined {
    return this.sessions.get(ptySessionId)
  }

  updateProcessTitle(ptySessionId: string, title: string): void {
    const session = this.sessions.get(ptySessionId)
    if (session) session.processTitle = title
  }

  async createSession(
    toolId: string,
    worktreePath: string,
    workspaceName: string,
    branch: string | null,
    ownerWindow: BrowserWindow,
    settingsOverrides?: Record<string, unknown>,
  ): Promise<{
    settingsArgs: string[]
    settingsEnv?: Record<string, string>
    hookPort: number
    hookPath: string
    hookAuthToken: string
    tempId: string
  }> {
    const adapter = getAdapter(toolId)
    if (!adapter) throw new Error(`No agent adapter for tool: ${toolId}`)

    const agentSessionId = randomUUID()
    const tempId = `_agent_${agentSessionId}`
    const sessionRef = { ptySessionId: tempId }

    const {
      port: hookPort,
      path: hookPath,
      authToken: hookAuthToken,
    } = await this.router.addSession(
      agentSessionId,
      (rawEvent: Record<string, unknown>): Record<string, unknown> | void => {
        const normalized = adapter.normalizeEvent(rawEvent)

        // Track busy state
        const rawName = (rawEvent.hook_event_name as string) ?? ''
        if (adapter.busyEvents.has(rawName)) {
          this.busySessions.add(sessionRef.ptySessionId)
        } else if (adapter.idleEvents.has(rawName)) {
          this.busySessions.delete(sessionRef.ptySessionId)
        }

        if (ownerWindow.isDestroyed()) return

        ownerWindow.webContents.send('agent:hookEvent', {
          ptySessionId: sessionRef.ptySessionId,
          agentType: adapter.agentType,
          event: normalized,
        })

        // Emit status change for notch overlay
        this.emitStatusChange(
          normalized,
          sessionRef.ptySessionId,
          ownerWindow,
          workspaceName,
          branch,
        )

        // Show notification if adapter requests it
        if (adapter.formatNotification) {
          const notif = adapter.formatNotification(normalized)
          if (notif) {
            this.showNotification(notif.title, notif.body, sessionRef.ptySessionId, ownerWindow)
          }
        }

        // SessionStart response
        if (normalized.event === 'SessionStart' && adapter.buildSessionContext) {
          return {
            hookSpecificOutput: {
              hookEventName: 'SessionStart',
              additionalContext: adapter.buildSessionContext(worktreePath, workspaceName, branch),
            },
          }
        }
      },
      (data: Record<string, unknown>) => {
        if (ownerWindow.isDestroyed()) return

        const normalized = adapter.normalizeStatus(data)
        ownerWindow.webContents.send('agent:statusUpdate', {
          ptySessionId: sessionRef.ptySessionId,
          agentType: adapter.agentType,
          status: normalized,
        })
      },
    )

    // Always use .sh scripts — Claude Code runs hooks via bash even on Windows
    const hookScriptPath = this.getResourceScript('canopy-agent-hook.sh')
    const statusLineScriptPath = this.getResourceScript('canopy-agent-statusline.sh')

    // Ensure scripts are executable
    if (process.platform !== 'win32') {
      try {
        chmodSync(hookScriptPath, 0o755)
      } catch {
        // May not have permission in packaged app
      }
      try {
        chmodSync(statusLineScriptPath, 0o755)
      } catch {
        // May not have permission in packaged app
      }
    }

    const settingsPath = join(this.hooksDir, `session-${agentSessionId}.json`)
    const settingsSetup = adapter.setupSettings(
      settingsPath,
      worktreePath,
      hookScriptPath,
      statusLineScriptPath,
      settingsOverrides,
    )

    const session: AgentSession = {
      agentType: adapter.agentType,
      adapter,
      agentSessionId,
      ptySessionId: tempId,
      settingsSetup,
      hookPort,
      hookPath,
      worktreePath,
      workspaceName,
      branch,
      sessionRef,
      ownerWindow,
    }
    this.sessions.set(tempId, session)

    return {
      settingsArgs: settingsSetup.args,
      settingsEnv: settingsSetup.env,
      hookPort,
      hookPath,
      hookAuthToken,
      tempId,
    }
  }

  /** Build CLI args using real preferences reader */
  getCliArgs(toolId: string, prefs: { get(key: string): string | null }): string[] {
    const adapter = getAdapter(toolId)
    return adapter ? adapter.buildCliArgs(prefs) : []
  }

  /** Build env vars using real preferences reader */
  getEnvVars(toolId: string, prefs: { get(key: string): string | null }): Record<string, string> {
    const adapter = getAdapter(toolId)
    return adapter ? adapter.buildEnvVars(prefs) : {}
  }

  /** Get resume args for the agent */
  getResumeArgs(toolId: string, resumeSessionId: string): string[] {
    const adapter = getAdapter(toolId)
    return adapter?.buildResumeArgs?.(resumeSessionId) ?? []
  }

  rekey(tempId: string, realPtySessionId: string): void {
    const session = this.sessions.get(tempId)
    if (!session) return
    this.sessions.delete(tempId)
    session.ptySessionId = realPtySessionId
    session.sessionRef.ptySessionId = realPtySessionId
    this.sessions.set(realPtySessionId, session)
  }

  isBusy(ptySessionId: string): boolean {
    return this.busySessions.has(ptySessionId)
  }

  isAgentSession(ptySessionId: string): boolean {
    return this.sessions.has(ptySessionId)
  }

  getAgentType(ptySessionId: string): AgentType | null {
    return this.sessions.get(ptySessionId)?.agentType ?? null
  }

  destroySession(ptySessionId: string): void {
    const session = this.sessions.get(ptySessionId)
    if (!session) return

    this.router.removeSession(session.agentSessionId)
    session.settingsSetup.cleanup()
    this.busySessions.delete(ptySessionId)
    this.emit('sessionDestroyed', ptySessionId)
    this.sessions.delete(ptySessionId)
  }

  cleanupOrphans(): void {
    if (!existsSync(this.hooksDir)) return
    const entries = readdirSync(this.hooksDir)
    for (const entry of entries) {
      const fullPath = join(this.hooksDir, entry)
      try {
        if (entry.startsWith('session-') && entry.endsWith('.json')) {
          unlinkSync(fullPath)
        } else if (entry.startsWith('gemini-home-') && statSync(fullPath).isDirectory()) {
          rmSync(fullPath, { recursive: true, force: true })
        }
      } catch {
        // Ignore
      }
    }
  }

  dispose(): void {
    for (const [id] of this.sessions) {
      this.destroySession(id)
    }
    this.busySessions.clear()
    this.router.dispose()
  }

  isAgentTool(toolId: string): boolean {
    return isRegistered(toolId)
  }

  private emitStatusChange(
    event: NormalizedHookEvent,
    ptySessionId: string,
    ownerWindow: BrowserWindow,
    workspaceName: string,
    branch: string | null,
  ): void {
    const session = this.sessions.get(ptySessionId)
    const adapter = session?.adapter
    if (!adapter) return

    const result = adapter.toNotchStatus(event)
    if (!result) return

    const sessionStatus: NotchSessionStatus = {
      ptySessionId,
      windowId: ownerWindow.webContents.id,
      workspaceName,
      branch,
      status: result.status,
      toolName: event.toolName,
      title: session?.processTitle,
      detail: result.detail,
      agentType: adapter.agentType,
    }
    this.emit('statusChange', sessionStatus)
  }

  private getResourceScript(name: string): string {
    const raw = is.dev
      ? join(process.cwd(), 'resources', name)
      : join(process.resourcesPath, 'app.asar.unpacked', 'resources', name)
    return process.platform === 'win32' ? raw.replaceAll('\\', '/') : raw
  }

  private showNotification(
    title: string,
    body: string,
    ptySessionId: string,
    ownerWindow: BrowserWindow,
  ): void {
    if (!Notification.isSupported()) return
    const n = new Notification({ title, body })
    n.on('click', () => {
      if (ownerWindow.isDestroyed()) return
      ownerWindow.focus()
      ownerWindow.webContents.send('agent:focusSession', { ptySessionId })
    })
    n.show()
  }
}

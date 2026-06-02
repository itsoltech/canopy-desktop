import type { WebContents } from 'electron'
import os from 'os'
import path from 'path'
import type { PtyManager } from '../pty/PtyManager'
import { resolveShell } from '../pty/PtyManager'
import type { WsBridge } from '../pty/WsBridge'
import type { WindowManager } from '../WindowManager'
import type { RunConfigManager } from '../runConfig/RunConfigManager'
import { runConfigErrorMessage } from '../runConfig/errors'
import { BLOCKED_ENV_VARS } from '../security/envBlocklist'
import type { RunConfigCommandResult, RunConfigProcessSnapshot } from './types'
import type { Result } from 'neverthrow'

interface RunConfigCommandServiceDeps {
  ptyManager: PtyManager
  wsBridge: WsBridge
  windowManager: WindowManager
  runConfigManager: RunConfigManager
  validatePathAccess: (webContentsId: number, targetPath: string) => Promise<string>
}

interface RunConfigExecutePayload {
  configDir: string
  name: string
  cwd?: string
}

function unwrapOrThrow<T, E>(result: Result<T, E>, toMessage: (e: E) => string): T {
  if (result.isErr()) throw new Error(toMessage(result.error))
  return result.value
}

function shellExecArgs(command: string): { command: string; args: string[] } {
  const shell = resolveShell()
  const flag = os.platform() === 'win32' ? '-Command' : '-lc'
  return { command: shell.command, args: [flag, command] }
}

function filterRunConfigEnv(env?: Record<string, string>): Record<string, string> | undefined {
  if (!env) return undefined

  const filtered: Record<string, string> = {}
  for (const [key, value] of Object.entries(env)) {
    if (typeof value === 'string' && !BLOCKED_ENV_VARS.has(key.toUpperCase())) {
      filtered[key] = value
    }
  }
  return filtered
}

export class RunConfigCommandService {
  private runningProcesses = new Map<string, RunConfigProcessSnapshot>()
  private runConfigInstances = new Map<string, number>()

  constructor(private deps: RunConfigCommandServiceDeps) {}

  async execute(
    sender: WebContents,
    payload: RunConfigExecutePayload,
  ): Promise<RunConfigCommandResult> {
    const resolvedConfigDir = await this.deps.validatePathAccess(sender.id, payload.configDir)
    const fileResult = await this.deps.runConfigManager.loadFile(resolvedConfigDir)
    const file = unwrapOrThrow(fileResult, runConfigErrorMessage)
    const config = file.configurations.find((c) => c.name === payload.name)
    if (!config) throw new Error(`Configuration "${payload.name}" not found`)

    const instanceKey = this.instanceKey(resolvedConfigDir, payload.name)
    let reserved = false
    if (config.max_instances && config.max_instances > 0) {
      const current = this.runConfigInstances.get(instanceKey) ?? 0
      if (current >= config.max_instances) {
        throw new Error(`"${payload.name}" is already running (max ${config.max_instances})`)
      }
    }
    this.incrementInstance(instanceKey)
    reserved = true

    try {
      if (!payload.cwd) throw new Error('No worktree selected')
      const worktreeRoot = await this.deps.validatePathAccess(sender.id, payload.cwd)
      const cwd = config.cwd ? path.resolve(worktreeRoot, config.cwd) : worktreeRoot
      if (config.cwd && cwd !== worktreeRoot && !cwd.startsWith(worktreeRoot + path.sep)) {
        throw new Error('config.cwd must not escape the worktree directory')
      }
      // `.canopy/run.toml` can be committed, so treat env overrides as untrusted.
      const env = filterRunConfigEnv(config.env)
      const fullCommand = config.args ? `${config.command} ${config.args}` : config.command

      // Pre-run hook (30s timeout)
      if (config.pre_run) {
        const PRE_RUN_TIMEOUT = 30_000
        // Cap captured output so a noisy pre_run can't balloon main-process
        // memory before the timer fires. We only surface the last ~5 lines on
        // failure, so trimming the head is safe.
        const PRE_RUN_OUTPUT_CAP = 32_768
        const pre = shellExecArgs(config.pre_run)
        const preSession = this.deps.ptyManager.spawn({
          command: pre.command,
          args: pre.args,
          cwd,
          env,
        })
        let preOutput = ''
        preSession.pty.onData((data) => {
          preOutput += data
          if (preOutput.length > PRE_RUN_OUTPUT_CAP) {
            preOutput = preOutput.slice(-PRE_RUN_OUTPUT_CAP / 2)
          }
        })
        await new Promise<void>((resolve, reject) => {
          let done = false
          const timer = setTimeout(() => {
            if (!done) {
              done = true
              this.deps.ptyManager.kill(preSession.id)
              reject(new Error(`pre_run "${config.pre_run}" timed out after 30s`))
            }
          }, PRE_RUN_TIMEOUT)
          preSession.pty.onExit(({ exitCode }) => {
            if (done) return
            done = true
            clearTimeout(timer)
            this.deps.ptyManager.kill(preSession.id)
            if (exitCode !== 0) {
              const lastLines = preOutput.trim().split('\n').slice(-5).join('\n')
              reject(
                new Error(`pre_run "${config.pre_run}" failed (exit ${exitCode}):\n${lastLines}`),
              )
            } else resolve()
          })
        })
      }

      // Run main command through shell so PATH is resolved
      const main = shellExecArgs(fullCommand)
      const session = this.deps.ptyManager.spawn({
        command: main.command,
        args: main.args,
        cwd,
        env,
      })
      const senderId = sender.id
      let cleanedUp = false
      const cleanup = (): void => {
        if (cleanedUp) return
        cleanedUp = true
        this.deps.windowManager.untrackPtySession(senderId, session.id)
        this.runningProcesses.delete(session.id)
        this.decrementInstance(instanceKey)
      }

      session.pty.onExit(({ exitCode, signal }) => {
        if (!sender.isDestroyed()) {
          sender.send('pty:exit', { sessionId: session.id, exitCode, signal })
        }
        cleanup()

        // Post-run hook
        if (config.post_run) {
          const postCmd = config.post_run
          const post = shellExecArgs(postCmd)
          const postSession = this.deps.ptyManager.spawn({
            command: post.command,
            args: post.args,
            cwd,
            env,
          })
          const POST_RUN_TIMEOUT = 30_000
          let postDone = false
          const postTimer = setTimeout(() => {
            if (!postDone) {
              postDone = true
              this.deps.ptyManager.kill(postSession.id)
              if (!sender.isDestroyed()) {
                sender.send('runConfig:postRunResult', {
                  success: false,
                  command: postCmd,
                  exitCode: -1,
                })
              }
            }
          }, POST_RUN_TIMEOUT)
          postSession.pty.onExit(({ exitCode: postExit }) => {
            if (postDone) return
            postDone = true
            clearTimeout(postTimer)
            if (!sender.isDestroyed()) {
              sender.send(
                'runConfig:postRunResult',
                postExit === 0
                  ? { success: true, command: postCmd }
                  : { success: false, command: postCmd, exitCode: postExit },
              )
            }
            this.deps.ptyManager.kill(postSession.id)
          })
        }
      })

      this.deps.windowManager.trackPtySession(senderId, session.id)
      this.runningProcesses.set(session.id, {
        sessionId: session.id,
        name: payload.name,
        configDir: resolvedConfigDir,
        worktreePath: worktreeRoot,
      })
      reserved = false

      try {
        const wsUrl = await this.deps.wsBridge.create(session.id, session.pty)
        return { sessionId: session.id, wsUrl, running: this.listRunning(sender) }
      } catch (error) {
        cleanup()
        this.deps.ptyManager.kill(session.id)
        throw error
      }
    } finally {
      if (reserved) this.decrementInstance(instanceKey)
    }
  }

  listRunning(sender?: WebContents): RunConfigProcessSnapshot[] {
    const snapshots = Array.from(this.runningProcesses.values())
    if (!sender) return snapshots
    return snapshots.filter((snapshot) =>
      this.deps.windowManager.ownsPtySession(sender.id, snapshot.sessionId),
    )
  }

  private instanceKey(configDir: string, name: string): string {
    return `${configDir}::${name}`
  }

  private incrementInstance(instanceKey: string): void {
    this.runConfigInstances.set(instanceKey, (this.runConfigInstances.get(instanceKey) ?? 0) + 1)
  }

  private decrementInstance(instanceKey: string): void {
    const count = (this.runConfigInstances.get(instanceKey) ?? 1) - 1
    if (count <= 0) this.runConfigInstances.delete(instanceKey)
    else this.runConfigInstances.set(instanceKey, count)
  }
}

import type { DataChannelRpc } from '../../../../renderer-shared/rpc/DataChannelRpc'
import type { RpcMethods, RpcMethodName } from '../../../../renderer-shared/rpc/methodList'
import { StateSnapshotProvider } from './StateSnapshotProvider'
import { PtyStreamForwarder } from './PtyStreamForwarder'
import { checkAction, resetSessionGrants } from './actionGuard'
import { openTool, closeTab, switchTab, tabsByWorktree } from '../stores/tabs.svelte'
import { selectWorktree } from '../stores/workspace.svelte'
import { allPanes } from '../stores/splitTree'
import { substituteLocalhost } from '../../../../renderer-shared/url/localhostSubstitution'
import { remoteSession } from '../stores/remoteSession.svelte'

/**
 * Registers the Canopy host-side handlers for the RPC whitelist declared
 * in `renderer-shared/rpc/methodList.ts`. One instance is created per peer
 * connection and disposed alongside the `RemoteHostController` that owns it.
 *
 * Surface: diagnostics (`diag.ping`), live state mirror (`state.getSnapshot`
 * plus delta events via {@link StateSnapshotProvider}), and the write
 * action set (tools / tabs / PTY / agent / workspace / browser) behind the
 * action-guard system.
 *
 * Handlers validate their own params against the `RpcMethods` types because
 * the payload crossed a WebRTC data channel and is untrusted — TypeScript
 * doesn't help at the boundary.
 */
export class HostRpcServer {
  private disposed = false
  private stateProvider = new StateSnapshotProvider()
  private ptyForwarder: PtyStreamForwarder | null = null
  /** Cleanup fn returned by `window.api.onPtyResized` — set in
   *  `registerAllHandlers()` and invoked in `dispose()` so we don't leak
   *  IPC listeners when a peer disconnects. */
  private unsubPtyResized: (() => void) | null = null

  constructor(private rpc: DataChannelRpc) {}

  registerAllHandlers(): void {
    this.register('diag.ping', (params) => {
      const n = assertNumber(params, 'n', 'diag.ping')
      return { n, receivedAt: Date.now() }
    })

    // The state provider registers `state.getSnapshot` on the RPC instance
    // AND installs reactive effects that emit delta events as the underlying
    // Svelte stores change. We wrap it in try/catch because `$effect.root`
    // can throw in non-component contexts depending on the Svelte runtime
    // state; a throw must NOT break the rest of handler registration, or
    // the peer gets spurious `method_not_found` errors for every write action.
    try {
      this.stateProvider.attach(this.rpc)
    } catch (err) {
      console.error('[remote] stateProvider.attach failed:', err)
    }

    // --- Phase 7: command forwarding (write actions) ---

    // Each write action finishes with `stateProvider.rebroadcast()` as a
    // fallback for Svelte 5 reactive tracking, which can miss mutations on
    // nested record stores (e.g. `activeTabId[worktreePath] = x`). Without
    // this, the peer stays visually out-of-sync even though the host state
    // has updated.
    const provider = this.stateProvider

    this.register('tools.spawn', async (params) => {
      const toolId = assertString(params, 'toolId', 'tools.spawn')
      const worktreePath = assertString(params, 'worktreePath', 'tools.spawn')
      const tab = await openTool(toolId, worktreePath)
      provider.rebroadcast()
      return { tabId: tab.id }
    })

    this.register('tabs.close', async (params) => {
      const tabId = assertString(params, 'tabId', 'tabs.close')
      await closeTab(tabId)
      provider.rebroadcast()
    })

    this.register('tabs.activate', async (params) => {
      const tabId = assertString(params, 'tabId', 'tabs.activate')
      await switchTab(tabId)
      provider.rebroadcast()
    })

    this.register('workspace.selectWorktree', async (params) => {
      const worktreePath = assertString(params, 'worktreePath', 'workspace.selectWorktree')
      await selectWorktree(worktreePath)
      provider.rebroadcast()
    })

    this.register('pty.write', async (params) => {
      const sessionId = assertString(params, 'sessionId', 'pty.write')
      const data = assertString(params, 'data', 'pty.write')
      await window.api.writePty(sessionId, data)
    })

    this.register('pty.resize', async (params) => {
      const sessionId = assertString(params, 'sessionId', 'pty.resize')
      const cols = assertNumber(params, 'cols', 'pty.resize')
      const rows = assertNumber(params, 'rows', 'pty.resize')
      // Clamp to sane bounds. 1×1 is rejected by node-pty on some platforms
      // and >500 cols is silly on any real viewport — both would just be
      // noise from a misbehaving peer computing bad dimensions.
      const safeCols = Math.max(10, Math.min(cols, 500))
      const safeRows = Math.max(3, Math.min(rows, 200))
      await window.api.resizePty(sessionId, safeCols, safeRows)
      // The `pty:resize` IPC handler already broadcasts `pty:resized` to
      // every window, which the host's own `onPtyResized` relay (set up
      // above) then emits as an RPC event. No extra work here.
    })

    this.register('agent.sendInput', async (params) => {
      const sessionId = assertString(params, 'sessionId', 'agent.sendInput')
      const text = assertString(params, 'text', 'agent.sendInput')
      // PTYs interpret Enter as carriage return (`\r`), NOT line feed
      // (`\n`). Shell, Claude CLI, Gemini — all of them treat `\n` as
      // a literal newline character inside the current input buffer
      // without submitting. We strip any trailing newline and append `\r`
      // to fire the actual Enter.
      const stripped = text.replace(/[\r\n]+$/, '')
      await window.api.writePty(sessionId, `${stripped}\r`)
    })

    this.register('pty.kill', async (params) => {
      const sessionId = assertString(params, 'sessionId', 'pty.kill')
      await window.api.killPty(sessionId)
      provider.rebroadcast()
    })

    // PTY stream forwarding. When the peer calls pty.subscribe, the
    // forwarder opens a secondary WebSocket to the same WsBridge the desktop
    // xterm.js uses, and relays each chunk as an RPC event.
    this.ptyForwarder = new PtyStreamForwarder(this.rpc)
    const forwarder = this.ptyForwarder

    // Relay PTY resize events from the host to the peer. When the host
    // desktop window resizes (or a split pane's fit addon re-runs),
    // `pty:resize` IPC fires and the main process broadcasts
    // `pty:resized` to every renderer. We forward each one as a topic
    // event keyed by sessionId so the peer's RemoteTerminalView can
    // subscribe and call `term.resize(cols, rows)`. Without this, the
    // peer's xterm stays at the PTY's original dimensions and cursor
    // positioning escape sequences land in the wrong column after a
    // host resize.
    this.unsubPtyResized = window.api.onPtyResized((sessionId, cols, rows) => {
      this.rpc.emit(`pty.resized.${sessionId}`, { cols, rows })
    })

    this.register('pty.subscribe', (params) => {
      const sessionId = assertString(params, 'sessionId', 'pty.subscribe')
      const wsUrl = findWsUrlForSession(sessionId)
      if (!wsUrl) throw new Error(`No active session with id ${sessionId}`)
      forwarder.subscribe(sessionId, wsUrl)
    })

    this.register('pty.unsubscribe', (params) => {
      const sessionId = assertString(params, 'sessionId', 'pty.unsubscribe')
      forwarder.unsubscribe(sessionId)
    })

    this.register('pty.getDimensions', async (params) => {
      const sessionId = assertString(params, 'sessionId', 'pty.getDimensions')
      return await window.api.getPtyDimensions(sessionId)
    })

    this.register('browser.openExternal', (params) => {
      // The host doesn't actually open anything — the remote peer
      // side opens the URL in its own browser after we substitute the
      // host for any `localhost` references.
      const url = assertString(params, 'url', 'browser.openExternal')
      return { substitutedUrl: substituteLocalhost(url, getHostLanIp()) }
    })

    // Diagnostic log so we can confirm in the dev console that the whole
    // registration sequence completed without interruption.
    console.log('[remote] HostRpcServer: registered all handlers')
  }

  dispose(): void {
    if (this.disposed) return
    this.disposed = true
    resetSessionGrants()
    this.unsubPtyResized?.()
    this.unsubPtyResized = null
    this.ptyForwarder?.dispose()
    this.ptyForwarder = null
    this.stateProvider.detach()
    // The underlying DataChannelRpc is disposed by its owner (the host
    // controller), which also clears registered handlers, so nothing else
    // to do here.
  }

  /**
   * Typed wrapper around `rpc.registerMethod` so handler signatures stay in
   * sync with `RpcMethods`. Every call is passed through the action guard
   * first — if it rejects, the RPC response is an error and the handler
   * never executes.
   */
  private register<M extends RpcMethodName>(
    method: M,
    handler: (
      params: RpcMethods[M]['params'],
    ) => RpcMethods[M]['result'] | Promise<RpcMethods[M]['result']>,
  ): void {
    this.rpc.registerMethod(method, async (params) => {
      const allowed = await checkAction(method, params)
      if (!allowed) {
        throw new Error(`Action "${method}" was rejected on desktop`)
      }
      return handler(params as RpcMethods[M]['params'])
    })
  }
}

function assertNumber(obj: unknown, key: string, method: string): number {
  if (typeof obj !== 'object' || obj === null) {
    throw new Error(`${method}: params must be an object`)
  }
  const value = (obj as Record<string, unknown>)[key]
  if (typeof value !== 'number' || !Number.isFinite(value)) {
    throw new Error(`${method}: "${key}" must be a finite number`)
  }
  return value
}

function assertString(obj: unknown, key: string, method: string): string {
  if (typeof obj !== 'object' || obj === null) {
    throw new Error(`${method}: params must be an object`)
  }
  const value = (obj as Record<string, unknown>)[key]
  if (typeof value !== 'string' || value.length === 0) {
    throw new Error(`${method}: "${key}" must be a non-empty string`)
  }
  return value
}

/** Read the LAN IP from the remote session status (populated during pairing). */
function getHostLanIp(): string {
  const s = remoteSession.status
  if ('lanIp' in s && typeof s.lanIp === 'string' && s.lanIp.length > 0) return s.lanIp
  return 'localhost'
}

/**
 * Walk every tab in every worktree, flatten their split trees, and find the
 * pane whose sessionId matches. Returns the `wsUrl` the terminal is
 * connected to so the {@link PtyStreamForwarder} can open a second WS.
 */
function findWsUrlForSession(sessionId: string): string | null {
  for (const tabs of Object.values(tabsByWorktree)) {
    for (const tab of tabs) {
      const panes = allPanes(tab.rootSplit)
      const match = panes.find((p) => p.sessionId === sessionId)
      if (match) return match.wsUrl || null
    }
  }
  return null
}

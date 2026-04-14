// Copied from src/renderer/src/remote/lib/api.ts
// Keep in sync when the wire protocol changes.

import type { DataChannelRpc } from './DataChannelRpc'
import { PeerRpcClient } from './PeerRpcClient'
import type { RpcMethods } from './protocol/rpc-methods'

/**
 * Typed surface the remote client uses to talk to the Canopy host.
 * Shapes mirror the dotted method names in `RpcMethods` as nested
 * namespaces, so `remoteApi.diag.ping(42)` maps to `'diag.ping'`.
 */
export interface RemoteApi {
  diag: {
    ping: (n: number) => Promise<RpcMethods['diag.ping']['result']>
  }
  state: {
    getSnapshot: () => Promise<RpcMethods['state.getSnapshot']['result']>
  }
  tools: {
    spawn: (toolId: string, worktreePath: string) => Promise<RpcMethods['tools.spawn']['result']>
  }
  tabs: {
    close: (tabId: string) => Promise<void>
    activate: (tabId: string) => Promise<void>
  }
  pty: {
    write: (sessionId: string, data: string) => Promise<void>
    resize: (sessionId: string, cols: number, rows: number) => Promise<void>
    kill: (sessionId: string) => Promise<void>
    subscribe: (sessionId: string) => Promise<void>
    unsubscribe: (sessionId: string) => Promise<void>
    getDimensions: (sessionId: string) => Promise<RpcMethods['pty.getDimensions']['result']>
  }
  agent: {
    sendInput: (sessionId: string, text: string) => Promise<void>
  }
  workspace: {
    selectWorktree: (worktreePath: string) => Promise<void>
  }
  browser: {
    openExternal: (url: string) => Promise<RpcMethods['browser.openExternal']['result']>
  }
  /** Escape hatch for topic subscriptions (projects/tabs/pty.data.<id>). */
  subscribe: <T>(topic: string, handler: (data: T) => void) => () => void
}

export function createRemoteApi(rpc: DataChannelRpc): RemoteApi {
  const client = new PeerRpcClient(rpc)
  return {
    diag: {
      ping: (n) => client.call('diag.ping', { n }),
    },
    state: {
      getSnapshot: () => client.call('state.getSnapshot'),
    },
    tools: {
      spawn: (toolId, worktreePath) => client.call('tools.spawn', { toolId, worktreePath }),
    },
    tabs: {
      close: (tabId) => client.call('tabs.close', { tabId }),
      activate: (tabId) => client.call('tabs.activate', { tabId }),
    },
    pty: {
      write: (sessionId, data) => client.call('pty.write', { sessionId, data }),
      resize: (sessionId, cols, rows) => client.call('pty.resize', { sessionId, cols, rows }),
      kill: (sessionId) => client.call('pty.kill', { sessionId }),
      subscribe: (sessionId) => client.call('pty.subscribe', { sessionId }),
      unsubscribe: (sessionId) => client.call('pty.unsubscribe', { sessionId }),
      getDimensions: (sessionId) => client.call('pty.getDimensions', { sessionId }),
    },
    agent: {
      sendInput: (sessionId, text) => client.call('agent.sendInput', { sessionId, text }),
    },
    workspace: {
      selectWorktree: (worktreePath) => client.call('workspace.selectWorktree', { worktreePath }),
    },
    browser: {
      openExternal: (url) => client.call('browser.openExternal', { url }),
    },
    subscribe: (topic, handler) => client.subscribe(topic, handler),
  }
}

import type { DataChannelRpc } from '../../../../renderer-shared/rpc/DataChannelRpc'
import { REMOTE_PROTOCOL_VERSION } from '../../../../renderer-shared/rpc/protocol'
import type {
  ProfileSnapshot,
  ProjectSnapshot,
  StateSnapshot,
  TabSnapshot,
  ToolSnapshot,
  WorktreeSnapshot,
} from '../../../../renderer-shared/state/snapshot'
import { agentSessions } from '../agents/agentState.svelte'
import { getWorktreeAgentStatus } from '../agents/worktreeStatus.svelte'
import { projects, workspaceState, type ProjectState } from '../stores/workspace.svelte'
import { remoteSession } from '../stores/remoteSession.svelte'
import { tabsByWorktree, activeTabId, type TabInfo } from '../stores/tabs.svelte'
import { getTools, getToolAvailability } from '../stores/tools.svelte'
import { getProfiles } from '../stores/profiles.svelte'
import { findLeaf } from '../stores/splitTree'

/**
 * Host-side bridge from reactive Svelte stores to the remote peer.
 *
 * Responsibilities:
 *   - Serialize the current store state into a {@link StateSnapshot} on demand
 *     (via the `state.getSnapshot` RPC call).
 *   - Subscribe to store changes using `$effect.root` and emit delta events
 *     on the same RPC when they happen.
 *
 * Why `$effect.root`: a plain `$effect` only runs inside a Svelte component's
 * lifecycle, but the provider is owned by `RemoteHostController` (a plain
 * class) that lives outside the component tree. `$effect.root` creates a
 * standalone effect scope we can dispose on `detach()`.
 *
 * Phase 6 emits *full-replace* deltas per topic (projects / tabs /
 * activeTab / activeWorktree / tools) rather than fine-grained patches.
 * Payloads are small in practice and this keeps the applier trivial. The
 * cost is that every effect run re-serializes the topic — acceptable at
 * Canopy's scale (a handful of projects, a handful of tabs).
 */
export class StateSnapshotProvider {
  private disposeEffects: (() => void) | null = null
  private rpc: DataChannelRpc | null = null

  attach(rpc: DataChannelRpc): void {
    if (this.rpc) return
    this.rpc = rpc

    rpc.registerMethod('state.getSnapshot', () => this.buildSnapshot())

    // $effect.root gives us a standalone reactive scope. Each $effect inside
    // reruns whenever its tracked state changes and pushes a delta down the
    // channel. The returned function tears down all inner effects atomically.
    //
    // We wrap it in try/catch because depending on when Svelte initialises
    // its reactivity runtime, calling `$effect.root` from a plain class
    // method (not from a component) can throw. If it does, we fall back to
    // a one-shot snapshot emit — the peer still works, just without live
    // delta updates until the next call to `state.getSnapshot`.
    try {
      this.disposeEffects = $effect.root(() => {
        $effect(() => {
          // Explicitly walk every agent session and read `.status.type` so
          // Svelte's dep tracker pins this effect to agent status mutations,
          // independent of the transitive read through
          // `serializeWorktree → getWorktreeAgentStatus`. Without this the
          // deep nested proxy read can be missed, leaving mobile dots frozen
          // at their initial snapshot state.
          for (const id in agentSessions) {
            void agentSessions[id]?.status.type
          }
          rpc.emit('projects', projects.map(serializeProject))
        })

        $effect(() => {
          // `tabsByWorktree` is a reactive map; accessing each worktree entry
          // subscribes us to changes anywhere in the map.
          const out: Record<string, TabSnapshot[]> = {}
          for (const [path, tabs] of Object.entries(tabsByWorktree)) {
            out[path] = tabs.map(serializeTab)
          }
          rpc.emit('tabs', out)
        })

        $effect(() => {
          rpc.emit('activeTab', { ...activeTabId })
        })

        $effect(() => {
          rpc.emit('activeWorktree', workspaceState.selectedWorktreePath)
        })

        $effect(() => {
          rpc.emit('tools', serializeTools())
        })

        $effect(() => {
          rpc.emit('profiles', serializeProfiles())
        })
      })
    } catch (err) {
      console.error(
        '[remote] StateSnapshotProvider: $effect.root failed, delta updates disabled',
        err,
      )
    }
  }

  detach(): void {
    this.disposeEffects?.()
    this.disposeEffects = null
    this.rpc = null
  }

  /**
   * Emit a fresh copy of every topic right now, bypassing reactive effects.
   * Used by write-action handlers in `HostRpcServer` as a belt-and-
   * suspenders fallback: Svelte 5's dependency tracking can miss mutations
   * on nested record stores (`activeTabId[worktreePath] = x`), so we force
   * a rebroadcast after every RPC that touches the stores to guarantee the
   * peer mirror stays in sync regardless of whether `$effect.root`
   * correctly fired.
   */
  rebroadcast(): void {
    if (!this.rpc) return
    const rpc = this.rpc
    try {
      rpc.emit('projects', projects.map(serializeProject))
      const tabsOut: Record<string, TabSnapshot[]> = {}
      for (const [path, tabs] of Object.entries(tabsByWorktree)) {
        tabsOut[path] = tabs.map(serializeTab)
      }
      rpc.emit('tabs', tabsOut)
      rpc.emit('activeTab', { ...activeTabId })
      rpc.emit('activeWorktree', workspaceState.selectedWorktreePath)
      rpc.emit('tools', serializeTools())
      rpc.emit('profiles', serializeProfiles())
    } catch (err) {
      console.warn('[remote] rebroadcast failed:', err)
    }
  }

  /**
   * Build a fresh snapshot from the current reactive stores. Called by the
   * `state.getSnapshot` RPC method; also safe to call outside an RPC context
   * for debugging.
   */
  buildSnapshot(): StateSnapshot {
    return {
      protocolVersion: REMOTE_PROTOCOL_VERSION,
      // hostInfo stays sparse; the remote peer already knows the
      // LAN IP from the pairing URL and the friendly hostname from the
      // `h=` fragment parameter, so the snapshot copy is just for display.
      hostInfo: {
        hostname: workspaceState.workspace?.name ?? 'Canopy',
        // The renderer doesn't know the LAN IP directly — it comes from
        // the RemoteSessionService via the pairing URL. We grab it from
        // the remoteSession store which mirrors the main-process status.
        lanIp: getLanIpFromStatus(),
      },
      projects: projects.map(serializeProject),
      tabsByWorktree: serializeTabsByWorktree(),
      activeTabByWorktree: { ...activeTabId },
      activeWorktreePath: workspaceState.selectedWorktreePath,
      tools: serializeTools(),
      profiles: serializeProfiles(),
    }
  }
}

function serializeProject(project: ProjectState): ProjectSnapshot {
  return {
    id: project.workspace.id,
    path: project.workspace.path,
    name: project.workspace.name,
    isGitRepo: project.isGitRepo,
    repoRoot: project.repoRoot,
    worktrees: project.worktrees.map(serializeWorktree),
  }
}

function serializeWorktree(wt: {
  path: string
  branch: string
  isMain: boolean
}): WorktreeSnapshot {
  // Reading agentSessions through getWorktreeAgentStatus inside the
  // projects $effect means Svelte tracks it too, so agent status changes
  // trigger a fresh 'projects' delta without needing a separate topic.
  return {
    path: wt.path,
    branch: wt.branch,
    isMain: wt.isMain,
    agentStatus: getWorktreeAgentStatus(wt.path),
  }
}

function serializeTab(tab: TabInfo): TabSnapshot {
  const focused = findLeaf(tab.rootSplit, tab.focusedPaneId)
  return {
    id: tab.id,
    toolId: tab.toolId,
    toolName: tab.toolName,
    name: tab.name,
    worktreePath: tab.worktreePath,
    paneType: focused?.paneType,
    focusedSessionId: focused?.sessionId,
    focusedUrl: focused?.url,
  }
}

function serializeTabsByWorktree(): Record<string, TabSnapshot[]> {
  const out: Record<string, TabSnapshot[]> = {}
  for (const [path, tabs] of Object.entries(tabsByWorktree)) {
    out[path] = tabs.map(serializeTab)
  }
  return out
}

function serializeTools(): ToolSnapshot[] {
  const availability = getToolAvailability()
  return getTools().map((t) => ({
    id: t.id,
    name: t.name,
    icon: t.icon,
    category: t.category,
    isCustom: t.isCustom,
    available: availability[t.id] !== false,
  }))
}

function serializeProfiles(): ProfileSnapshot[] {
  // Trim to the minimum the peer picker needs — NEVER include
  // `prefs.customEnv` / `prefs.settingsJson`: those routinely hold API
  // tokens and provider URLs and must not cross the WebRTC wire.
  return getProfiles()
    .map((p) => ({
      id: p.id,
      agentType: p.agentType,
      name: p.name,
      isDefault: p.isDefault,
      sortIndex: p.sortIndex,
    }))
    .sort((a, b) => a.sortIndex - b.sortIndex || a.name.localeCompare(b.name))
}

function getLanIpFromStatus(): string {
  const s = remoteSession.status
  if ('lanIp' in s && typeof s.lanIp === 'string') return s.lanIp
  return ''
}

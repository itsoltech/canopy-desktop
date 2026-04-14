import { useSyncExternalStore } from 'react'

import { getMirrorState, subscribeMirrorState, type MirrorState } from '@/lib/remote/mirror-state'
import type {
  ProfileSnapshot,
  ProjectSnapshot,
  TabSnapshot,
  ToolSnapshot,
} from '@/lib/remote/protocol/state-snapshot'

/**
 * Reactive view over the mirror state populated by `StateApplier`. Components
 * calling these hooks automatically re-render when the corresponding topic
 * receives a delta from the host.
 */
export function useRemoteState(): MirrorState {
  return useSyncExternalStore(subscribeMirrorState, getMirrorState, getMirrorState)
}

export function useProjects(): ProjectSnapshot[] {
  return useRemoteState().projects
}

export function useTabsFor(worktreePath: string): TabSnapshot[] {
  const { tabsByWorktree } = useRemoteState()
  return tabsByWorktree[worktreePath] ?? []
}

export function useActiveTabId(worktreePath: string): string | null {
  const { activeTabByWorktree } = useRemoteState()
  return activeTabByWorktree[worktreePath] ?? null
}

export function useIsHydrated(): boolean {
  return useRemoteState().hydrated
}

export function useTools(): ToolSnapshot[] {
  return useRemoteState().tools
}

export function useProfiles(): ProfileSnapshot[] {
  return useRemoteState().profiles
}

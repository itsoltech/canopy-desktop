// Singleton that maps mirror-state changes to iOS Live Activity lifecycle.
// iOS suspends the JS runtime ~30s after the app is backgrounded, so the
// activity freezes at its last pushed state until the app returns to the
// foreground — v1 accepts this, no APNs push path.

import type { LiveActivity } from 'expo-widgets'

import { getSessionState, subscribeSession } from '../remote/session'
import { getMirrorState, subscribeMirrorState } from '../remote/mirror-state'
import type { ProjectSnapshot, WorktreeAgentStatus } from '../remote/protocol/state-snapshot'

import type { AgentActivityProps, AgentActivitySlotStatus, AgentActivityStatus } from './types'
import { CanopyAgentActivity } from './widget'

type Slot = { branch: string; status: AgentActivitySlotStatus }

const IDLE_PROPS: AgentActivityProps = {
  overallStatus: 'idle',
  workingCount: 0,
  waitingCount: 0,
  errorCount: 0,
  top1Branch: '',
  top1Status: '',
  top2Branch: '',
  top2Status: '',
  top3Branch: '',
  top3Status: '',
}

function mapStatus(raw: WorktreeAgentStatus | undefined): AgentActivitySlotStatus {
  if (raw === 'working') return 'working'
  if (raw === 'waitingPermission') return 'waiting'
  if (raw === 'error') return 'error'
  if (raw === 'idle') return 'idle'
  return ''
}

function priority(status: AgentActivitySlotStatus): number {
  if (status === 'error') return 0
  if (status === 'waiting') return 1
  if (status === 'working') return 2
  return 3
}

function deriveOverall(
  workingCount: number,
  waitingCount: number,
  errorCount: number,
): AgentActivityStatus {
  if (errorCount > 0) return 'error'
  if (waitingCount > 0) return 'waiting'
  if (workingCount > 0) return 'working'
  return 'idle'
}

export function computeAggregate(projects: ProjectSnapshot[]): AgentActivityProps {
  const active: Slot[] = []
  let workingCount = 0
  let waitingCount = 0
  let errorCount = 0

  for (const project of projects) {
    for (const wt of project.worktrees) {
      const status = mapStatus(wt.agentStatus)
      if (status === '' || status === 'idle') continue
      if (status === 'working') workingCount++
      else if (status === 'waiting') waitingCount++
      else if (status === 'error') errorCount++
      active.push({ branch: wt.branch, status })
    }
  }

  active.sort((a, b) => priority(a.status) - priority(b.status))
  const top = active.slice(0, 3)

  return {
    overallStatus: deriveOverall(workingCount, waitingCount, errorCount),
    workingCount,
    waitingCount,
    errorCount,
    top1Branch: top[0]?.branch ?? '',
    top1Status: top[0]?.status ?? '',
    top2Branch: top[1]?.branch ?? '',
    top2Status: top[1]?.status ?? '',
    top3Branch: top[2]?.branch ?? '',
    top3Status: top[2]?.status ?? '',
  }
}

function propsEqual(a: AgentActivityProps, b: AgentActivityProps): boolean {
  return (
    a.overallStatus === b.overallStatus &&
    a.workingCount === b.workingCount &&
    a.waitingCount === b.waitingCount &&
    a.errorCount === b.errorCount &&
    a.top1Branch === b.top1Branch &&
    a.top1Status === b.top1Status &&
    a.top2Branch === b.top2Branch &&
    a.top2Status === b.top2Status &&
    a.top3Branch === b.top3Branch &&
    a.top3Status === b.top3Status
  )
}

let instance: LiveActivity<AgentActivityProps> | null = null
let lastProps: AgentActivityProps = IDLE_PROPS
let permissionDenied = false

function startOrUpdate(next: AgentActivityProps): void {
  if (permissionDenied) return
  if (instance === null) {
    try {
      instance = CanopyAgentActivity.start(next)
      lastProps = next
    } catch (err) {
      permissionDenied = true
      console.warn('[live-activity] start failed, disabling for session:', err)
    }
    return
  }
  if (propsEqual(next, lastProps)) return
  instance.update(next).catch((err) => {
    console.warn('[live-activity] update failed:', err)
  })
  lastProps = next
}

function endIfActive(): void {
  if (instance === null) return
  const ref = instance
  instance = null
  lastProps = IDLE_PROPS
  ref.end('immediate').catch((err) => {
    console.warn('[live-activity] end failed:', err)
  })
}

function tick(): void {
  const session = getSessionState()
  if (session.kind !== 'ready') {
    endIfActive()
    return
  }
  const props = computeAggregate(getMirrorState().projects)
  if (props.overallStatus === 'idle') {
    endIfActive()
    return
  }
  startOrUpdate(props)
}

export function initAgentActivityManager(): () => void {
  const unsubMirror = subscribeMirrorState(tick)
  const unsubSession = subscribeSession(() => tick())
  tick()
  return () => {
    unsubMirror()
    unsubSession()
    endIfActive()
  }
}

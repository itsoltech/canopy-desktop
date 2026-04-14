export type AgentActivityStatus = 'idle' | 'working' | 'waiting' | 'error'

export type AgentActivitySlotStatus = '' | 'idle' | 'working' | 'waiting' | 'error'

export type AgentActivityProps = {
  overallStatus: AgentActivityStatus
  workingCount: number
  waitingCount: number
  errorCount: number
  top1Branch: string
  top1Status: AgentActivitySlotStatus
  top2Branch: string
  top2Status: AgentActivitySlotStatus
  top3Branch: string
  top3Status: AgentActivitySlotStatus
}

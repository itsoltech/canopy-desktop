import { describe, it, expect, beforeEach } from 'vitest'
import {
  agentSessions,
  handleHookEvent,
  initAgentSession,
  removeAgentSession,
  type TaskRecord,
} from './agentState.svelte'

const SESSION = 'pty-1'

function createTask(id: string, subject: string): void {
  handleHookEvent(SESSION, {
    event: 'AfterToolUse',
    toolName: 'TaskCreate',
    toolInput: { subject },
    toolResponse: { task: { id } },
  })
}

function updateTaskStatus(id: string, status: unknown): void {
  handleHookEvent(SESSION, {
    event: 'AfterToolUse',
    toolName: 'TaskUpdate',
    toolInput: { taskId: id, status },
  })
}

function statusOf(id: string): TaskRecord['status'] | undefined {
  return agentSessions[SESSION]?.tasks.find((t) => t.id === id)?.status
}

describe('agent task status from hook payloads', () => {
  beforeEach(() => {
    removeAgentSession(SESSION)
    initAgentSession(SESSION, 'claude')
  })

  it('applies a valid status transition', () => {
    createTask('t1', 'Write the migration')
    updateTaskStatus('t1', 'in_progress')
    expect(statusOf('t1')).toBe('in_progress')

    updateTaskStatus('t1', 'completed')
    expect(statusOf('t1')).toBe('completed')
  })

  it('ignores a status outside the TaskRecord union', () => {
    // The hook payload is JSON parsed from an external agent CLI over HTTP and
    // is not validated upstream, so an unknown status must not be written into
    // reactive state: it would count toward `total` but never toward `done`,
    // leaving task progress permanently short of 100%.
    createTask('t2', 'Review the PR')
    updateTaskStatus('t2', 'blocked')
    expect(statusOf('t2')).toBe('pending')
  })

  it('ignores a non-string status', () => {
    createTask('t3', 'Ship it')
    updateTaskStatus('t3', { nested: true })
    expect(statusOf('t3')).toBe('pending')
  })

  it('keeps a task evictable after a rejected status update', () => {
    // MAX_TASKS eviction only drops `completed` tasks, so a task left in a
    // bogus status would be un-evictable.
    createTask('t4', 'Tidy up')
    updateTaskStatus('t4', 'blocked')
    updateTaskStatus('t4', 'completed')
    expect(statusOf('t4')).toBe('completed')
  })
})

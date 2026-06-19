export interface TaskToAgentTask {
  key: string
  summary: string
  description?: string
  status?: string
  priority?: string
  type?: string
  parentKey?: string
  sprintName?: string
  sprintNumber?: number
  assignee?: string
  url?: string
}

export interface TaskToAgentTarget {
  sessionId: string
  worktreePath?: string
}

export type TaskToAgentOutcome =
  | { status: 'sent'; sessionId: string }
  | { status: 'no-active-agent'; message: string }
  | { status: 'agent-not-ready'; message: string; sessionId?: string }
  | { status: 'agent-start-failed'; message: string; errorMessage: string }
  | { status: 'tab-focus-failed'; message: string; errorMessage: string; sessionId?: string }
  | { status: 'context-build-failed'; message: string; errorMessage: string }
  | { status: 'paste-failed'; message: string; errorMessage: string; sessionId: string }

export interface SendTaskToAgentInput {
  connectionId: string
  task: TaskToAgentTask
  repoRoot?: string
  target: TaskToAgentTarget | null
}

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error)
}

export function noActiveAgentOutcome(): TaskToAgentOutcome {
  return {
    status: 'no-active-agent',
    message: 'No running agent is available. Start an agent and try again.',
  }
}

export function agentNotReadyOutcome(sessionId?: string): TaskToAgentOutcome {
  return {
    status: 'agent-not-ready',
    sessionId,
    message: 'The agent did not become ready. The worktree was created, but the task was not sent.',
  }
}

export function agentStartFailedOutcome(error: unknown): TaskToAgentOutcome {
  return {
    status: 'agent-start-failed',
    errorMessage: errorMessage(error),
    message: 'The worktree was created, but the selected agent could not be started.',
  }
}

export function tabFocusFailedOutcome(error: unknown, sessionId?: string): TaskToAgentOutcome {
  return {
    status: 'tab-focus-failed',
    sessionId,
    errorMessage: errorMessage(error),
    message: 'Could not focus the target agent tab. The task was not sent.',
  }
}

export async function sendTaskToAgentContext(
  input: SendTaskToAgentInput,
): Promise<TaskToAgentOutcome> {
  if (!input.target) return noActiveAgentOutcome()

  let context = ''
  try {
    context = await window.api.taskTrackerBuildTaskContext({
      connectionId: input.connectionId,
      task: input.task,
      repoRoot: input.repoRoot,
    })
  } catch (error) {
    return {
      status: 'context-build-failed',
      errorMessage: errorMessage(error),
      message: 'Could not build the task context. The task was not sent to the agent.',
    }
  }

  try {
    const result = await window.api.agentSendTaskContext({
      text: context,
      worktreePath: input.target.worktreePath,
      sessionId: input.target.sessionId,
    })
    return { status: 'sent', sessionId: result.sessionId }
  } catch (error) {
    return {
      status: 'paste-failed',
      sessionId: input.target.sessionId,
      errorMessage: errorMessage(error),
      message: 'Could not paste the task into the agent. The worktree is still available.',
    }
  }
}

export function taskToAgentErrorDetail(
  outcome: Exclude<TaskToAgentOutcome, { status: 'sent' }>,
): string | undefined {
  return outcome.status === 'agent-start-failed' ||
    outcome.status === 'tab-focus-failed' ||
    outcome.status === 'context-build-failed' ||
    outcome.status === 'paste-failed'
    ? outcome.errorMessage
    : undefined
}

/**
 * User-facing message for a failed send: the generic message plus the underlying
 * error detail when one is available. Surfacing the detail lets users report the
 * real cause (e.g. from the task-tracker provider) without opening DevTools.
 */
export function taskToAgentUserMessage(
  outcome: Exclude<TaskToAgentOutcome, { status: 'sent' }>,
): string {
  const detail = taskToAgentErrorDetail(outcome)
  return detail ? `${outcome.message} — ${detail}` : outcome.message
}

export function logTaskToAgentFailure(
  outcome: Exclude<TaskToAgentOutcome, { status: 'sent' }>,
  metadata: {
    taskKey: string
    connectionId: string
    selectedAgentId?: string
    sessionId?: string
  },
): void {
  console.error('Task to agent failed', {
    outcome: outcome.status,
    message: outcome.message,
    errorMessage:
      outcome.status === 'agent-start-failed' ||
      outcome.status === 'tab-focus-failed' ||
      outcome.status === 'context-build-failed' ||
      outcome.status === 'paste-failed'
        ? outcome.errorMessage
        : undefined,
    taskKey: metadata.taskKey,
    connectionId: metadata.connectionId,
    selectedAgentId: metadata.selectedAgentId,
    hasSessionId: Boolean(metadata.sessionId),
  })
}

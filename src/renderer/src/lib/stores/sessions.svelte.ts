interface SessionInfo {
  sessionId: string
  wsUrl: string
}

export const sessions: Record<string, SessionInfo> = $state({})

const pending: Record<string, Promise<SessionInfo>> = {}

export async function ensureSession(worktreePath: string): Promise<SessionInfo> {
  // Already have a session
  if (sessions[worktreePath]) {
    return sessions[worktreePath]
  }

  // Already spawning
  const inflight = pending[worktreePath]
  if (inflight) {
    return inflight
  }

  // Spawn a new PTY session
  const promise = window.api.spawnPty({ cwd: worktreePath }).then((result) => {
    const info: SessionInfo = { sessionId: result.sessionId, wsUrl: result.wsUrl }
    sessions[worktreePath] = info
    delete pending[worktreePath]
    return info
  })

  pending[worktreePath] = promise
  return promise
}

export function getSession(worktreePath: string): SessionInfo | null {
  return sessions[worktreePath] ?? null
}

export async function killSession(worktreePath: string): Promise<void> {
  const session = sessions[worktreePath]
  if (session) {
    await window.api.killPty(session.sessionId)
    delete sessions[worktreePath]
  }
}

export async function killAll(): Promise<void> {
  const paths = Object.keys(sessions)
  await Promise.all(paths.map((p) => killSession(p)))
}

interface QueuedGitProcess<T> {
  key: string | null
  operation: () => Promise<T>
  promise: Promise<T>
  resolve: (value: T | PromiseLike<T>) => void
  reject: (reason?: unknown) => void
}

export class GitProcessQueueFullError extends Error {
  constructor(maxPending: number) {
    super(`Git command queue is full (${maxPending} pending operations)`)
    this.name = 'GitProcessQueueFullError'
  }
}

/**
 * Process-wide, bounded lane for Git subprocesses.
 *
 * simple-git limits concurrency only within one client instance. Canopy creates
 * clients per request, so filesystem events and several visible UI panels could
 * otherwise start an unbounded number of Git processes at the same time.
 *
 * Identical read operations share one promise while queued or running. Mutating
 * operations pass a null key and are never deduplicated.
 */
export class GitProcessQueue {
  private active = false
  private readonly pending: QueuedGitProcess<unknown>[] = []
  private readonly keyed = new Map<string, Promise<unknown>>()
  private mutationGeneration = 0

  constructor(private readonly maxPending = 32) {}

  run<T>(key: string | null, operation: () => Promise<T>): Promise<T> {
    const effectiveKey = key === null ? null : JSON.stringify([this.mutationGeneration, key])
    if (key === null) this.mutationGeneration++

    if (effectiveKey) {
      const existing = this.keyed.get(effectiveKey)
      if (existing) return existing as Promise<T>
    }

    if (this.pending.length >= this.maxPending) {
      return Promise.reject(new GitProcessQueueFullError(this.maxPending))
    }

    let resolve!: (value: T | PromiseLike<T>) => void
    let reject!: (reason?: unknown) => void
    const promise = new Promise<T>((resolvePromise, rejectPromise) => {
      resolve = resolvePromise
      reject = rejectPromise
    })
    const task: QueuedGitProcess<T> = {
      key: effectiveKey,
      operation,
      promise,
      resolve,
      reject,
    }

    if (effectiveKey) this.keyed.set(effectiveKey, promise)
    this.pending.push(task as QueuedGitProcess<unknown>)
    this.startNext()
    return promise
  }

  private startNext(): void {
    if (this.active) return
    const task = this.pending.shift()
    if (!task) return

    this.active = true
    Promise.resolve()
      .then(task.operation)
      .then(
        (value) => {
          this.finish(task)
          task.resolve(value)
        },
        (error: unknown) => {
          this.finish(task)
          task.reject(error)
        },
      )
  }

  private finish(task: QueuedGitProcess<unknown>): void {
    if (task.key && this.keyed.get(task.key) === task.promise) {
      this.keyed.delete(task.key)
    }
    this.active = false
    this.startNext()
  }
}

const gitProcessQueue = new GitProcessQueue()

export function runGitProcess<T>(key: string | null, operation: () => Promise<T>): Promise<T> {
  return gitProcessQueue.run(key, operation)
}

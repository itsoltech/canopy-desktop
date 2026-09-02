import { describe, expect, it } from 'vitest'
import { GitProcessQueue, GitProcessQueueFullError } from './GitProcessQueue'

function deferred<T>(): {
  promise: Promise<T>
  resolve: (value: T) => void
} {
  let resolve!: (value: T) => void
  const promise = new Promise<T>((resolvePromise) => {
    resolve = resolvePromise
  })
  return { promise, resolve }
}

describe('GitProcessQueue', () => {
  it('allows only one Git operation to run at a time', async () => {
    const queue = new GitProcessQueue()
    const firstGate = deferred<void>()
    const started: string[] = []

    const first = queue.run(null, async () => {
      started.push('first')
      await firstGate.promise
      return 1
    })
    const second = queue.run(null, async () => {
      started.push('second')
      return 2
    })

    await Promise.resolve()
    expect(started).toEqual(['first'])

    firstGate.resolve()
    await expect(first).resolves.toBe(1)
    await expect(second).resolves.toBe(2)
    expect(started).toEqual(['first', 'second'])
  })

  it('shares identical read operations instead of building a backlog', async () => {
    const queue = new GitProcessQueue()
    const gate = deferred<number>()
    let calls = 0
    const operation = async (): Promise<number> => {
      calls++
      return gate.promise
    }

    const first = queue.run('repo:status', operation)
    const duplicate = queue.run('repo:status', operation)

    expect(duplicate).toBe(first)
    gate.resolve(7)
    await expect(Promise.all([first, duplicate])).resolves.toEqual([7, 7])
    expect(calls).toBe(1)
  })

  it('does not share a stale read across a queued mutation', async () => {
    const queue = new GitProcessQueue()
    const firstGate = deferred<string>()
    let readCalls = 0
    const read = async (): Promise<string> => {
      readCalls++
      if (readCalls === 1) return firstGate.promise
      return 'after mutation'
    }

    const before = queue.run('repo:status', read)
    const mutation = queue.run(null, async () => undefined)
    const after = queue.run('repo:status', read)

    expect(after).not.toBe(before)
    firstGate.resolve('before mutation')
    await expect(before).resolves.toBe('before mutation')
    await mutation
    await expect(after).resolves.toBe('after mutation')
    expect(readCalls).toBe(2)
  })

  it('rejects excess distinct operations when the bounded backlog is full', async () => {
    const queue = new GitProcessQueue(1)
    const gate = deferred<void>()
    const first = queue.run(null, () => gate.promise)
    const second = queue.run(null, async () => undefined)
    const excess = queue.run(null, async () => undefined)

    await expect(excess).rejects.toBeInstanceOf(GitProcessQueueFullError)
    gate.resolve()
    await Promise.all([first, second])
  })
})

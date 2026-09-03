import { describe, expect, it } from 'vitest'
import { createLatestRequestGuard } from './latestRequest'

function deferred<T>(): { promise: Promise<T>; resolve: (value: T) => void } {
  let resolve!: (value: T) => void
  const promise = new Promise<T>((done) => (resolve = done))
  return { promise, resolve }
}

describe('createLatestRequestGuard', () => {
  it('ignores an older server response that resolves after the current one', async () => {
    const guard = createLatestRequestGuard()
    const serverA = deferred<string[]>()
    const serverB = deferred<string[]>()
    let visible: string[] = []

    const load = async (scope: string, request: Promise<string[]>): Promise<void> => {
      const token = guard.begin(scope)
      const response = await request
      if (guard.isCurrent(token, scope)) visible = response
    }

    const requestA = load('https://a.example.com', serverA.promise)
    const requestB = load('https://b.example.com', serverB.promise)
    serverB.resolve(['B'])
    await requestB
    serverA.resolve(['A'])
    await requestA

    expect(visible).toEqual(['B'])
  })

  it('invalidates a pending request when the selected scope changes', () => {
    const guard = createLatestRequestGuard()
    const token = guard.begin('https://a.example.com')

    guard.invalidate()

    expect(guard.isCurrent(token, 'https://a.example.com')).toBe(false)
    expect(guard.isLatest(token)).toBe(false)
  })

  it('keeps sequence ownership separate from a changed scope for safe cleanup', () => {
    const guard = createLatestRequestGuard()
    const token = guard.begin('https://a.example.com')

    expect(guard.isCurrent(token, 'https://b.example.com')).toBe(false)
    expect(guard.isLatest(token)).toBe(true)
  })
})

import { describe, expect, it } from 'vitest'
import { buildFlatLayout, createLeaf, type SplitNode } from './splitTree'

const leaf = (id: string): SplitNode =>
  createLeaf({
    id,
    sessionId: `session-${id}`,
    toolId: 'shell',
    toolName: 'Shell',
    isRunning: true,
    exitCode: null,
    title: null,
  })

describe('buildFlatLayout', () => {
  it("gives each divider the size of its own split's region", () => {
    // vsplit(A, vsplit(B, C)): the inner split only spans the right half of the container.
    const root: SplitNode = {
      type: 'vsplit',
      id: 'outer',
      ratio: 0.5,
      first: leaf('a'),
      second: { type: 'vsplit', id: 'inner', ratio: 0.5, first: leaf('b'), second: leaf('c') },
    }

    const { dividers } = buildFlatLayout(root, 1000, 500)
    const extent = Object.fromEntries(dividers.map((d) => [d.splitId, d.extent]))

    expect(extent.outer).toBe(1)
    expect(extent.inner).toBeCloseTo(0.498, 3)
  })
})

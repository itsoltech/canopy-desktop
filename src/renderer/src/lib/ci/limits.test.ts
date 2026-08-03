import { describe, expect, it } from 'vitest'
// The renderer cannot import main modules at RUNTIME (separate processes), but the
// test runner loads both in one node project — which is exactly what makes this
// drift guard possible: config.ts is pure (no Electron imports).
import { CI_MAX_BUILD_TYPES as mainCap } from '../../../../main/ci/config'
import { CI_MAX_BUILD_TYPES as rendererCap } from './limits'

describe('CI limits mirror', () => {
  it('matches the main-process cap — the renderer gates Save and prints it', () => {
    expect(rendererCap).toBe(mainCap)
  })
})

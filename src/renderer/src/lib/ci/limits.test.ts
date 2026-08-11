import { describe, expect, it } from 'vitest'
// The renderer cannot import main modules at RUNTIME (separate processes), but the
// test runner loads both in one node project — which is exactly what makes this
// drift guard possible: config.ts is pure (no Electron imports).
import {
  CI_MAX_BUILD_TYPES as mainBuildTypeCap,
  CI_MAX_WORKFLOWS as mainWorkflowCap,
} from '../../../../main/ci/config'
import {
  CI_MAX_BUILD_TYPES as rendererBuildTypeCap,
  CI_MAX_WORKFLOWS as rendererWorkflowCap,
  ciWorkflowSelectionOverflow,
} from './limits'

describe('CI limits mirror', () => {
  it('matches the main-process cap — the renderer gates Save and prints it', () => {
    expect(rendererBuildTypeCap).toBe(mainBuildTypeCap)
    expect(rendererWorkflowCap).toBe(mainWorkflowCap)
  })

  it('blocks only selections beyond the workflow cap', () => {
    expect(ciWorkflowSelectionOverflow(49)).toBe(0)
    expect(ciWorkflowSelectionOverflow(50)).toBe(0)
    expect(ciWorkflowSelectionOverflow(51)).toBe(1)
  })
})

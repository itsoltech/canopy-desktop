import { describe, expect, it } from 'vitest'
import { tooltipPosition } from './tooltipPosition'

describe('tooltipPosition', () => {
  it('places the tooltip below the trigger when it fits in the viewport', () => {
    expect(
      tooltipPosition(
        { left: 100, top: 40, width: 40, height: 20 },
        { width: 80, height: 30 },
        { width: 400, height: 300 },
      ),
    ).toEqual({ left: 80, top: 64 })
  })

  it('flips the tooltip above a trigger near the bottom edge', () => {
    expect(
      tooltipPosition(
        { left: 100, top: 260, width: 40, height: 20 },
        { width: 120, height: 50 },
        { width: 400, height: 300 },
      ),
    ).toEqual({ left: 60, top: 206 })
  })

  it('clamps the tooltip to the viewport when neither side fits', () => {
    expect(
      tooltipPosition(
        { left: 0, top: 10, width: 20, height: 20 },
        { width: 160, height: 290 },
        { width: 160, height: 300 },
      ),
    ).toEqual({ left: 4, top: 4 })
  })
})

import { describe, expect, it } from 'vitest'
import { LABEL_FONT_PX, MIN_LABEL_SCREEN_PX, labelScale } from './rules'

describe('label size constants', () => {
  it("are today's 18px font and a 16px on-screen minimum", () => {
    expect(LABEL_FONT_PX).toBe(18)
    expect(MIN_LABEL_SCREEN_PX).toBe(16)
  })
})

describe('labelScale', () => {
  it('is exactly 1 at the threshold zoom', () => {
    expect(labelScale(MIN_LABEL_SCREEN_PX / LABEL_FONT_PX)).toBe(1)
  })

  it.each([0.9, 1, 2.5])('is 1 above the threshold (zoom %f)', (zoom) => {
    expect(labelScale(zoom)).toBe(1)
  })

  it.each([0.1, 0.25, 0.5, 0.8])(
    'below the threshold makes the on-screen size exactly the minimum (zoom %f)',
    (zoom) => {
      const scale = labelScale(zoom)
      expect(scale).toBeGreaterThan(1)
      expect(scale).toBeCloseTo(16 / (18 * zoom), 10)
      expect(18 * zoom * scale).toBeCloseTo(16, 10)
    },
  )
})

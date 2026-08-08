import { describe, expect, it } from 'vitest'
import { resolveXAxisLabelOpacity } from './x-axis'

describe('resolveXAxisLabelOpacity', () => {
  it('keeps labels fully visible when fadeOnHover is disabled', () => {
    expect(
      resolveXAxisLabelOpacity({
        crosshairX: 100,
        fadeOnHover: false,
        hoveredLabel: '14:00',
        isHovering: true,
        label: '14:00',
        tickerHalfWidth: 50,
        x: 100
      })
    ).toBe(1)
  })

  it('hides labels under the date-pill width while hovering', () => {
    expect(
      resolveXAxisLabelOpacity({
        crosshairX: 100,
        fadeOnHover: true,
        hoveredLabel: '14:32',
        isHovering: true,
        label: '14:00',
        tickerHalfWidth: 50,
        x: 120
      })
    ).toBe(0)
  })

  it('hides a tick that matches the hovered label even when farther away', () => {
    expect(
      resolveXAxisLabelOpacity({
        crosshairX: 100,
        fadeOnHover: true,
        hoveredLabel: '14:00',
        isHovering: true,
        label: '14:00',
        tickerHalfWidth: 50,
        x: 300
      })
    ).toBe(0)
  })

  it('partially fades labels in the soft edge around the pill', () => {
    expect(
      resolveXAxisLabelOpacity({
        crosshairX: 100,
        fadeOnHover: true,
        hoveredLabel: '14:32',
        isHovering: true,
        label: '15:00',
        tickerHalfWidth: 50,
        x: 160
      })
    ).toBe(0.5)
  })
})

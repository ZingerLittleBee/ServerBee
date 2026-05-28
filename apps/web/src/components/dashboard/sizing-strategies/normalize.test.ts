import type { LayoutItem } from 'react-grid-layout'
import { describe, expect, it } from 'vitest'
import type { SizingStrategy } from '@/lib/widget-types'
import { normalizeRenderItem, pixelSquareFineH } from './normalize'

const W_AT_1004 = 1004 // a representative container width

describe('pixelSquareFineH', () => {
  it('returns wCoarse * SCALE when containerWidth is non-positive', () => {
    expect(pixelSquareFineH(2, 0)).toBe(8)
    expect(pixelSquareFineH(3, -100)).toBe(12)
  })

  it('returns pixel-square fine h at typical container width', () => {
    // At 1004px: colStepPx = (1004+16)/12 = 85; fineRowStepPx = 8+16 = 24
    // pixelSquareFineH(2) = round(2 * 85 / 24) = round(7.083) = 7
    expect(pixelSquareFineH(2, W_AT_1004)).toBe(7)
    // pixelSquareFineH(3) = round(3 * 85 / 24) = round(10.625) = 11
    expect(pixelSquareFineH(3, W_AT_1004)).toBe(11)
    // pixelSquareFineH(6) = round(6 * 85 / 24) = round(21.25) = 21
    expect(pixelSquareFineH(6, W_AT_1004)).toBe(21)
  })

  it('never returns less than 1', () => {
    expect(pixelSquareFineH(0, W_AT_1004)).toBe(1)
  })
})

describe('normalizeRenderItem', () => {
  const baseItem: LayoutItem = {
    i: 'widget-1',
    x: 0,
    y: 0,
    w: 2,
    h: 8, // fine units
    minW: 2,
    minH: 8,
    maxW: 6,
    maxH: 24
  }

  it('returns aspect-square h derived from w', () => {
    const strategy: SizingStrategy = { kind: 'aspect-square', tiers: [2, 3, 4, 5, 6] }
    const result = normalizeRenderItem(baseItem, strategy, { containerWidth: W_AT_1004 })
    expect(result.h).toBe(7) // pixelSquareFineH(2, 1004)
    expect(result.minH).toBe(7) // pixelSquareFineH(minW=2, 1004)
    expect(result.maxH).toBe(21) // pixelSquareFineH(maxW=6, 1004)
  })

  it('aspect-square leaves w/x/y/minW/maxW unchanged', () => {
    const strategy: SizingStrategy = { kind: 'aspect-square', tiers: [2, 3, 4, 5, 6] }
    const result = normalizeRenderItem(baseItem, strategy, { containerWidth: W_AT_1004 })
    expect(result.w).toBe(2)
    expect(result.x).toBe(0)
    expect(result.minW).toBe(2)
    expect(result.maxW).toBe(6)
  })

  it('content-height uses measured fine h when present', () => {
    const strategy: SizingStrategy = { kind: 'content-height' }
    const result = normalizeRenderItem(baseItem, strategy, {
      containerWidth: W_AT_1004,
      autoMeasuredFineH: 11
    })
    expect(result.h).toBe(11)
    expect(result.minH).toBe(11)
    expect(result.maxH).toBe(11)
  })

  it('content-height returns item unchanged when measurement is missing', () => {
    const strategy: SizingStrategy = { kind: 'content-height' }
    const result = normalizeRenderItem(baseItem, strategy, { containerWidth: W_AT_1004 })
    expect(result.h).toBe(8) // unchanged
    expect(result.minH).toBe(8)
    expect(result.maxH).toBe(24)
  })

  it('free returns item unchanged', () => {
    const strategy: SizingStrategy = { kind: 'free' }
    const result = normalizeRenderItem(baseItem, strategy, { containerWidth: W_AT_1004 })
    expect(result.h).toBe(8)
    expect(result.minH).toBe(8)
    expect(result.maxH).toBe(24)
  })

  it('fixed returns item unchanged', () => {
    const strategy: SizingStrategy = { kind: 'fixed' }
    const result = normalizeRenderItem(baseItem, strategy, { containerWidth: W_AT_1004 })
    expect(result.h).toBe(8)
    expect(result.minH).toBe(8)
    expect(result.maxH).toBe(24)
  })

  it('aspect-square handles undefined maxW gracefully', () => {
    const strategy: SizingStrategy = { kind: 'aspect-square', tiers: [2, 3, 4, 5, 6] }
    const itemNoMaxW: LayoutItem = { ...baseItem, maxW: undefined, maxH: undefined }
    const result = normalizeRenderItem(itemNoMaxW, strategy, { containerWidth: W_AT_1004 })
    expect(result.maxH).toBeUndefined()
  })
})

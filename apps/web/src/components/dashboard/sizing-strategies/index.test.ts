import type { LayoutItem } from 'react-grid-layout'
import { describe, expect, it } from 'vitest'
import { applyCoarsePatch, nearestTier, type SnapPatch } from './index'

describe('nearestTier', () => {
  const tiers = [2, 3, 4, 5, 6] as const

  it('returns the exact tier when value matches', () => {
    expect(nearestTier(3, tiers)).toBe(3)
  })

  it('rounds down when between two tiers, closer to lower', () => {
    expect(nearestTier(3.4, tiers)).toBe(3)
  })

  it('rounds up when between two tiers, closer to upper', () => {
    expect(nearestTier(3.6, tiers)).toBe(4)
  })

  it('picks the first tier on exact ties (conservative)', () => {
    expect(nearestTier(3.5, [3, 4])).toBe(3)
    expect(nearestTier(4.5, tiers)).toBe(4)
  })

  it('clamps to minimum tier for values below the range', () => {
    expect(nearestTier(0, tiers)).toBe(2)
    expect(nearestTier(-5, tiers)).toBe(2)
  })

  it('clamps to maximum tier for values above the range', () => {
    expect(nearestTier(100, tiers)).toBe(6)
  })
})

describe('applyCoarsePatch', () => {
  const baseItem: LayoutItem = {
    i: 'gauge-1',
    x: 2,
    y: 0,
    w: 2,
    h: 8 // fine units (= 2 coarse)
  }

  it('returns item unchanged when patch is empty', () => {
    const result = applyCoarsePatch(baseItem, {})
    expect(result.w).toBe(2)
    expect(result.h).toBe(8)
  })

  it('applies coarse w directly (w lives in coarse units throughout)', () => {
    const patch: SnapPatch = { w: 3 }
    const result = applyCoarsePatch(baseItem, patch)
    expect(result.w).toBe(3)
    expect(result.h).toBe(8) // unchanged
  })

  it('converts coarse h to fine by multiplying by SCALE (4)', () => {
    const patch: SnapPatch = { h: 3 }
    const result = applyCoarsePatch(baseItem, patch)
    expect(result.w).toBe(2)
    expect(result.h).toBe(12) // 3 * 4 = 12 fine
  })

  it('applies both w and h', () => {
    const patch: SnapPatch = { w: 4, h: 4 }
    const result = applyCoarsePatch(baseItem, patch)
    expect(result.w).toBe(4)
    expect(result.h).toBe(16)
  })

  it('does not mutate the input item', () => {
    const patch: SnapPatch = { w: 5, h: 5 }
    applyCoarsePatch(baseItem, patch)
    expect(baseItem.w).toBe(2)
    expect(baseItem.h).toBe(8)
  })
})

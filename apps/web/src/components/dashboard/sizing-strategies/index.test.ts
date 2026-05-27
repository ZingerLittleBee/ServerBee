import type { LayoutItem } from 'react-grid-layout'
import { describe, expect, it } from 'vitest'
import type { SizingStrategy } from '@/lib/widget-types'
import { applyCoarsePatch, applyStrategy, nearestTier, type SnapPatch } from './index'

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

describe('applyStrategy', () => {
  it('free returns no constraints and undefined resize handles', () => {
    const strategy: SizingStrategy = { kind: 'free' }
    const desc = applyStrategy(strategy)
    expect(desc.constraints).toEqual([])
    expect(desc.resizeHandles).toBeUndefined()
    expect(desc.isResizable).toBe(true)
  })

  it('fixed returns no constraints, empty resize handles, isResizable false', () => {
    const strategy: SizingStrategy = { kind: 'fixed' }
    const desc = applyStrategy(strategy)
    expect(desc.constraints).toEqual([])
    expect(desc.resizeHandles).toEqual([])
    expect(desc.isResizable).toBe(false)
  })

  it('aspect-square returns aspectRatio(1) constraint and SE handle', () => {
    const strategy: SizingStrategy = { kind: 'aspect-square', tiers: [2, 3, 4, 5, 6] }
    const desc = applyStrategy(strategy)
    expect(desc.constraints).toHaveLength(1)
    expect(desc.constraints[0].name).toBe('aspectRatio(1)')
    expect(desc.resizeHandles).toEqual(['se'])
    expect(desc.isResizable).toBe(true)
  })

  it('content-height returns lockHeight constraint when measured', () => {
    const strategy: SizingStrategy = { kind: 'content-height' }
    const desc = applyStrategy(strategy, 11)
    expect(desc.constraints).toHaveLength(1)
    expect(desc.constraints[0].name).toBe('lockHeight(11)')
    expect(desc.resizeHandles).toEqual(['e'])
    expect(desc.isResizable).toBe(true)
  })

  it('content-height returns no constraint when unmeasured', () => {
    const strategy: SizingStrategy = { kind: 'content-height' }
    const desc = applyStrategy(strategy)
    expect(desc.constraints).toEqual([])
    expect(desc.resizeHandles).toEqual(['e'])
    expect(desc.isResizable).toBe(true)
  })

  it('lockHeight constraint locks h to the measured value', () => {
    const strategy: SizingStrategy = { kind: 'content-height' }
    const desc = applyStrategy(strategy, 11)
    const constraint = desc.constraints[0]
    // Call the constraint directly — params: item, w, h, handle, context
    const result = constraint.constrainSize?.({ i: 'x', x: 0, y: 0, w: 4, h: 99 }, 4, 99, 'e', {
      cols: 12,
      containerWidth: 1000,
      maxRows: Number.POSITIVE_INFINITY,
      rowHeight: 8,
      margin: [16, 16],
      layout: []
    } as any)
    expect(result).toEqual({ w: 4, h: 11 })
  })
})

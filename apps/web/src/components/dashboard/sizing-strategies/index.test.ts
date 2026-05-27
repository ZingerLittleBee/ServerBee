import { describe, expect, it } from 'vitest'
import { nearestTier } from './index'

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

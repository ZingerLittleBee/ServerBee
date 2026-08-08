import { describe, expect, it } from 'vitest'
import {
  getUtilizationBarColor,
  getUtilizationRingColor,
  getUtilizationSeverity,
  getUtilizationTextColor,
  UTILIZATION_HIGH_THRESHOLD,
  UTILIZATION_VERY_HIGH_THRESHOLD
} from './utilization-colors'

describe('getUtilizationSeverity', () => {
  it('maps low utilization to healthy', () => {
    expect(getUtilizationSeverity(0)).toBe('healthy')
    expect(getUtilizationSeverity(UTILIZATION_HIGH_THRESHOLD)).toBe('healthy')
  })

  it('maps high utilization to warning', () => {
    expect(getUtilizationSeverity(UTILIZATION_HIGH_THRESHOLD + 0.1)).toBe('warning')
    expect(getUtilizationSeverity(UTILIZATION_VERY_HIGH_THRESHOLD)).toBe('warning')
  })

  it('maps very high utilization to danger', () => {
    expect(getUtilizationSeverity(UTILIZATION_VERY_HIGH_THRESHOLD + 0.1)).toBe('danger')
    expect(getUtilizationSeverity(100)).toBe('danger')
  })
})

describe('utilization color tokens', () => {
  it('uses status tokens for ring strokes', () => {
    expect(getUtilizationRingColor(10)).toBe('var(--status-healthy)')
    expect(getUtilizationRingColor(75)).toBe('var(--status-warning)')
    expect(getUtilizationRingColor(95)).toBe('var(--status-danger)')
  })

  it('uses status utilities for bars and text', () => {
    expect(getUtilizationBarColor(10)).toBe('bg-status-healthy')
    expect(getUtilizationBarColor(75)).toBe('bg-status-warning')
    expect(getUtilizationBarColor(95)).toBe('bg-status-danger')

    expect(getUtilizationTextColor(10)).toBe('text-foreground')
    expect(getUtilizationTextColor(75)).toBe('text-status-warning-text')
    expect(getUtilizationTextColor(95)).toBe('text-status-danger-text')
  })
})

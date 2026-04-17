import { describe, expect, it } from 'vitest'
import {
  getCombinedBarColor,
  getCombinedSeverity,
  getLatencyBarColor,
  getLatencyStatus,
  getLatencyTextClass,
  getLossDotBgClass,
  isLatencyFailure,
  LATENCY_HEALTHY_THRESHOLD_MS
} from './network-latency-constants'

describe('network-latency-constants', () => {
  it('treats latency below 300ms as healthy', () => {
    expect(getLatencyStatus({ latencyMs: LATENCY_HEALTHY_THRESHOLD_MS - 1 })).toBe('healthy')
    expect(getLatencyTextClass({ latencyMs: LATENCY_HEALTHY_THRESHOLD_MS - 1 })).toContain('text-emerald-600')
    expect(getLatencyBarColor({ latencyMs: LATENCY_HEALTHY_THRESHOLD_MS - 1 })).toBe('#10b981')
  })

  it('treats latency at or above 300ms as warning', () => {
    expect(getLatencyStatus({ latencyMs: LATENCY_HEALTHY_THRESHOLD_MS })).toBe('warning')
    expect(getLatencyTextClass({ latencyMs: LATENCY_HEALTHY_THRESHOLD_MS })).toContain('text-amber-600')
    expect(getLatencyBarColor({ latencyMs: LATENCY_HEALTHY_THRESHOLD_MS })).toBe('#f59e0b')
  })

  it('treats explicit failure as failed even without latency', () => {
    expect(isLatencyFailure(1)).toBe(true)
    expect(getLatencyStatus({ latencyMs: null, failed: true })).toBe('failed')
    expect(getLatencyTextClass({ latencyMs: null, failed: true })).toContain('text-red-600')
    expect(getLatencyBarColor({ latencyMs: null, failed: true })).toBe('#ef4444')
  })

  it('keeps missing data muted when there is no failure signal', () => {
    expect(isLatencyFailure(null)).toBe(false)
    expect(getLatencyStatus({ latencyMs: null })).toBe('unknown')
    expect(getLatencyTextClass({ latencyMs: null })).toBe('text-muted-foreground')
    expect(getLatencyBarColor({ latencyMs: null })).toBe('var(--color-muted)')
  })

  describe('getCombinedSeverity', () => {
    it('returns healthy when latency < 300 and loss < 1%', () => {
      expect(getCombinedSeverity({ latencyMs: 50, lossRatio: 0 })).toBe('healthy')
      expect(getCombinedSeverity({ latencyMs: 299, lossRatio: 0.009 })).toBe('healthy')
    })

    it('returns warning when latency >= 300 or loss in [1%, 5%)', () => {
      expect(getCombinedSeverity({ latencyMs: 300, lossRatio: 0 })).toBe('warning')
      expect(getCombinedSeverity({ latencyMs: 50, lossRatio: 0.01 })).toBe('warning')
      expect(getCombinedSeverity({ latencyMs: 50, lossRatio: 0.049 })).toBe('warning')
    })

    it('returns severe when loss >= 5% but not total failure', () => {
      expect(getCombinedSeverity({ latencyMs: 50, lossRatio: 0.05 })).toBe('severe')
      expect(getCombinedSeverity({ latencyMs: 500, lossRatio: 0.5 })).toBe('severe')
    })

    it('returns failed when loss ratio hits 100%', () => {
      expect(getCombinedSeverity({ latencyMs: null, lossRatio: 1 })).toBe('failed')
      expect(getCombinedSeverity({ latencyMs: 0, lossRatio: 1 })).toBe('failed')
    })

    it('returns unknown when both inputs are null', () => {
      expect(getCombinedSeverity({ latencyMs: null, lossRatio: null })).toBe('unknown')
    })

    it('tolerates one null input', () => {
      expect(getCombinedSeverity({ latencyMs: null, lossRatio: 0 })).toBe('healthy')
      expect(getCombinedSeverity({ latencyMs: 50, lossRatio: null })).toBe('healthy')
      expect(getCombinedSeverity({ latencyMs: 400, lossRatio: null })).toBe('warning')
      expect(getCombinedSeverity({ latencyMs: null, lossRatio: 0.1 })).toBe('severe')
    })
  })

  describe('getCombinedBarColor', () => {
    it('maps severity levels to expected hex colors', () => {
      expect(getCombinedBarColor({ latencyMs: 50, lossRatio: 0 })).toBe('#10b981')
      expect(getCombinedBarColor({ latencyMs: 400, lossRatio: 0 })).toBe('#f59e0b')
      expect(getCombinedBarColor({ latencyMs: 50, lossRatio: 0.08 })).toBe('#ef4444')
      expect(getCombinedBarColor({ latencyMs: null, lossRatio: 1 })).toBe('#ef4444')
      expect(getCombinedBarColor({ latencyMs: null, lossRatio: null })).toBe('var(--color-muted)')
    })
  })

  describe('getLossDotBgClass', () => {
    it('maps loss ratio to Tailwind bg class', () => {
      expect(getLossDotBgClass(null)).toBe('bg-muted-foreground')
      expect(getLossDotBgClass(0)).toBe('bg-emerald-500')
      expect(getLossDotBgClass(0.009)).toBe('bg-emerald-500')
      expect(getLossDotBgClass(0.01)).toBe('bg-amber-500')
      expect(getLossDotBgClass(0.049)).toBe('bg-amber-500')
      expect(getLossDotBgClass(0.05)).toBe('bg-red-500')
      expect(getLossDotBgClass(1)).toBe('bg-red-500')
    })
  })
})

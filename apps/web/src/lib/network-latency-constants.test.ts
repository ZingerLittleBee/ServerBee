import { describe, expect, it } from 'vitest'
import {
  getLatencyBarColor,
  getLatencyStatus,
  getLatencyTextClass,
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
})

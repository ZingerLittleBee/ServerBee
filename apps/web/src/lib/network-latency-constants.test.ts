import { describe, expect, it } from 'vitest'
import {
  getCombinedSeverity,
  getLatencyStatus,
  getLatencyTextClass,
  getLossDotBgClass,
  getLossSeverity,
  getSeveritySquareColor,
  isLatencyFailure
} from './network-latency-constants'

describe('network-latency-constants', () => {
  it('treats latency below 300ms as healthy', () => {
    expect(getLatencyStatus({ latencyMs: 299 })).toBe('healthy')
    expect(getLatencyTextClass({ latencyMs: 299 })).toBe('text-status-healthy-text')
  })

  it('treats latency at or above 300ms as warning', () => {
    expect(getLatencyStatus({ latencyMs: 300 })).toBe('warning')
    expect(getLatencyTextClass({ latencyMs: 300 })).toBe('text-status-warning-text')
  })

  it('treats explicit failure as failed even without latency', () => {
    expect(isLatencyFailure(1)).toBe(true)
    expect(getLatencyStatus({ latencyMs: null, failed: true })).toBe('failed')
    expect(getLatencyTextClass({ latencyMs: null, failed: true })).toBe('text-status-danger-text')
  })

  it('keeps missing data muted when there is no failure signal', () => {
    expect(isLatencyFailure(null)).toBe(false)
    expect(getLatencyStatus({ latencyMs: null })).toBe('unknown')
    expect(getLatencyTextClass({ latencyMs: null })).toBe('text-muted-foreground')
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

  describe('getLossDotBgClass', () => {
    it('maps loss ratio to Tailwind bg class', () => {
      expect(getLossDotBgClass(null)).toBe('bg-muted-foreground')
      expect(getLossDotBgClass(0)).toBe('bg-status-healthy')
      expect(getLossDotBgClass(0.009)).toBe('bg-status-healthy')
      expect(getLossDotBgClass(0.01)).toBe('bg-status-warning')
      expect(getLossDotBgClass(0.049)).toBe('bg-status-warning')
      expect(getLossDotBgClass(0.05)).toBe('bg-status-danger')
      expect(getLossDotBgClass(1)).toBe('bg-status-danger')
    })
  })

  describe('getLossSeverity', () => {
    it('returns unknown for missing loss', () => {
      expect(getLossSeverity(null)).toBe('unknown')
      expect(getLossSeverity(undefined)).toBe('unknown')
    })

    it('returns healthy below the warning threshold', () => {
      expect(getLossSeverity(0)).toBe('healthy')
      expect(getLossSeverity(0.005)).toBe('healthy')
      expect(getLossSeverity(0.009)).toBe('healthy')
    })

    it('returns warning between warning and severe thresholds', () => {
      expect(getLossSeverity(0.01)).toBe('warning')
      expect(getLossSeverity(0.049)).toBe('warning')
    })

    it('returns severe above the severe threshold but below total failure', () => {
      expect(getLossSeverity(0.05)).toBe('severe')
      expect(getLossSeverity(0.99)).toBe('severe')
    })

    it('returns failed at total packet loss', () => {
      expect(getLossSeverity(1)).toBe('failed')
    })
  })

  describe('getSeveritySquareColor', () => {
    it('maps severity levels to the square-grid status colors', () => {
      expect(getSeveritySquareColor('healthy')).toBe('var(--network-grid-healthy)')
      expect(getSeveritySquareColor('warning')).toBe('var(--network-grid-warning)')
      expect(getSeveritySquareColor('severe')).toBe('var(--network-grid-severe)')
      expect(getSeveritySquareColor('failed')).toBe('var(--network-grid-failed)')
      expect(getSeveritySquareColor('unknown')).toBe('var(--network-grid-unknown)')
    })

    it('keeps severe and failed visually distinct', () => {
      expect(getSeveritySquareColor('severe')).not.toBe(getSeveritySquareColor('failed'))
    })
  })
})

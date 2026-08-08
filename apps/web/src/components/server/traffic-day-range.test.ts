import { describe, expect, it } from 'vitest'
import { formatUtcDateYmd, inclusiveUtcDateWindow } from './traffic-day-range'

const POSITIVE_INTEGER_ERROR = /positive integer/

/** Inclusive day count between two `YYYY-MM-DD` UTC dates. */
function inclusiveDayCount(from: string, to: string): number {
  const start = Date.parse(`${from}T00:00:00.000Z`)
  const end = Date.parse(`${to}T00:00:00.000Z`)
  return Math.round((end - start) / (24 * 60 * 60 * 1000)) + 1
}

describe('inclusiveUtcDateWindow', () => {
  it('returns exactly N inclusive UTC dates for 7/30/90 day windows', () => {
    // Fixed anchor: 2026-03-31 UTC (afternoon so local-midnight bugs cannot hide).
    const end = new Date(Date.UTC(2026, 2, 31, 15, 30, 0))

    expect(inclusiveUtcDateWindow(7, end)).toEqual({ from: '2026-03-25', to: '2026-03-31' })
    expect(inclusiveDayCount('2026-03-25', '2026-03-31')).toBe(7)

    // 30 days ending Mar 31 starts Mar 2 — not Mar 1 (that would be 31 days).
    expect(inclusiveUtcDateWindow(30, end)).toEqual({ from: '2026-03-02', to: '2026-03-31' })
    expect(inclusiveDayCount('2026-03-02', '2026-03-31')).toBe(30)
    expect(inclusiveUtcDateWindow(30, end).from).not.toBe('2026-03-01')

    // 2026 is not a leap year: Jan 1–Mar 31 is exactly 90 inclusive days.
    expect(inclusiveUtcDateWindow(90, end)).toEqual({ from: '2026-01-01', to: '2026-03-31' })
    expect(inclusiveDayCount('2026-01-01', '2026-03-31')).toBe(90)
  })

  it('keeps the UTC calendar day under timezone-sensitive wall-clock times', () => {
    // 2026-03-31 02:00 UTC is still Mar 30 evening in US Pacific — local setDate +
    // toISOString can report Mar 30; UTC fields must stay on Mar 31.
    const earlyUtc = new Date('2026-03-31T02:00:00.000Z')
    expect(formatUtcDateYmd(earlyUtc)).toBe('2026-03-31')

    const window = inclusiveUtcDateWindow(30, earlyUtc)
    expect(window.to).toBe('2026-03-31')
    expect(window.from).toBe('2026-03-02')
    expect(inclusiveDayCount(window.from, window.to)).toBe(30)

    // Single-day window is just the end date.
    expect(inclusiveUtcDateWindow(1, earlyUtc)).toEqual({ from: '2026-03-31', to: '2026-03-31' })
  })

  it('rejects non-positive day counts', () => {
    expect(() => inclusiveUtcDateWindow(0)).toThrow(POSITIVE_INTEGER_ERROR)
    expect(() => inclusiveUtcDateWindow(-5)).toThrow(POSITIVE_INTEGER_ERROR)
  })
})

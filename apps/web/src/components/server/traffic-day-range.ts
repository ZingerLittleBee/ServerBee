const MS_PER_DAY = 24 * 60 * 60 * 1000

/** Format a Date as `YYYY-MM-DD` using UTC calendar fields only. */
export function formatUtcDateYmd(date: Date): string {
  const year = date.getUTCFullYear()
  const month = String(date.getUTCMonth() + 1).padStart(2, '0')
  const day = String(date.getUTCDate()).padStart(2, '0')
  return `${year}-${month}-${day}`
}

/**
 * Inclusive UTC calendar window of exactly `dayCount` dates ending on `end`.
 *
 * For dayCount=30 and end=2026-03-31 this yields from=2026-03-02, to=2026-03-31
 * (30 dates, not 31). Uses UTC fields so local timezone / DST never shifts the
 * calendar day the way `setDate` + `toISOString` can.
 */
export function inclusiveUtcDateWindow(dayCount: number, end: Date = new Date()): { from: string; to: string } {
  if (!Number.isInteger(dayCount) || dayCount < 1) {
    throw new Error(`dayCount must be a positive integer, got ${String(dayCount)}`)
  }

  const endUtcMs = Date.UTC(end.getUTCFullYear(), end.getUTCMonth(), end.getUTCDate())
  const fromUtcMs = endUtcMs - (dayCount - 1) * MS_PER_DAY

  return {
    from: formatUtcDateYmd(new Date(fromUtcMs)),
    to: formatUtcDateYmd(new Date(endUtcMs))
  }
}

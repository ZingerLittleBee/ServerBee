/**
 * Shared utilization severity colors for meters, ring charts, and percent labels.
 *
 * Color language (one meaning per hue, via the global --status-* tokens):
 * - low (≤70%): healthy / emerald
 * - high (70%–90%]: warning / amber
 * - very high (>90%): danger / red
 *
 * Thresholds match the server list metric bars so the dashboard cards and
 * table view speak the same language.
 */

export type UtilizationSeverity = 'healthy' | 'warning' | 'danger'

/** Exclusive lower bounds: pct > high → warning, pct > veryHigh → danger. */
export const UTILIZATION_HIGH_THRESHOLD = 70
export const UTILIZATION_VERY_HIGH_THRESHOLD = 90

export function getUtilizationSeverity(pct: number): UtilizationSeverity {
  if (pct > UTILIZATION_VERY_HIGH_THRESHOLD) {
    return 'danger'
  }
  if (pct > UTILIZATION_HIGH_THRESHOLD) {
    return 'warning'
  }
  return 'healthy'
}

/** CSS color for SVG strokes / inline styles (ring charts). */
export function getUtilizationRingColor(pct: number): string {
  switch (getUtilizationSeverity(pct)) {
    case 'danger':
      return 'var(--status-danger)'
    case 'warning':
      return 'var(--status-warning)'
    default:
      return 'var(--status-healthy)'
  }
}

/** Tailwind background utility for progress bars. */
export function getUtilizationBarColor(pct: number): string {
  switch (getUtilizationSeverity(pct)) {
    case 'danger':
      return 'bg-status-danger'
    case 'warning':
      return 'bg-status-warning'
    default:
      return 'bg-status-healthy'
  }
}

/** Tailwind text utility for percent labels on light/dark surfaces. */
export function getUtilizationTextColor(pct: number): string {
  switch (getUtilizationSeverity(pct)) {
    case 'danger':
      return 'text-status-danger-text'
    case 'warning':
      return 'text-status-warning-text'
    default:
      return 'text-foreground'
  }
}

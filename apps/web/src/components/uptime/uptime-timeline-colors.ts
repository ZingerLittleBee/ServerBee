import type { UptimeColor } from '@/lib/widget-helpers'

/** Status fills shared by the timeline bars and its legend swatches. */
export const SEGMENT_COLOR_VALUE_MAP: Record<UptimeColor, string> = {
  green: 'var(--uptime-operational)',
  yellow: 'var(--uptime-degraded)',
  red: 'var(--uptime-down)',
  gray: 'var(--color-muted)'
}

/** Higher-separation palette shared with the status-history markers. */
export const STATUS_HISTORY_COLOR_VALUE_MAP: Record<UptimeColor, string> = {
  green: 'var(--network-grid-healthy)',
  yellow: 'var(--network-grid-warning)',
  red: 'var(--network-grid-failed)',
  gray: 'var(--network-grid-unknown)'
}

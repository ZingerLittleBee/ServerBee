import type { UptimeColor } from '@/lib/widget-helpers'

/** Status fills shared by the timeline bars and its legend swatches. */
export const SEGMENT_COLOR_VALUE_MAP: Record<UptimeColor, string> = {
  green: 'var(--uptime-operational)',
  yellow: 'var(--uptime-degraded)',
  red: 'var(--uptime-down)',
  gray: 'var(--color-muted)'
}

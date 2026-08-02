/**
 * High-contrast 12-color palette for multi-series charts (e.g. LatencyChart).
 * Used by both LatencyChart (ChartConfig) and TargetCard (color dots).
 *
 * The values resolve through the `--chart-series-*` tokens declared in
 * `index.css`, so the palette lives with the rest of the design tokens.
 * Every consumer feeds these strings into a place where `var()` resolves:
 * inline `style={{ backgroundColor }}` on the legend dots, and shadcn's
 * ChartStyle which emits them as `--color-<key>` custom properties that the
 * Recharts `stroke`/`fill` attributes then reference.
 */
export const CHART_COLORS = [
  'var(--chart-series-1)',
  'var(--chart-series-2)',
  'var(--chart-series-3)',
  'var(--chart-series-4)',
  'var(--chart-series-5)',
  'var(--chart-series-6)',
  'var(--chart-series-7)',
  'var(--chart-series-8)',
  'var(--chart-series-9)',
  'var(--chart-series-10)',
  'var(--chart-series-11)',
  'var(--chart-series-12)'
] as const

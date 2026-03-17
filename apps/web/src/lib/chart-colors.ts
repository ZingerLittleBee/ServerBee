/**
 * High-contrast 12-color palette for multi-series charts (e.g. LatencyChart).
 * Matches the original COLOR_PALETTE from network/$serverId.tsx.
 * Used by both LatencyChart (ChartConfig) and TargetCard (color dots).
 */
export const CHART_COLORS = [
  '#3b82f6', // blue-500
  '#ef4444', // red-500
  '#22c55e', // green-500
  '#f59e0b', // amber-500
  '#8b5cf6', // violet-500
  '#ec4899', // pink-500
  '#14b8a6', // teal-500
  '#f97316', // orange-500
  '#6366f1', // indigo-500
  '#06b6d4', // cyan-500
  '#84cc16', // lime-500
  '#e11d48' // rose-600
] as const

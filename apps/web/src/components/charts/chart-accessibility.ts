export const MAX_ACCESSIBLE_CHART_ROWS = 50

export function sampleChartRows(data: Record<string, unknown>[]): Record<string, unknown>[] {
  if (data.length <= MAX_ACCESSIBLE_CHART_ROWS) {
    return data
  }

  const lastIndex = data.length - 1
  const indices = Array.from({ length: MAX_ACCESSIBLE_CHART_ROWS }, (_, index) =>
    Math.round((index / (MAX_ACCESSIBLE_CHART_ROWS - 1)) * lastIndex)
  )
  return [...new Set(indices)].flatMap((index) => (data[index] ? [data[index]] : []))
}

export function formatFiniteChartValue(value: unknown, formatter: (value: number) => string): string {
  return typeof value === 'number' && Number.isFinite(value) ? formatter(value) : '--'
}

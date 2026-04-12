export const SPARKLINE_LENGTH = 30

export interface SparklinePoint {
  latency: number | null
  loss: number | null
}

interface SparklineSummaryLike {
  latency_sparkline?: unknown
  loss_sparkline?: unknown
}

function normalizeSparklineSeries(series: unknown): readonly (number | null)[] {
  return Array.isArray(series) ? series : []
}

export function seedFromSummary(summary: SparklineSummaryLike): SparklinePoint[] {
  const latencySparkline = normalizeSparklineSeries(summary.latency_sparkline)
  const lossSparkline = normalizeSparklineSeries(summary.loss_sparkline)

  return Array.from({ length: SPARKLINE_LENGTH }, (_, index) => ({
    latency: latencySparkline[index] ?? null,
    loss: lossSparkline[index] ?? null
  }))
}

export function toBarData(points: SparklinePoint[], pick: 'latency' | 'lossPercent'): (number | null)[] {
  return points.map((point) => {
    if (pick === 'lossPercent') {
      return point.loss == null ? null : point.loss * 100
    }

    return point.latency
  })
}

export function summaryStats(points: SparklinePoint[]): {
  avgLatency: number | null
  avgLoss: number | null
} {
  let latencyTotal = 0
  let latencyCount = 0
  let lossTotal = 0
  let lossCount = 0

  for (const point of points) {
    if (point.latency != null) {
      latencyTotal += point.latency
      latencyCount += 1
    }

    if (point.loss != null) {
      lossTotal += point.loss
      lossCount += 1
    }
  }

  return {
    avgLatency: latencyCount === 0 ? null : latencyTotal / latencyCount,
    avgLoss: lossCount === 0 ? null : lossTotal / lossCount
  }
}

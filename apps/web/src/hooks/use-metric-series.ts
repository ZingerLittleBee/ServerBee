import { useMemo } from 'react'
import type { ServerMetrics } from '@/hooks/use-servers-ws'
import type { ServerMetricRecord } from '@/lib/api-schema'
import { extractLiveMetric, extractRecordMetric } from '@/lib/widget-helpers'

export interface MetricSeriesPoint {
  t: number
  v: number
}

export interface MetricSeries {
  avg: number | null
  current: number
  oneHourDelta: number | null
  peak: number | null
  points: MetricSeriesPoint[]
}

interface Params {
  metric: string
  records: ServerMetricRecord[] | undefined
  server: ServerMetrics | undefined
}

const ONE_HOUR_MS = 60 * 60_000
const DELTA_WINDOW_MS = 5 * 60_000

function buildRecordPoints(
  records: ServerMetricRecord[] | undefined,
  metric: string,
  server: ServerMetrics | undefined
): MetricSeriesPoint[] {
  const points: MetricSeriesPoint[] = []
  if (!records) {
    return points
  }
  for (const r of records) {
    const t = new Date(r.time).getTime()
    if (Number.isFinite(t)) {
      points.push({ t, v: extractRecordMetric(r, metric, server) })
    }
  }
  points.sort((a, b) => a.t - b.t)
  return points
}

function computePeakAndAvg(points: MetricSeriesPoint[]): { avg: number; peak: number } {
  let peak = points[0].v
  let sum = 0
  for (const p of points) {
    if (p.v > peak) {
      peak = p.v
    }
    sum += p.v
  }
  return { peak, avg: sum / points.length }
}

function computeOneHourDelta(points: MetricSeriesPoint[], nowTs: number, current: number): number | null {
  const target = nowTs - ONE_HOUR_MS
  let delta: number | null = null
  let bestDist = Number.POSITIVE_INFINITY
  for (const p of points) {
    const dist = Math.abs(p.t - target)
    if (dist <= DELTA_WINDOW_MS && dist < bestDist) {
      bestDist = dist
      delta = current - p.v
    }
  }
  return delta
}

export function useMetricSeries({ records, server, metric }: Params): MetricSeries {
  return useMemo(() => {
    const points = buildRecordPoints(records, metric, server)
    const liveValue = server ? extractLiveMetric(server, metric) : 0
    const liveTick: MetricSeriesPoint = { t: Date.now(), v: liveValue }

    const last = points.at(-1)
    if (!last || liveTick.t > last.t) {
      points.push(liveTick)
    }

    if (points.length === 0) {
      return { points, current: 0, peak: null, avg: null, oneHourDelta: null }
    }

    const current = points.at(-1)?.v ?? 0
    const { peak, avg } = computePeakAndAvg(points)
    const oneHourDelta = computeOneHourDelta(points, liveTick.t, current)

    return { points, current, peak, avg, oneHourDelta }
  }, [records, server, metric])
}

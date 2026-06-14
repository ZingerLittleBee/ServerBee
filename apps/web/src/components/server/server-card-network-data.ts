import type { NetworkProbeResultData, NetworkServerSummary, NetworkTargetSummary } from '@/lib/network-types'

const MAX_TREND_POINTS = 30

export const AGGREGATE_TARGET_ID = '__aggregate__'

export interface ServerCardTooltipTarget {
  latency: number | null
  lossRatio: number
  targetId: string
  targetName: string
}

export interface ServerCardMetricPoint {
  synthetic: boolean
  targets: readonly ServerCardTooltipTarget[]
  timestamp: string
  value: number | null
}

interface ServerCardNetworkTrend {
  currentAvgLatency: number | null
  currentAvgLossRatio: number | null
  latencyPoints: ServerCardMetricPoint[]
  lossPoints: ServerCardMetricPoint[]
}

export interface ServerCardNetworkState extends ServerCardNetworkTrend {
  currentTargets: readonly ServerCardTooltipTarget[]
}

function hasTargetSample(target: NetworkTargetSummary): boolean {
  return target.avg_latency != null || target.packet_loss > 0 || target.availability > 0
}

function average(values: readonly number[]): number | null {
  if (values.length === 0) {
    return null
  }

  return values.reduce((sum, value) => sum + value, 0) / values.length
}

function buildTargetLookup(targets: readonly NetworkTargetSummary[]): Map<string, NetworkTargetSummary> {
  return new Map(targets.map((target) => [target.target_id, target]))
}

function buildTargetOrder(targets: readonly NetworkTargetSummary[]): Map<string, number> {
  return new Map(targets.map((target, index) => [target.target_id, index]))
}

function compareTargets(
  left: ServerCardTooltipTarget,
  right: ServerCardTooltipTarget,
  targetOrder: ReadonlyMap<string, number>
): number {
  const leftOrder = targetOrder.get(left.targetId) ?? Number.MAX_SAFE_INTEGER
  const rightOrder = targetOrder.get(right.targetId) ?? Number.MAX_SAFE_INTEGER

  if (leftOrder !== rightOrder) {
    return leftOrder - rightOrder
  }

  return left.targetName.localeCompare(right.targetName)
}

function buildSyntheticTimestamp(index: number): string {
  return `synthetic-${index}`
}

function buildFallbackTargets(
  targets: readonly NetworkTargetSummary[],
  targetOrder: ReadonlyMap<string, number>
): ServerCardTooltipTarget[] {
  return targets
    .filter(hasTargetSample)
    .map((target) => ({
      latency: target.avg_latency,
      lossRatio: target.packet_loss,
      targetId: target.target_id,
      targetName: target.target_name
    }))
    .sort((left, right) => compareTargets(left, right, targetOrder))
}

function padMetricPoints(points: readonly ServerCardMetricPoint[]): ServerCardMetricPoint[] {
  const trimmedPoints = points.slice(-MAX_TREND_POINTS)
  if (trimmedPoints.length === 0) {
    return []
  }

  const paddingLength = MAX_TREND_POINTS - trimmedPoints.length

  return [
    ...Array.from({ length: paddingLength }, (_, index) => ({
      synthetic: true,
      targets: [] as ServerCardTooltipTarget[],
      timestamp: `padding-${index}`,
      value: null
    })),
    ...trimmedPoints
  ]
}

function padState(state: ServerCardNetworkTrend): ServerCardNetworkTrend {
  return {
    ...state,
    latencyPoints: padMetricPoints(state.latencyPoints),
    lossPoints: padMetricPoints(state.lossPoints)
  }
}

function buildRealtimeState(
  bucketMap: ReadonlyMap<string, ServerCardTooltipTarget[]>,
  targetOrder: ReadonlyMap<string, number>
): ServerCardNetworkTrend {
  const timestamps = [...bucketMap.keys()].sort().slice(-MAX_TREND_POINTS)
  // Sort each timestamp's targets once and reuse for both latency and loss points;
  // this runs in every server card's memo on every realtime network tick.
  const sortedByTimestamp = new Map(
    timestamps.map((timestamp) => [
      timestamp,
      [...(bucketMap.get(timestamp) ?? [])].sort((left, right) => compareTargets(left, right, targetOrder))
    ])
  )
  const latencyPoints = timestamps.map((timestamp) => {
    const targets = sortedByTimestamp.get(timestamp) ?? []
    const avgLatency = average(targets.flatMap((target) => (target.latency == null ? [] : [target.latency])))

    return {
      synthetic: false,
      targets,
      timestamp,
      value: avgLatency
    }
  })
  const lossPoints = timestamps.map((timestamp) => {
    const targets = sortedByTimestamp.get(timestamp) ?? []
    const avgLossRatio = average(targets.map((target) => target.lossRatio))

    return {
      synthetic: false,
      targets,
      timestamp,
      value: avgLossRatio == null ? null : avgLossRatio * 100
    }
  })

  return {
    currentAvgLatency: latencyPoints.at(-1)?.value ?? null,
    currentAvgLossRatio: (() => {
      const lastLossPoint = lossPoints.at(-1)?.value
      return lastLossPoint == null ? null : lastLossPoint / 100
    })(),
    latencyPoints,
    lossPoints
  }
}

function buildSparklineTimestamps(
  lastProbeAt: string | null | undefined,
  bucketSeconds: number,
  pointCount: number
): (string | null)[] {
  const lastMs = lastProbeAt ? Date.parse(lastProbeAt) : Number.NaN
  if (Number.isNaN(lastMs) || bucketSeconds <= 0) {
    return Array.from({ length: pointCount }, () => null)
  }
  const bucketMs = bucketSeconds * 1000
  const latestBucketMs = Math.floor(lastMs / bucketMs) * bucketMs
  return Array.from({ length: pointCount }, (_, index) =>
    new Date(latestBucketMs - (pointCount - 1 - index) * bucketMs).toISOString()
  )
}

function buildFallbackState(
  summary: Pick<NetworkServerSummary, 'latency_sparkline' | 'loss_sparkline' | 'targets' | 'last_probe_at'> | undefined,
  fallbackTargets: readonly ServerCardTooltipTarget[],
  bucketSeconds: number
): ServerCardNetworkTrend {
  const latencySparkline = summary?.latency_sparkline ?? []
  const lossSparkline = summary?.loss_sparkline ?? []
  const pointCount = Math.max(latencySparkline.length, lossSparkline.length, fallbackTargets.length > 0 ? 1 : 0)

  if (pointCount === 0) {
    return {
      currentAvgLatency: null,
      currentAvgLossRatio: null,
      latencyPoints: [],
      lossPoints: []
    }
  }

  const startIndex = Math.max(0, pointCount - MAX_TREND_POINTS)
  const sparklineTimestamps = buildSparklineTimestamps(summary?.last_probe_at, bucketSeconds, pointCount)
  const latencyPoints: ServerCardMetricPoint[] = []
  const lossPoints: ServerCardMetricPoint[] = []

  for (let index = startIndex; index < pointCount; index += 1) {
    const timestamp = sparklineTimestamps[index] ?? buildSyntheticTimestamp(index)
    const latencyValue = latencySparkline[index] ?? null
    const lossValue = lossSparkline[index] ?? null
    let bucketTargets: readonly ServerCardTooltipTarget[]
    if (fallbackTargets.length > 0) {
      bucketTargets = fallbackTargets
    } else if (latencyValue == null && lossValue == null) {
      bucketTargets = []
    } else {
      bucketTargets = [
        {
          latency: latencyValue,
          lossRatio: lossValue ?? 0,
          targetId: AGGREGATE_TARGET_ID,
          targetName: AGGREGATE_TARGET_ID
        }
      ]
    }
    latencyPoints.push({
      synthetic: true,
      targets: bucketTargets,
      timestamp,
      value: latencyValue
    })
    lossPoints.push({
      synthetic: true,
      targets: bucketTargets,
      timestamp,
      value: lossValue == null ? null : lossValue * 100
    })
  }

  return {
    currentAvgLatency:
      latencyPoints.at(-1)?.value ??
      average(fallbackTargets.flatMap((target) => (target.latency == null ? [] : [target.latency]))),
    currentAvgLossRatio:
      lossPoints.at(-1)?.value == null
        ? average(fallbackTargets.map((target) => target.lossRatio))
        : (lossPoints.at(-1)?.value ?? 0) / 100,
    latencyPoints,
    lossPoints
  }
}

function mergeStates(
  fallbackState: ServerCardNetworkTrend,
  realtimeState: ServerCardNetworkTrend
): ServerCardNetworkTrend {
  const latencyPoints = [...fallbackState.latencyPoints, ...realtimeState.latencyPoints].slice(-MAX_TREND_POINTS)
  const lossPoints = [...fallbackState.lossPoints, ...realtimeState.lossPoints].slice(-MAX_TREND_POINTS)

  return {
    currentAvgLatency: latencyPoints.at(-1)?.value ?? null,
    currentAvgLossRatio: (() => {
      const lastLossPoint = lossPoints.at(-1)?.value
      return lastLossPoint == null ? null : lastLossPoint / 100
    })(),
    latencyPoints,
    lossPoints
  }
}

function selectCurrentTargets(
  latencyPoints: readonly ServerCardMetricPoint[],
  fallbackTargets: readonly ServerCardTooltipTarget[]
): readonly ServerCardTooltipTarget[] {
  const lastTargets = latencyPoints.at(-1)?.targets ?? []
  const perTarget = lastTargets.filter((target) => target.targetId !== AGGREGATE_TARGET_ID)
  return perTarget.length > 0 ? perTarget : fallbackTargets
}

export function buildServerCardNetworkState(
  summary: Pick<NetworkServerSummary, 'latency_sparkline' | 'loss_sparkline' | 'targets' | 'last_probe_at'> | undefined,
  realtimeData: Record<string, NetworkProbeResultData[]>,
  bucketSeconds = 60
): ServerCardNetworkState {
  const targetOrder = buildTargetOrder(summary?.targets ?? [])
  const fallbackTargets = buildFallbackTargets(summary?.targets ?? [], targetOrder)
  const fallbackState = buildFallbackState(summary, fallbackTargets, bucketSeconds)
  const hasBackendSparkline =
    (summary?.latency_sparkline?.length ?? 0) > 0 || (summary?.loss_sparkline?.length ?? 0) > 0
  const targetLookup = buildTargetLookup(summary?.targets ?? [])
  const bucketMap = new Map<string, ServerCardTooltipTarget[]>()

  for (const [targetId, results] of Object.entries(realtimeData)) {
    const target = targetLookup.get(targetId)
    const targetName = target?.target_name ?? targetId

    for (const result of results) {
      const bucket = bucketMap.get(result.timestamp) ?? []
      bucket.push({
        latency: result.avg_latency,
        lossRatio: result.packet_loss,
        targetId,
        targetName
      })
      bucketMap.set(result.timestamp, bucket)
    }
  }

  let trend: ServerCardNetworkTrend
  if (bucketMap.size > 0) {
    const realtimeState = buildRealtimeState(bucketMap, targetOrder)
    trend = padState(hasBackendSparkline ? mergeStates(fallbackState, realtimeState) : realtimeState)
  } else {
    trend = padState(fallbackState)
  }

  return {
    ...trend,
    currentTargets: selectCurrentTargets(trend.latencyPoints, fallbackTargets)
  }
}

import type { NetworkProbeResultData, NetworkServerSummary, NetworkTargetSummary } from '@/lib/network-types'

const MAX_TREND_POINTS = 12

export interface ServerCardTooltipTarget {
  latency: number | null
  lossRatio: number
  targetId: string
  targetName: string
}

export interface ServerCardMetricPoint {
  synthetic: boolean
  targets: ServerCardTooltipTarget[]
  timestamp: string
  value: number | null
}

export interface ServerCardNetworkState {
  currentAvgLatency: number | null
  currentAvgLossRatio: number | null
  latencyPoints: ServerCardMetricPoint[]
  lossPoints: ServerCardMetricPoint[]
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

function padState(state: ServerCardNetworkState): ServerCardNetworkState {
  return {
    ...state,
    latencyPoints: padMetricPoints(state.latencyPoints),
    lossPoints: padMetricPoints(state.lossPoints)
  }
}

function buildRealtimeState(
  bucketMap: ReadonlyMap<string, ServerCardTooltipTarget[]>,
  targetOrder: ReadonlyMap<string, number>
): ServerCardNetworkState {
  const timestamps = [...bucketMap.keys()].sort().slice(-MAX_TREND_POINTS)
  const latencyPoints = timestamps.map((timestamp) => {
    const targets = [...(bucketMap.get(timestamp) ?? [])].sort((left, right) =>
      compareTargets(left, right, targetOrder)
    )
    const avgLatency = average(targets.flatMap((target) => (target.latency == null ? [] : [target.latency])))

    return {
      synthetic: false,
      targets,
      timestamp,
      value: avgLatency
    }
  })
  const lossPoints = timestamps.map((timestamp) => {
    const targets = [...(bucketMap.get(timestamp) ?? [])].sort((left, right) =>
      compareTargets(left, right, targetOrder)
    )
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

function buildFallbackState(
  summary: Pick<NetworkServerSummary, 'latency_sparkline' | 'loss_sparkline' | 'targets'> | undefined,
  fallbackTargets: readonly ServerCardTooltipTarget[]
): ServerCardNetworkState {
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
  const latencyPoints: ServerCardMetricPoint[] = []
  const lossPoints: ServerCardMetricPoint[] = []

  for (let index = startIndex; index < pointCount; index += 1) {
    const timestamp = buildSyntheticTimestamp(index)
    latencyPoints.push({
      synthetic: true,
      targets: fallbackTargets,
      timestamp,
      value: latencySparkline[index] ?? null
    })
    lossPoints.push({
      synthetic: true,
      targets: fallbackTargets,
      timestamp,
      value: lossSparkline[index] == null ? null : lossSparkline[index] * 100
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
  fallbackState: ServerCardNetworkState,
  realtimeState: ServerCardNetworkState
): ServerCardNetworkState {
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

export function buildServerCardNetworkState(
  summary: Pick<NetworkServerSummary, 'latency_sparkline' | 'loss_sparkline' | 'targets'> | undefined,
  realtimeData: Record<string, NetworkProbeResultData[]>
): ServerCardNetworkState {
  const targetOrder = buildTargetOrder(summary?.targets ?? [])
  const fallbackTargets = buildFallbackTargets(summary?.targets ?? [], targetOrder)
  const fallbackState = buildFallbackState(summary, fallbackTargets)
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

  if (bucketMap.size > 0) {
    const realtimeState = buildRealtimeState(bucketMap, targetOrder)
    return padState(hasBackendSparkline ? mergeStates(fallbackState, realtimeState) : realtimeState)
  }

  return padState(fallbackState)
}

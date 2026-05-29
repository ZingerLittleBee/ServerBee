import type { NetworkProbeRecord, NetworkProbeResultData } from './network-types'

interface MergeArgs {
  historical: NetworkProbeRecord[]
  isRealtime: boolean
  realtime: Record<string, NetworkProbeResultData[]>
  seed: NetworkProbeRecord[]
  serverId: string
}

// Combine the 1h "seed" snapshot with live realtime points in realtime mode, or
// return historical records as-is otherwise. Realtime points override seed points
// at the same (target_id, timestamp) bucket. Mirrors the logic previously inlined
// in the network detail page.
export function mergeNetworkChartRecords({
  historical,
  isRealtime,
  realtime,
  seed,
  serverId
}: MergeArgs): NetworkProbeRecord[] {
  if (!isRealtime) {
    return historical
  }

  const realtimeFlat: NetworkProbeRecord[] = []
  for (const [targetId, points] of Object.entries(realtime)) {
    for (const point of points) {
      realtimeFlat.push({
        id: 0,
        server_id: serverId,
        target_id: targetId,
        timestamp: point.timestamp,
        avg_latency: point.avg_latency,
        min_latency: point.min_latency,
        max_latency: point.max_latency,
        packet_loss: point.packet_loss,
        packet_sent: point.packet_sent,
        packet_received: point.packet_received
      })
    }
  }

  const merged = [...seed, ...realtimeFlat]
  const seen = new Set<string>()
  const deduped: NetworkProbeRecord[] = []
  for (let i = merged.length - 1; i >= 0; i--) {
    const r = merged[i]
    const key = `${r.target_id}:${r.timestamp}`
    if (!seen.has(key)) {
      seen.add(key)
      deduped.push(r)
    }
  }
  deduped.reverse()
  return deduped
}

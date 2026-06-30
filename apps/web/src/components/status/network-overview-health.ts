import { getCombinedSeverity } from '@/lib/network-latency-constants'
import type { NetworkOverviewSummary } from './network-overview-content'

export type Health = 'healthy' | 'warning' | 'severe' | 'unknown' | 'offline'

export function avgLatencyFromTargets(targets: NetworkOverviewSummary['targets']): number | null {
  const valid = targets.filter((t) => t.avg_latency != null)
  if (valid.length === 0) {
    return null
  }
  return valid.reduce((sum, t) => sum + (t.avg_latency ?? 0), 0) / valid.length
}

export function avgLossFromTargets(targets: NetworkOverviewSummary['targets']): number | null {
  if (targets.length === 0) {
    return null
  }
  return targets.reduce((sum, t) => sum + t.packet_loss, 0) / targets.length
}

export function serverHealth(summary: NetworkOverviewSummary): Health {
  if (!summary.online) {
    return 'offline'
  }
  const latency = avgLatencyFromTargets(summary.targets)
  if (latency == null) {
    return 'unknown'
  }
  const sev = getCombinedSeverity({ latencyMs: latency, lossRatio: avgLossFromTargets(summary.targets) })
  if (sev === 'failed' || sev === 'severe') {
    return 'severe'
  }
  if (sev === 'warning') {
    return 'warning'
  }
  return 'healthy'
}

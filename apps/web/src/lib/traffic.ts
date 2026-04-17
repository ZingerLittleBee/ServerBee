import type { TrafficOverviewItem } from '@/hooks/use-traffic-overview'

export const DEFAULT_TRAFFIC_LIMIT_BYTES = 1024 ** 4

export interface TrafficQuota {
  limit: number
  pct: number
  used: number
}

interface ComputeInput {
  entry: TrafficOverviewItem | undefined
  netInTransfer: number
  netOutTransfer: number
}

export function computeTrafficQuota({ entry, netInTransfer, netOutTransfer }: ComputeInput): TrafficQuota {
  const used = entry ? entry.cycle_in + entry.cycle_out : netInTransfer + netOutTransfer
  const rawLimit = entry?.traffic_limit ?? null
  const limit = rawLimit != null && rawLimit > 0 ? rawLimit : DEFAULT_TRAFFIC_LIMIT_BYTES
  const rawPct = limit > 0 ? (used / limit) * 100 : 0
  const pct = Math.min(rawPct, 100)
  return { used, limit, pct }
}

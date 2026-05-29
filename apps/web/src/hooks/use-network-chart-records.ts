import { useMemo } from 'react'
import { useNetworkRecords } from '@/hooks/use-network-api'
import { useNetworkRealtime } from '@/hooks/use-network-realtime'
import { mergeNetworkChartRecords } from '@/lib/network-chart-records'
import type { NetworkProbeRecord } from '@/lib/network-types'

// `hours === 0` means realtime. Returns a record series ready for LatencyChart,
// combining historical OR (seed + live) data depending on the range.
export function useNetworkChartRecords(serverId: string, hours: number): NetworkProbeRecord[] {
  const isRealtime = hours === 0
  const { data: historical } = useNetworkRecords(serverId, hours, { enabled: !isRealtime && serverId.length > 0 })
  const { data: seed } = useNetworkRecords(serverId, 1, { enabled: isRealtime && serverId.length > 0 })
  const { data: realtime } = useNetworkRealtime(serverId)

  return useMemo(
    () =>
      mergeNetworkChartRecords({
        historical: historical ?? [],
        isRealtime,
        realtime,
        seed: seed ?? [],
        serverId
      }),
    [historical, isRealtime, realtime, seed, serverId]
  )
}

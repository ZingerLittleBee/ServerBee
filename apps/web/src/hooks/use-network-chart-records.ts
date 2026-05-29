import { useMemo } from 'react'
import { useNetworkRecords } from '@/hooks/use-network-api'
import { useNetworkRealtime } from '@/hooks/use-network-realtime'
import { mergeNetworkChartRecords } from '@/lib/network-chart-records'
import type { NetworkProbeRecord } from '@/lib/network-types'

interface NetworkChartRecords {
  // True while the query backing the current range is still loading its first
  // page, so callers can show a skeleton instead of flashing an empty state.
  isLoading: boolean
  records: NetworkProbeRecord[]
}

// `hours === 0` means realtime. Returns a record series ready for LatencyChart,
// combining historical OR (seed + live) data depending on the range, plus the
// loading state of whichever query is active for the current range.
export function useNetworkChartRecords(serverId: string, hours: number): NetworkChartRecords {
  const isRealtime = hours === 0
  const historicalQuery = useNetworkRecords(serverId, hours, { enabled: !isRealtime && serverId.length > 0 })
  const seedQuery = useNetworkRecords(serverId, 1, { enabled: isRealtime && serverId.length > 0 })
  const { data: realtime } = useNetworkRealtime(serverId)

  const records = useMemo(
    () =>
      mergeNetworkChartRecords({
        historical: historicalQuery.data ?? [],
        isRealtime,
        realtime,
        seed: seedQuery.data ?? [],
        serverId
      }),
    [historicalQuery.data, isRealtime, realtime, seedQuery.data, serverId]
  )

  const isLoading = isRealtime ? seedQuery.isLoading : historicalQuery.isLoading

  return { isLoading, records }
}

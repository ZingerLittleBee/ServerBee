import { useCallback, useEffect, useRef, useState } from 'react'
import type { NetworkProbeResultData } from '@/lib/network-types'

const MAX_POINTS = 200
const TRIM_AT = 250

interface RealtimeData {
  [targetId: string]: NetworkProbeResultData[]
}

export function useNetworkRealtime(serverId: string) {
  const [data, setData] = useState<RealtimeData>({})
  const dataRef = useRef<RealtimeData>({})

  const handleUpdate = useCallback(
    (event: Event) => {
      const detail = (event as CustomEvent).detail
      if (detail.server_id !== serverId) {
        return
      }

      const results: NetworkProbeResultData[] = detail.results
      const newData = { ...dataRef.current }

      for (const result of results) {
        // Rebuild the array immutably: `newData` is only a shallow copy of the
        // previous state, so pushing in place would mutate the array still held by
        // the last committed React state. This runs on every probe tick.
        const next = [...(newData[result.target_id] ?? []), result]
        newData[result.target_id] = next.length > TRIM_AT ? next.slice(-MAX_POINTS) : next
      }

      dataRef.current = newData
      setData(newData)
    },
    [serverId]
  )

  useEffect(() => {
    window.addEventListener('network-probe-update', handleUpdate)
    return () => window.removeEventListener('network-probe-update', handleUpdate)
  }, [handleUpdate])

  const reset = useCallback(() => {
    dataRef.current = {}
    setData({})
  }, [])

  return { data, reset }
}

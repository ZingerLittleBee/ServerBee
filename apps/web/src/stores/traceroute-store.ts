import { create } from 'zustand'
import type { TracerouteStreamState } from '@/hooks/use-traceroute-stream'
import type { TraceProtocol } from '@/lib/network-types'

/** Per-server traceroute run state that must survive tab switches. The network
 * tab unmounts when the viewer switches to another server-detail tab; keeping
 * the in-flight request id plus the latest stream frame here lets the tab
 * resume the live progress view on remount (each `traceroute_update` frame
 * carries the full hop state, so the next frame fully catches us up). Ephemeral
 * UI state (dialog open flags, legend filters) intentionally stays local. */
export interface TracerouteRunState {
  selectedRecordId: string | null
  streamSnapshot: TracerouteStreamState | null
  traceProtocol: TraceProtocol
  traceRequestId: string | null
  traceTarget: string
}

export const INITIAL_TRACEROUTE_RUN_STATE: TracerouteRunState = {
  selectedRecordId: null,
  streamSnapshot: null,
  traceProtocol: 'icmp',
  traceRequestId: null,
  traceTarget: ''
}

interface TracerouteStoreState {
  byServer: Map<string, TracerouteRunState>
  patchServer: (serverId: string, patch: Partial<TracerouteRunState>) => void
}

export const useTracerouteStore = create<TracerouteStoreState>()((set) => ({
  byServer: new Map(),

  patchServer: (serverId: string, patch: Partial<TracerouteRunState>) => {
    set((state) => {
      const byServer = new Map(state.byServer)
      const current = byServer.get(serverId) ?? INITIAL_TRACEROUTE_RUN_STATE
      byServer.set(serverId, { ...current, ...patch })
      return { byServer }
    })
  }
}))

export function useTracerouteRunState(serverId: string): TracerouteRunState {
  return useTracerouteStore((state) => state.byServer.get(serverId)) ?? INITIAL_TRACEROUTE_RUN_STATE
}

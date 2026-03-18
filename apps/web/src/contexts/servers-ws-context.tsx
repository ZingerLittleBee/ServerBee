import { createContext, useContext } from 'react'
import type { ConnectionState } from '@/lib/ws-client'

interface ServersWsContextValue {
  connectionState: ConnectionState
  send: (data: unknown) => void
}

export const ServersWsContext = createContext<ServersWsContextValue | null>(null)

export function useServersWsSend(): ServersWsContextValue {
  const ctx = useContext(ServersWsContext)
  if (!ctx) {
    throw new Error('useServersWsSend must be used within ServersWsContext provider')
  }
  return ctx
}

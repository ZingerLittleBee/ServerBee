import { useEffect } from 'react'
import { useServersWsSend } from '@/contexts/servers-ws-context'

export function useDockerSubscription(serverId: string, enabled = true): void {
  const { send, connectionState } = useServersWsSend()

  useEffect(() => {
    if (!enabled || connectionState !== 'connected') {
      return
    }

    send({ type: 'docker_subscribe', server_id: serverId })

    return () => {
      if (enabled && connectionState === 'connected') {
        send({ type: 'docker_unsubscribe', server_id: serverId })
      }
    }
  }, [connectionState, enabled, send, serverId])
}

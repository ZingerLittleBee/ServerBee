import { useEffect } from 'react'
import { useServersWsSend } from '@/contexts/servers-ws-context'

export function useDockerSubscription(serverId: string): void {
  const { send, connectionState } = useServersWsSend()

  useEffect(() => {
    if (connectionState !== 'connected') {
      return
    }

    send({ type: 'docker_subscribe', server_id: serverId })

    return () => {
      if (connectionState === 'connected') {
        send({ type: 'docker_unsubscribe', server_id: serverId })
      }
    }
  }, [connectionState, send, serverId])
}

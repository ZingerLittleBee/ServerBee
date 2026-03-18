import { useEffect } from 'react'
import { useServersWsSend } from '@/contexts/servers-ws-context'

export function useDockerSubscription(serverId: string): void {
  const { send } = useServersWsSend()

  useEffect(() => {
    send({ type: 'docker_subscribe', server_id: serverId })

    return () => {
      send({ type: 'docker_unsubscribe', server_id: serverId })
    }
  }, [send, serverId])
}

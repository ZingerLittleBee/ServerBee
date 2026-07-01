import { useServer } from '@/hooks/use-api'
import { ServerEditDialog } from './server-edit-dialog'

export function ServerCardEditDialog({ serverId, onClose }: { onClose: () => void; serverId: string }) {
  const { data: server } = useServer(serverId)
  if (!server) {
    return null
  }
  return <ServerEditDialog onClose={onClose} open server={server} />
}

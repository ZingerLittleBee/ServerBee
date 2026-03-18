import { useQuery } from '@tanstack/react-query'
import { Badge } from '@/components/ui/badge'
import { Dialog, DialogContent, DialogHeader, DialogTitle } from '@/components/ui/dialog'
import { Skeleton } from '@/components/ui/skeleton'
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from '@/components/ui/table'
import { api } from '@/lib/api-client'
import type { DockerNetwork } from '../types'

interface DockerNetworksDialogProps {
  onOpenChange: (open: boolean) => void
  open: boolean
  serverId: string
}

export function DockerNetworksDialog({ serverId, open, onOpenChange }: DockerNetworksDialogProps) {
  const { data: networks, isLoading } = useQuery<DockerNetwork[]>({
    queryKey: ['docker', 'networks', serverId],
    queryFn: () => api.get<DockerNetwork[]>(`/api/servers/${serverId}/docker/networks`),
    enabled: open
  })

  return (
    <Dialog onOpenChange={onOpenChange} open={open}>
      <DialogContent className="max-h-[85vh] overflow-y-auto sm:max-w-2xl">
        <DialogHeader>
          <DialogTitle>Docker Networks</DialogTitle>
        </DialogHeader>

        {isLoading && (
          <div className="space-y-3">
            <Skeleton className="h-8 w-full" />
            <Skeleton className="h-8 w-full" />
            <Skeleton className="h-8 w-full" />
          </div>
        )}

        {!isLoading && (!networks || networks.length === 0) && (
          <div className="flex min-h-[120px] items-center justify-center">
            <p className="text-muted-foreground text-sm">No networks found</p>
          </div>
        )}

        {!isLoading && networks && networks.length > 0 && (
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>Name</TableHead>
                <TableHead>Driver</TableHead>
                <TableHead>Scope</TableHead>
                <TableHead className="text-right">Containers</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {networks.map((network) => (
                <TableRow key={network.id}>
                  <TableCell className="font-medium">{network.name}</TableCell>
                  <TableCell>
                    <Badge variant="secondary">{network.driver}</Badge>
                  </TableCell>
                  <TableCell>{network.scope}</TableCell>
                  <TableCell className="text-right">{Object.keys(network.containers).length}</TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
        )}
      </DialogContent>
    </Dialog>
  )
}

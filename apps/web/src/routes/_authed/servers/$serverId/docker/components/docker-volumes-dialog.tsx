import { useQuery } from '@tanstack/react-query'
import { Badge } from '@/components/ui/badge'
import { Dialog, DialogContent, DialogHeader, DialogTitle } from '@/components/ui/dialog'
import { Skeleton } from '@/components/ui/skeleton'
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from '@/components/ui/table'
import { api } from '@/lib/api-client'
import type { DockerVolume } from '../types'

interface DockerVolumesDialogProps {
  onOpenChange: (open: boolean) => void
  open: boolean
  serverId: string
}

function formatCreatedDate(dateStr: string | null): string {
  if (!dateStr) {
    return 'N/A'
  }
  return new Date(dateStr).toLocaleString()
}

export function DockerVolumesDialog({ serverId, open, onOpenChange }: DockerVolumesDialogProps) {
  const { data: volumes, isLoading } = useQuery<DockerVolume[]>({
    queryKey: ['docker', 'volumes', serverId],
    queryFn: () => api.get<DockerVolume[]>(`/api/servers/${serverId}/docker/volumes`),
    enabled: open
  })

  return (
    <Dialog onOpenChange={onOpenChange} open={open}>
      <DialogContent className="max-h-[85vh] overflow-y-auto sm:max-w-3xl">
        <DialogHeader>
          <DialogTitle>Docker Volumes</DialogTitle>
        </DialogHeader>

        {isLoading && (
          <div className="space-y-3">
            <Skeleton className="h-8 w-full" />
            <Skeleton className="h-8 w-full" />
            <Skeleton className="h-8 w-full" />
          </div>
        )}

        {!isLoading && (!volumes || volumes.length === 0) && (
          <div className="flex min-h-[120px] items-center justify-center">
            <p className="text-muted-foreground text-sm">No volumes found</p>
          </div>
        )}

        {!isLoading && volumes && volumes.length > 0 && (
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>Name</TableHead>
                <TableHead>Driver</TableHead>
                <TableHead>Mountpoint</TableHead>
                <TableHead>Created</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {volumes.map((volume) => (
                <TableRow key={volume.name}>
                  <TableCell className="max-w-[200px] truncate font-medium" title={volume.name}>
                    {volume.name}
                  </TableCell>
                  <TableCell>
                    <Badge variant="secondary">{volume.driver}</Badge>
                  </TableCell>
                  <TableCell className="max-w-[250px] truncate font-mono text-xs" title={volume.mountpoint}>
                    {volume.mountpoint}
                  </TableCell>
                  <TableCell className="text-sm">{formatCreatedDate(volume.created_at)}</TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
        )}
      </DialogContent>
    </Dialog>
  )
}

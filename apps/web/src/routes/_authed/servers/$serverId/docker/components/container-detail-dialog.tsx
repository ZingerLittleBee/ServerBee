import { useMemo } from 'react'
import { useTranslation } from 'react-i18next'
import { Badge } from '@/components/ui/badge'
import { Dialog, DialogContent, DialogHeader, DialogTitle } from '@/components/ui/dialog'
import type { DockerContainer, DockerContainerStats } from '../types'
import { ContainerLogs } from './container-logs'
import { ContainerStats } from './container-stats'

interface ContainerDetailDialogProps {
  container: DockerContainer | null
  onOpenChange: (open: boolean) => void
  open: boolean
  serverId: string
  stats: DockerContainerStats[]
}

function formatPortMapping(container: DockerContainer): string {
  const mappings = container.ports
    .filter((p) => p.public_port != null)
    .map((p) => `${p.ip ?? '0.0.0.0'}:${p.public_port} -> ${p.private_port}/${p.port_type}`)

  return mappings.length > 0 ? mappings.join(', ') : 'None'
}

function formatCreatedDate(timestamp: number): string {
  return new Date(timestamp * 1000).toLocaleString()
}

export function ContainerDetailDialog({ container, serverId, stats, open, onOpenChange }: ContainerDetailDialogProps) {
  const { t } = useTranslation('docker')
  const containerStats = useMemo(() => {
    if (!container) {
      return undefined
    }
    return stats.find((s) => s.id === container.id)
  }, [container, stats])

  if (!container) {
    return null
  }

  const portsDisplay = formatPortMapping(container)

  return (
    <Dialog onOpenChange={onOpenChange} open={open}>
      <DialogContent className="max-h-[85vh] overflow-y-auto sm:max-w-3xl">
        <DialogHeader>
          <DialogTitle>{container.name}</DialogTitle>
        </DialogHeader>

        <div className="space-y-6">
          {/* Container Meta Info */}
          <div className="grid gap-3 sm:grid-cols-2">
            <div>
              <p className="text-muted-foreground text-xs">{t('detail.image')}</p>
              <p className="mt-0.5 truncate font-mono text-sm" title={container.image}>
                {container.image}
              </p>
            </div>
            <div>
              <p className="text-muted-foreground text-xs">{t('detail.status')}</p>
              <div className="mt-0.5 flex items-center gap-2">
                <Badge variant={container.state === 'running' ? 'default' : 'secondary'}>{container.state}</Badge>
                <span className="text-muted-foreground text-sm">{container.status}</span>
              </div>
            </div>
            <div>
              <p className="text-muted-foreground text-xs">{t('detail.ports')}</p>
              <p className="mt-0.5 font-mono text-sm">{portsDisplay === 'None' ? t('detail.noPorts') : portsDisplay}</p>
            </div>
            <div>
              <p className="text-muted-foreground text-xs">{t('detail.created')}</p>
              <p className="mt-0.5 text-sm">{formatCreatedDate(container.created)}</p>
            </div>
            <div className="sm:col-span-2">
              <p className="text-muted-foreground text-xs">{t('detail.containerId')}</p>
              <p className="mt-0.5 truncate font-mono text-sm" title={container.id}>
                {container.id}
              </p>
            </div>
          </div>

          {/* Stats */}
          <ContainerStats stats={containerStats} />

          {/* Logs */}
          <ContainerLogs containerId={container.id} serverId={serverId} />
        </div>
      </DialogContent>
    </Dialog>
  )
}

import { useMutation, useQueryClient } from '@tanstack/react-query'
import { Loader2, Play, RotateCcw, Square, Trash2 } from 'lucide-react'
import { useMemo, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { toast } from 'sonner'
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle
} from '@/components/ui/alert-dialog'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Dialog, DialogBody, DialogContent, DialogHeader, DialogTitle } from '@/components/ui/dialog'
import { useAuth } from '@/hooks/use-auth'
import { api } from '@/lib/api-client'
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

// Externally-tagged DockerAction (crates/common/src/docker_types.rs): unit
// variant serializes to a bare string, struct variants to a single-key object.
type DockerActionBody =
  | 'Start'
  | { Stop: { timeout: null } }
  | { Restart: { timeout: null } }
  | { Remove: { force: boolean } }

interface ActionResult {
  error: string | null
  success: boolean
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
  const { user } = useAuth()
  const queryClient = useQueryClient()
  const [confirmRemove, setConfirmRemove] = useState(false)
  const isAdmin = user?.role === 'admin'

  const actionMutation = useMutation({
    mutationFn: (input: { action: DockerActionBody; containerId: string }) =>
      api.post<ActionResult>(`/api/servers/${serverId}/docker/containers/${input.containerId}/action`, {
        action: input.action
      }),
    onSuccess: (data, variables) => {
      if (!data.success) {
        toast.error(data.error ?? t('actions.failed'))
        return
      }
      const isRemove = typeof variables.action === 'object' && 'Remove' in variables.action
      const isStop = typeof variables.action === 'object' && 'Stop' in variables.action
      const isRestart = typeof variables.action === 'object' && 'Restart' in variables.action
      let message = t('actions.started')
      if (isRemove) {
        message = t('actions.removed')
      } else if (isStop) {
        message = t('actions.stopped')
      } else if (isRestart) {
        message = t('actions.restarted')
      }
      toast.success(message)
      queryClient.invalidateQueries({ queryKey: ['docker', 'events', serverId] })
      if (isRemove) {
        onOpenChange(false)
      }
    },
    onError: (err: unknown) => {
      toast.error(err instanceof Error ? err.message : t('actions.failed'))
    }
  })

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
  const isRunning = container.state === 'running'
  const isPending = actionMutation.isPending

  const runAction = (action: DockerActionBody) => {
    actionMutation.mutate({ action, containerId: container.id })
  }

  return (
    <Dialog onOpenChange={onOpenChange} open={open}>
      <DialogContent className="sm:max-w-3xl">
        <DialogHeader>
          <DialogTitle>{container.name}</DialogTitle>
        </DialogHeader>

        <DialogBody>
          <div className="space-y-6">
            {/* Lifecycle actions (admin only) */}
            {isAdmin && (
              <div className="flex flex-wrap items-center gap-2">
                {isRunning ? (
                  <>
                    <Button
                      disabled={isPending}
                      onClick={() => runAction({ Restart: { timeout: null } })}
                      size="sm"
                      variant="outline"
                    >
                      {isPending ? (
                        <Loader2 aria-hidden="true" className="mr-1.5 size-4 animate-spin" />
                      ) : (
                        <RotateCcw aria-hidden="true" className="mr-1.5 size-4" />
                      )}
                      {t('actions.restart')}
                    </Button>
                    <Button
                      disabled={isPending}
                      onClick={() => runAction({ Stop: { timeout: null } })}
                      size="sm"
                      variant="outline"
                    >
                      <Square aria-hidden="true" className="mr-1.5 size-4" />
                      {t('actions.stop')}
                    </Button>
                  </>
                ) : (
                  <Button disabled={isPending} onClick={() => runAction('Start')} size="sm" variant="outline">
                    {isPending ? (
                      <Loader2 aria-hidden="true" className="mr-1.5 size-4 animate-spin" />
                    ) : (
                      <Play aria-hidden="true" className="mr-1.5 size-4" />
                    )}
                    {t('actions.start')}
                  </Button>
                )}
                <Button disabled={isPending} onClick={() => setConfirmRemove(true)} size="sm" variant="destructive">
                  <Trash2 aria-hidden="true" className="mr-1.5 size-4" />
                  {t('actions.remove')}
                </Button>
              </div>
            )}

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
                <p className="mt-0.5 font-mono text-sm">
                  {portsDisplay === 'None' ? t('detail.noPorts') : portsDisplay}
                </p>
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
        </DialogBody>
      </DialogContent>

      <AlertDialog onOpenChange={setConfirmRemove} open={confirmRemove}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>{t('actions.removeTitle')}</AlertDialogTitle>
            <AlertDialogDescription>
              {t('actions.removeConfirm', { name: container.name })}
              {isRunning ? ` ${t('actions.removeRunningNote')}` : ''}
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>{t('actions.cancel')}</AlertDialogCancel>
            <AlertDialogAction onClick={() => runAction({ Remove: { force: isRunning } })}>
              {t('actions.remove')}
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </Dialog>
  )
}

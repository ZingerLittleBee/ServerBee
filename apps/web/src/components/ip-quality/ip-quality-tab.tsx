import { AlertTriangle, Copy, RefreshCw, ShieldCheck } from 'lucide-react'
import { useTranslation } from 'react-i18next'
import { toast } from 'sonner'
import { IpQualityCard } from '@/components/ip-quality/ip-quality-card'
import { UnlockMatrix } from '@/components/ip-quality/unlock-matrix'
import { UnlockStatusBadge } from '@/components/ip-quality/unlock-status-badge'
import { Button } from '@/components/ui/button'
import {
  Dialog,
  DialogBody,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
  DialogTrigger
} from '@/components/ui/dialog'
import { ScrollArea } from '@/components/ui/scroll-area'
import { Skeleton } from '@/components/ui/skeleton'
import { useCheckNow, useIpQualityEvents, useIpQualityServer, useIpQualityServices } from '@/hooks/use-ip-quality-api'
import { CAP_IP_QUALITY, hasCap } from '@/lib/capabilities'
import { formatDateTime } from '@/lib/format'
import type { UnlockStatus } from '@/lib/ip-quality-types'

interface Props {
  /** Bitmap allowed by the running agent process (null when agent has not reported yet). */
  agentLocalCapabilities?: number | null
  serverId: string
  serverName: string
}

type CapState = 'ok' | 'off' | 'unknown'

function deriveCapState(agentLocalCapabilities?: number | null): CapState {
  if (agentLocalCapabilities == null) {
    return 'unknown'
  }
  return hasCap(agentLocalCapabilities, CAP_IP_QUALITY) ? 'ok' : 'off'
}

export function IpQualityTab({ serverId, serverName, agentLocalCapabilities }: Props) {
  const { t } = useTranslation('ip-quality')

  const { data: serverData, isLoading: serverLoading } = useIpQualityServer(serverId)
  const { data: services = [], isLoading: servicesLoading } = useIpQualityServices()
  const { data: events = [], isLoading: eventsLoading } = useIpQualityEvents(serverId)
  const checkNow = useCheckNow()

  const isLoading = serverLoading || servicesLoading || eventsLoading
  const capState = deriveCapState(agentLocalCapabilities)
  const canCheck = capState === 'ok'

  const enabledServices = services.filter((s) => s.enabled)

  // Build a single-server array for UnlockMatrix
  const servers = [{ id: serverId, name: serverName }]
  const overview = serverData ? [serverData] : []

  function handleCheckNow() {
    checkNow.mutate(serverId, {
      onSuccess: () => {
        toast.success(t('check_triggered'))
      },
      onError: (err) => {
        toast.error(err instanceof Error ? err.message : t('check_failed'))
      }
    })
  }

  return (
    <ScrollArea className="w-full">
      <div className="space-y-6 px-px pt-4 pb-4">
        {/* Header row */}
        <div className="flex items-center justify-between">
          <h2 className="font-semibold text-base">{t('tab_title')}</h2>
          <Button disabled={!canCheck || checkNow.isPending} onClick={handleCheckNow} size="sm" variant="outline">
            <RefreshCw aria-hidden="true" className="mr-1.5 size-3.5" />
            {t('check_now')}
          </Button>
        </div>

        {capState !== 'ok' && <CapDisabledCallout state={capState} t={t} />}

        {isLoading && (
          <div className="space-y-3">
            <Skeleton className="h-32 rounded-xl" />
            <Skeleton className="h-24 rounded-xl" />
          </div>
        )}

        {!isLoading && (
          <>
            {/* IP quality card */}
            <IpQualityCard className="max-w-sm" ipQuality={serverData?.ip_quality ?? null} serverName={serverName} />

            {/* Unlock matrix */}
            {enabledServices.length > 0 && (
              <div className="space-y-2">
                <h3 className="font-medium text-muted-foreground text-sm">{t('unlock_matrix')}</h3>
                <UnlockMatrix overview={overview} servers={servers} services={enabledServices} />
              </div>
            )}

            {enabledServices.length === 0 && !serverData?.ip_quality && capState === 'ok' && (
              <div className="flex min-h-[160px] items-center justify-center rounded-xl border border-dashed">
                <div className="space-y-2 text-center">
                  <ShieldCheck aria-hidden="true" className="mx-auto size-8 text-muted-foreground" />
                  <p className="font-medium text-sm">{t('no_data')}</p>
                  <p className="max-w-xs text-muted-foreground text-xs">{t('no_data_hint')}</p>
                </div>
              </div>
            )}

            {/* Status-change event history */}
            {events.length > 0 && (
              <div className="space-y-2">
                <h3 className="font-medium text-muted-foreground text-sm">{t('event_history')}</h3>
                <div className="rounded-xl bg-card ring-1 ring-foreground/10">
                  <table className="w-full border-collapse text-sm">
                    <thead>
                      <tr className="border-b">
                        <th className="px-3 py-2 text-left font-medium">{t('event_service')}</th>
                        <th className="px-3 py-2 text-left font-medium">{t('event_change')}</th>
                        <th className="px-3 py-2 text-left font-medium">{t('event_time')}</th>
                      </tr>
                    </thead>
                    <tbody>
                      {events.map((event) => {
                        const service = services.find((s) => s.id === event.service_id)
                        return (
                          <tr className="border-b last:border-b-0" key={event.id}>
                            <td className="px-3 py-2 font-medium">{service?.name ?? event.service_id}</td>
                            <td className="px-3 py-2 text-muted-foreground">
                              <span className="inline-flex items-center gap-1">
                                <UnlockStatusBadge status={event.old_status as UnlockStatus} />
                                <span>→</span>
                                <UnlockStatusBadge status={event.new_status as UnlockStatus} />
                              </span>
                            </td>
                            <td className="px-3 py-2 text-muted-foreground text-xs">
                              {formatDateTime(event.changed_at)}
                            </td>
                          </tr>
                        )
                      })}
                    </tbody>
                  </table>
                </div>
              </div>
            )}
          </>
        )}
      </div>
    </ScrollArea>
  )
}

function CapDisabledCallout({ state, t }: { state: Exclude<CapState, 'ok'>; t: (key: string) => string }) {
  return (
    <div className="flex gap-3 rounded-xl border border-amber-500/40 bg-amber-500/10 p-4 text-amber-700 dark:text-amber-300">
      <AlertTriangle aria-hidden="true" className="mt-0.5 size-4 shrink-0" />
      <div className="flex min-w-0 flex-1 flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
        <div className="space-y-1">
          <p className="font-medium text-sm">{t(state === 'unknown' ? 'cap_unknown_title' : 'cap_off_title')}</p>
          <p className="text-xs">{t(state === 'unknown' ? 'cap_unknown_hint' : 'cap_off_hint')}</p>
        </div>
        {state === 'off' && (
          <Dialog>
            <DialogTrigger
              render={<Button className="self-start sm:shrink-0 sm:self-auto" size="sm" variant="outline" />}
            >
              {t('cap_enable_action')}
            </DialogTrigger>
            <DialogContent className="sm:max-w-xl">
              <DialogHeader>
                <DialogTitle>{t('cap_enable_dialog_title')}</DialogTitle>
                <DialogDescription>{t('cap_agent_owned_hint')}</DialogDescription>
              </DialogHeader>
              <DialogBody className="space-y-5">
                <section className="space-y-2">
                  <h3 className="font-medium">{t('cap_enable_permanent_title')}</h3>
                  <p className="text-muted-foreground">{t('cap_enable_permanent_hint')}</p>
                  <p className="text-muted-foreground">{t('cap_restart_hint')}</p>
                  <CommandBlock command="sudo serverbee restart agent" label={t('copy_restart_command')} />
                </section>
                <section className="space-y-2">
                  <h3 className="font-medium">{t('cap_enable_temporary_title')}</h3>
                  <p className="text-muted-foreground">{t('cap_enable_temporary_hint')}</p>
                  <CommandBlock
                    command="sudo serverbee-agent grant ip_quality --for 30m"
                    label={t('copy_grant_command')}
                  />
                  <p className="text-muted-foreground">{t('cap_revoke_hint')}</p>
                  <CommandBlock command="sudo serverbee-agent revoke ip_quality" label={t('copy_revoke_command')} />
                </section>
              </DialogBody>
            </DialogContent>
          </Dialog>
        )}
      </div>
    </div>
  )
}

function CommandBlock({ command, label }: { command: string; label: string }) {
  const { t } = useTranslation('ip-quality')

  async function handleCopy() {
    try {
      await navigator.clipboard.writeText(command)
      toast.success(t('command_copied'))
    } catch {
      toast.error(t('command_copy_failed'))
    }
  }

  return (
    <div className="flex items-start gap-2 rounded-md bg-muted p-2">
      <code className="min-w-0 flex-1 self-center whitespace-pre-wrap break-words px-1 text-xs">{command}</code>
      <Button
        aria-label={label}
        className="shrink-0"
        onClick={handleCopy}
        size="icon-sm"
        title={label}
        type="button"
        variant="ghost"
      >
        <Copy aria-hidden="true" className="size-4" />
      </Button>
    </div>
  )
}

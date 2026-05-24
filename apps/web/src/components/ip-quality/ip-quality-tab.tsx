import { AlertTriangle, RefreshCw, ShieldCheck } from 'lucide-react'
import { useTranslation } from 'react-i18next'
import { toast } from 'sonner'
import { IpQualityCard } from '@/components/ip-quality/ip-quality-card'
import { UnlockMatrix } from '@/components/ip-quality/unlock-matrix'
import { Button } from '@/components/ui/button'
import { ScrollArea } from '@/components/ui/scroll-area'
import { Skeleton } from '@/components/ui/skeleton'
import { useCheckNow, useIpQualityEvents, useIpQualityServer, useIpQualityServices } from '@/hooks/use-ip-quality-api'
import { CAP_IP_QUALITY, hasCap } from '@/lib/capabilities'

interface Props {
  /** Bitmap allowed by the running agent process (null when agent has not reported yet). */
  agentLocalCapabilities?: number | null
  /** Server-side configured capability bitmap. */
  capabilities?: number | null
  serverId: string
  serverName: string
}

type CapState = 'ok' | 'server_off' | 'agent_off' | 'both_off'

function deriveCapState(capabilities?: number | null, agentLocalCapabilities?: number | null): CapState {
  const serverHas = capabilities != null && hasCap(capabilities, CAP_IP_QUALITY)
  // A null agent bitmap means the agent has not reported its local policy
  // yet (either offline or pre-protocol-v2). Either way `Check now` would
  // fail server-side validation, so treat it as the agent side being off.
  const agentHas = agentLocalCapabilities != null && hasCap(agentLocalCapabilities, CAP_IP_QUALITY)
  if (serverHas && agentHas) {
    return 'ok'
  }
  if (!(serverHas || agentHas)) {
    return 'both_off'
  }
  if (!serverHas) {
    return 'server_off'
  }
  return 'agent_off'
}

export function IpQualityTab({ serverId, serverName, capabilities, agentLocalCapabilities }: Props) {
  const { t } = useTranslation('ip-quality')

  const { data: serverData, isLoading: serverLoading } = useIpQualityServer(serverId)
  const { data: services = [], isLoading: servicesLoading } = useIpQualityServices()
  const { data: events = [], isLoading: eventsLoading } = useIpQualityEvents(serverId)
  const checkNow = useCheckNow()

  const isLoading = serverLoading || servicesLoading || eventsLoading
  const capState = deriveCapState(capabilities, agentLocalCapabilities)
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
      <div className="space-y-6 pt-4 pb-4">
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
                              <span className="capitalize">{event.old_status}</span>
                              <span className="mx-1">→</span>
                              <span className="capitalize">{event.new_status}</span>
                            </td>
                            <td className="px-3 py-2 text-muted-foreground text-xs">
                              {new Date(event.changed_at).toLocaleString()}
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
  const titleKey = `cap_off_${state}_title`
  const hintKey = `cap_off_${state}_hint`
  return (
    <div className="flex gap-3 rounded-xl border border-amber-500/40 bg-amber-500/10 p-4 text-amber-700 dark:text-amber-300">
      <AlertTriangle aria-hidden="true" className="mt-0.5 size-4 shrink-0" />
      <div className="space-y-1">
        <p className="font-medium text-sm">{t(titleKey)}</p>
        <p className="text-xs">{t(hintKey)}</p>
      </div>
    </div>
  )
}

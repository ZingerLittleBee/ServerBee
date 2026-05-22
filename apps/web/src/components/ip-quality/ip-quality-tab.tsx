import { RefreshCw, ShieldCheck } from 'lucide-react'
import { useTranslation } from 'react-i18next'
import { toast } from 'sonner'
import { IpQualityCard } from '@/components/ip-quality/ip-quality-card'
import { UnlockMatrix } from '@/components/ip-quality/unlock-matrix'
import { Button } from '@/components/ui/button'
import { ScrollArea } from '@/components/ui/scroll-area'
import { Skeleton } from '@/components/ui/skeleton'
import { useCheckNow, useIpQualityEvents, useIpQualityServer, useIpQualityServices } from '@/hooks/use-ip-quality-api'

interface Props {
  serverId: string
  serverName: string
}

export function IpQualityTab({ serverId, serverName }: Props) {
  const { t } = useTranslation('servers')

  const { data: serverData, isLoading: serverLoading } = useIpQualityServer(serverId)
  const { data: services = [], isLoading: servicesLoading } = useIpQualityServices()
  const { data: events = [], isLoading: eventsLoading } = useIpQualityEvents(serverId)
  const checkNow = useCheckNow()

  const isLoading = serverLoading || servicesLoading || eventsLoading

  const enabledServices = services.filter((s) => s.enabled)

  // Build a single-server array for UnlockMatrix
  const servers = [{ id: serverId, name: serverName }]
  const overview = serverData ? [serverData] : []

  function handleCheckNow() {
    checkNow.mutate(serverId, {
      onSuccess: () => {
        toast.success(
          t('ip_quality_check_triggered', { defaultValue: 'Check triggered — results will arrive shortly.' })
        )
      },
      onError: (err) => {
        toast.error(
          err instanceof Error
            ? err.message
            : t('ip_quality_check_failed', { defaultValue: 'Failed to trigger check.' })
        )
      }
    })
  }

  return (
    <ScrollArea className="w-full">
      <div className="space-y-6 pt-4 pb-4">
        {/* Header row */}
        <div className="flex items-center justify-between">
          <h2 className="font-semibold text-base">{t('ip_quality_tab_title', { defaultValue: 'IP Quality' })}</h2>
          <Button disabled={checkNow.isPending} onClick={handleCheckNow} size="sm" variant="outline">
            <RefreshCw aria-hidden="true" className="mr-1.5 size-3.5" />
            {t('ip_quality_check_now', { defaultValue: 'Check now' })}
          </Button>
        </div>

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
                <h3 className="font-medium text-muted-foreground text-sm">
                  {t('ip_quality_unlock_matrix', { defaultValue: 'Unlock Matrix' })}
                </h3>
                <UnlockMatrix overview={overview} servers={servers} services={enabledServices} />
              </div>
            )}

            {enabledServices.length === 0 && !serverData?.ip_quality && (
              <div className="flex min-h-[160px] items-center justify-center rounded-xl border border-dashed">
                <div className="space-y-2 text-center">
                  <ShieldCheck aria-hidden="true" className="mx-auto size-8 text-muted-foreground" />
                  <p className="font-medium text-sm">
                    {t('ip_quality_no_data', { defaultValue: 'No IP quality data yet' })}
                  </p>
                  <p className="max-w-xs text-muted-foreground text-xs">
                    {t('ip_quality_no_data_hint', {
                      defaultValue:
                        'Enable the ip_quality capability on this server and start the agent with --allow-cap ip_quality.'
                    })}
                  </p>
                </div>
              </div>
            )}

            {/* Status-change event history */}
            {events.length > 0 && (
              <div className="space-y-2">
                <h3 className="font-medium text-muted-foreground text-sm">
                  {t('ip_quality_event_history', { defaultValue: 'Status Change History' })}
                </h3>
                <div className="rounded-xl bg-card ring-1 ring-foreground/10">
                  <table className="w-full border-collapse text-sm">
                    <thead>
                      <tr className="border-b">
                        <th className="px-3 py-2 text-left font-medium">
                          {t('ip_quality_event_service', { defaultValue: 'Service' })}
                        </th>
                        <th className="px-3 py-2 text-left font-medium">
                          {t('ip_quality_event_change', { defaultValue: 'Change' })}
                        </th>
                        <th className="px-3 py-2 text-left font-medium">
                          {t('ip_quality_event_time', { defaultValue: 'Time' })}
                        </th>
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

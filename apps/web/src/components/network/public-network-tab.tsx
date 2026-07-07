import { useQuery } from '@tanstack/react-query'
import { useState } from 'react'
import { useTranslation } from 'react-i18next'
import { NetworkDetailContent } from '@/components/status/network-detail-content'
import { Skeleton } from '@/components/ui/skeleton'
import { api } from '@/lib/api-client'
import type { PublicNetworkServerDetail } from '@/lib/api-schema'
import { formatDateTime } from '@/lib/format'

// Public network data exposes only `summary + anomalies` (no latency-record
// history, no traceroute), so the public tab is inherently the light version
// of the admin network tab. The anomaly window matches the auth'd realtime
// default (24h) so the count lines up with the overview card.
const PUBLIC_ANOMALY_WINDOW_HOURS = 24

export function PublicNetworkTab({ serverId }: { serverId: string }) {
  const { t } = useTranslation('network')
  const [anomalyOpen, setAnomalyOpen] = useState(false)

  const { data, isLoading, error } = useQuery({
    queryKey: ['public-status', 'network', serverId],
    queryFn: () => api.get<PublicNetworkServerDetail>(`/api/status/network/${serverId}`),
    refetchInterval: 30_000,
    enabled: serverId.length > 0,
    retry: false
  })

  if (isLoading) {
    return (
      <div className="mt-4 space-y-4">
        <Skeleton className="h-4 w-48" />
        <Skeleton className="h-48 w-full" />
      </div>
    )
  }

  if (error || !data) {
    return (
      <div className="mt-4 flex min-h-[200px] items-center justify-center rounded-lg border border-dashed">
        <p className="text-muted-foreground text-sm">{t('no_data')}</p>
      </div>
    )
  }

  const { summary, anomalies } = data

  return (
    <div className="mt-4">
      {summary.last_probe_at && (
        <p className="mb-4 text-muted-foreground text-sm">
          {t('last_probe')}:{' '}
          {formatDateTime(summary.last_probe_at, {
            month: 'short',
            day: 'numeric',
            hour: '2-digit',
            minute: '2-digit'
          })}
        </p>
      )}

      <NetworkDetailContent
        anomalies={anomalies}
        anomalyOpen={anomalyOpen}
        anomalyWindowHours={PUBLIC_ANOMALY_WINDOW_HOURS}
        onAnomalyOpenChange={setAnomalyOpen}
        summary={summary}
        variant="public"
      />

      {anomalies.length > 0 && (
        <div className="mt-4 flex justify-end">
          <button
            className="rounded-md border bg-card px-3 py-2 text-sm hover:bg-muted/40"
            onClick={() => setAnomalyOpen(true)}
            type="button"
          >
            {t('anomaly_count_with_value', { count: anomalies.length })}
          </button>
        </div>
      )}
    </div>
  )
}

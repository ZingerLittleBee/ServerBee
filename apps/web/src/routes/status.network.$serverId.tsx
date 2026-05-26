import { useQuery } from '@tanstack/react-query'
import { createFileRoute, Link, useNavigate } from '@tanstack/react-router'
import { ArrowLeft } from 'lucide-react'
import { useEffect, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { StatusBadge } from '@/components/server/status-badge'
import { NetworkDetailContent } from '@/components/status/network-detail-content'
import { Skeleton } from '@/components/ui/skeleton'
import { usePublicStatusConfig } from '@/hooks/use-public-status'
import { api } from '@/lib/api-client'
import type { PublicNetworkServerDetail } from '@/lib/api-schema'

export const Route = createFileRoute('/status/network/$serverId')({
  component: PublicNetworkDetailPage
})

// Public network detail does not expose latency-record history or traceroute
// recordings; the API surface returns only `summary + anomalies`. The shared
// `NetworkDetailContent` renders the read-only target tabs + anomaly dialog;
// the anomaly window matches the auth'd realtime default (24h) so the count
// shown in the overview card lines up with what's inside.
const PUBLIC_ANOMALY_WINDOW_HOURS = 24

function PublicNetworkDetailPage() {
  const { serverId } = Route.useParams()
  const { t } = useTranslation('network')
  const { data: config } = usePublicStatusConfig()
  const navigate = useNavigate()
  const [anomalyOpen, setAnomalyOpen] = useState(false)

  const networkEnabled = config?.show_network !== false
  const { data, isLoading, error } = useQuery({
    queryKey: ['public-status', 'network', serverId],
    queryFn: () => api.get<PublicNetworkServerDetail>(`/api/status/network/${serverId}`),
    refetchInterval: 30_000,
    enabled: networkEnabled && serverId.length > 0,
    retry: false
  })

  useEffect(() => {
    if (config && config.show_network === false) {
      navigate({ to: '/status', replace: true })
    }
  }, [config, navigate])

  if (config?.show_network === false) {
    return null
  }

  if (isLoading) {
    return (
      <div className="space-y-6">
        <Skeleton className="h-8 w-48" />
        <Skeleton className="h-4 w-96" />
        <Skeleton className="h-48 w-full" />
      </div>
    )
  }

  if (error || !data) {
    return (
      <div className="flex min-h-[400px] items-center justify-center">
        <p className="text-muted-foreground">{t('server_not_found')}</p>
      </div>
    )
  }

  const { summary, anomalies } = data

  return (
    <div className="pb-6">
      <div className="mb-6">
        <Link
          className="mb-3 inline-flex items-center gap-1 text-muted-foreground text-sm hover:text-foreground"
          to="/status/network"
        >
          <ArrowLeft aria-hidden="true" className="size-4" />
          {t('back_to_overview')}
        </Link>

        <div className="flex items-center gap-3">
          <h1 className="font-bold text-2xl">{summary.server_name}</h1>
          <StatusBadge status={summary.online ? 'online' : 'offline'} />
        </div>

        <div className="mt-2 flex flex-wrap gap-x-4 gap-y-1 text-muted-foreground text-sm">
          {summary.last_probe_at && (
            <span>
              {t('last_probe')}:{' '}
              {new Date(summary.last_probe_at).toLocaleString([], {
                month: 'short',
                day: 'numeric',
                hour: '2-digit',
                minute: '2-digit'
              })}
            </span>
          )}
        </div>
      </div>

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

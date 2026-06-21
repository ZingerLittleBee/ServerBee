import { useQuery } from '@tanstack/react-query'
import { createFileRoute, Link, useNavigate } from '@tanstack/react-router'
import { ArrowLeft } from 'lucide-react'
import { useEffect } from 'react'
import { useTranslation } from 'react-i18next'
import { CountryFlag } from '@/components/country-flag'
import { StatusBadge } from '@/components/server/status-badge'
import { ServerDetailContent } from '@/components/status/server-detail-content'
import { Skeleton } from '@/components/ui/skeleton'
import { usePublicStatusConfig } from '@/hooks/use-public-status'
import { api } from '@/lib/api-client'
import type { PublicServerDetail } from '@/lib/api-schema'
import { formatBytes } from '@/lib/utils'

interface PublicServerDetailSearch {
  range?: string
}

export const Route = createFileRoute('/status/server/$serverId')({
  component: PublicServerDetailPage,
  validateSearch: (search: Record<string, unknown>): PublicServerDetailSearch => ({
    range: typeof search.range === 'string' ? search.range : undefined
  })
})

function PublicServerDetailPage() {
  const { serverId } = Route.useParams()
  const { range: rangeParam } = Route.useSearch()
  const navigate = useNavigate()
  const { t } = useTranslation('status')
  const { data: config } = usePublicStatusConfig()

  // Derive the active window straight from the URL search param so a refresh or
  // shared link lands on the same window without an extra state-sync render.
  const range = rangeParam

  const detailEnabled = config?.show_server_detail !== false
  const { data, isLoading, error } = useQuery({
    queryKey: ['public-status', 'server', serverId],
    queryFn: () => api.get<PublicServerDetail>(`/api/status/servers/${serverId}`),
    refetchInterval: 30_000,
    enabled: detailEnabled && serverId.length > 0,
    retry: false
  })

  useEffect(() => {
    if (config && config.show_server_detail === false) {
      navigate({ to: '/status', replace: true })
    }
  }, [config, navigate])

  if (config?.show_server_detail === false) {
    return null
  }

  if (isLoading) {
    return (
      <div className="space-y-6">
        <Skeleton className="h-8 w-48" />
        <Skeleton className="h-4 w-96" />
        <div className="grid gap-4 lg:grid-cols-2">
          <Skeleton className="h-64" />
          <Skeleton className="h-64" />
        </div>
      </div>
    )
  }

  if (error || !data) {
    return (
      <div className="flex min-h-[400px] items-center justify-center">
        <p className="text-muted-foreground">{t('detail_not_found')}</p>
      </div>
    )
  }

  return (
    <div className="pb-6">
      <div className="mb-6">
        <Link
          className="mb-3 inline-flex items-center gap-1 text-muted-foreground text-sm hover:text-foreground"
          to="/status"
        >
          <ArrowLeft aria-hidden="true" className="size-4" />
          {t('detail_back')}
        </Link>

        <div className="flex items-center gap-3">
          <CountryFlag className="text-xl" code={data.country_code} />
          <h1 className="font-bold text-2xl">{data.name}</h1>
          <StatusBadge status={data.online ? 'online' : 'offline'} />
        </div>

        <PublicServerInfoMeta server={data} />
      </div>

      <ServerDetailContent
        onRangeChange={(rangeKey) => {
          navigate({
            to: '/status/server/$serverId',
            params: { serverId },
            search: { range: rangeKey },
            replace: true
          })
        }}
        rangeKey={range}
        server={data}
        serverId={serverId}
        variant="public"
      />
    </div>
  )
}

function PublicServerInfoMeta({ server }: { server: PublicServerDetail }) {
  const { t } = useTranslation('servers')
  return (
    <div className="mt-2 flex flex-wrap gap-x-4 gap-y-1 text-muted-foreground text-sm">
      {server.os && (
        <span>
          {t('detail_os')} {server.os}
        </span>
      )}
      {server.cpu_name && (
        <span>
          {t('detail_cpu')} {server.cpu_name}
          {server.cpu_cores != null && ` (${t('detail_cores', { count: server.cpu_cores })})`}
          {server.cpu_arch && ` ${server.cpu_arch}`}
        </span>
      )}
      {server.mem_total != null && (
        <span>
          {t('detail_ram')} {formatBytes(server.mem_total)}
        </span>
      )}
      {server.kernel_version && (
        <span>
          {t('detail_kernel_label')} {server.kernel_version}
        </span>
      )}
      {server.region && (
        <span>
          {t('detail_region_label')} {server.region}
        </span>
      )}
      {server.agent_version && <span>{t('detail_agent_label', { version: server.agent_version })}</span>}
      {server.process_count != null && (
        <span>
          {t('detail_processes', { defaultValue: 'Processes' })}: {server.process_count}
        </span>
      )}
      {(server.tcp_conn != null || server.udp_conn != null) && (
        <span>
          {t('detail_connections', { defaultValue: 'Connections' })}: TCP {server.tcp_conn ?? 0} · UDP{' '}
          {server.udp_conn ?? 0}
        </span>
      )}
    </div>
  )
}

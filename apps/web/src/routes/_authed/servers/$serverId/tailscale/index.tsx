import { useQuery } from '@tanstack/react-query'
import { createFileRoute, Link } from '@tanstack/react-router'
import { ArrowLeft, RefreshCw, Share2 } from 'lucide-react'
import { useTranslation } from 'react-i18next'
import { PageBody } from '@/components/layout/page-body'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Skeleton } from '@/components/ui/skeleton'
import { api } from '@/lib/api-client'
import type { ServerTailscaleStatusResponse } from '@/lib/api-schema'
import { CAP_TAILSCALE, getEffectiveCapabilityEnabled } from '@/lib/capabilities'
import { useServerDetail } from '@/lib/server-catalog'
import { cn } from '@/lib/utils'
import { TailscaleOverviewCards } from './components/tailscale-overview-cards'
import { TailscalePeersTable } from './components/tailscale-peers-table'

export const Route = createFileRoute('/_authed/servers/$serverId/tailscale/')({
  component: ServerTailscalePage
})

export function ServerTailscalePage() {
  const { t } = useTranslation(['tailscale', 'common'])
  const { serverId } = Route.useParams()
  const { data: serverDetail, isLoading: serverLoading } = useServerDetail(serverId)

  const capabilityEnabled = getEffectiveCapabilityEnabled(
    serverDetail?.effective_capabilities,
    serverDetail?.capabilities,
    CAP_TAILSCALE
  )

  const {
    data: tailscaleData,
    isLoading: statusLoading,
    isFetching,
    refetch
  } = useQuery<ServerTailscaleStatusResponse>({
    queryKey: ['tailscale', 'status', serverId],
    queryFn: () => api.get<ServerTailscaleStatusResponse>(`/api/servers/${serverId}/tailscale/status`),
    enabled: capabilityEnabled,
    refetchInterval: 30_000,
    staleTime: 10_000
  })

  const hasTailscaleData = Boolean(
    tailscaleData &&
      (tailscaleData.available ||
        tailscaleData.status != null ||
        Boolean(tailscaleData.tailscale_ipv4) ||
        Boolean(tailscaleData.tailscale_dns_name))
  )

  return (
    <PageBody>
      <div>
        <div className="mb-6">
          <Link
            className="mb-3 inline-flex items-center gap-1 text-muted-foreground text-sm hover:text-foreground"
            params={{ id: serverId }}
            search={{ range: 'realtime' }}
            to="/servers/$id"
          >
            <ArrowLeft aria-hidden="true" className="size-4" />
            {t('tailscale:backToServer')}
          </Link>

          <div className="flex flex-wrap items-center gap-3">
            <Share2 aria-hidden="true" className="size-6" />
            <h1 className="font-bold text-2xl">
              {serverDetail?.name ? `${serverDetail.name} · ${t('tailscale:title')}` : t('tailscale:title')}
            </h1>
            {tailscaleData?.cached && (
              <Badge
                className="border-amber-500/30 bg-amber-500/10 text-amber-600 dark:text-amber-400"
                title={t('tailscale:cachedDescription')}
                variant="outline"
              >
                {t('tailscale:cachedBadge')}
              </Badge>
            )}
            {capabilityEnabled && (
              <div className="ml-auto flex items-center gap-2">
                <Button disabled={isFetching} onClick={() => refetch()} size="sm" type="button" variant="outline">
                  <RefreshCw aria-hidden="true" className={cn('mr-1.5 size-4', isFetching && 'animate-spin')} />
                  {t('tailscale:refresh')}
                </Button>
              </div>
            )}
          </div>
        </div>

        {!(serverLoading || capabilityEnabled) && (
          <div className="flex min-h-[300px] items-center justify-center rounded-lg border border-dashed p-6">
            <div className="max-w-md text-center">
              <Share2 aria-hidden="true" className="mx-auto mb-3 size-10 text-muted-foreground" />
              <p className="font-medium text-sm">{t('common:capability_disabled_title')}</p>
              <p className="mt-1 text-muted-foreground text-xs">{t('common:capability_disabled_desc')}</p>
            </div>
          </div>
        )}

        {(serverLoading || (capabilityEnabled && statusLoading)) && (
          <div className="space-y-6">
            <div className="grid gap-4 sm:grid-cols-2 xl:grid-cols-5">
              <Skeleton className="h-28 w-full" />
              <Skeleton className="h-28 w-full" />
              <Skeleton className="h-28 w-full" />
              <Skeleton className="h-28 w-full" />
              <Skeleton className="h-28 w-full" />
            </div>
            <Skeleton className="h-64 w-full" />
          </div>
        )}

        {!serverLoading && capabilityEnabled && !statusLoading && !hasTailscaleData && (
          <div className="flex min-h-[300px] items-center justify-center rounded-lg border border-dashed p-6">
            <div className="max-w-md text-center">
              <Share2 aria-hidden="true" className="mx-auto mb-3 size-10 text-muted-foreground" />
              <p className="text-muted-foreground text-sm">{t('tailscale:notAvailable.title')}</p>
              <p className="mt-1 text-muted-foreground text-xs">{t('tailscale:notAvailable.description')}</p>
            </div>
          </div>
        )}

        {!serverLoading && capabilityEnabled && !statusLoading && tailscaleData && hasTailscaleData && (
          <div className="space-y-6">
            {tailscaleData.cached && (
              <p className="rounded-md border border-amber-500/30 bg-amber-500/5 px-3 py-2 text-amber-700 text-xs dark:text-amber-300">
                {t('tailscale:cachedDescription')}
              </p>
            )}
            <TailscaleOverviewCards data={tailscaleData} />
            <TailscalePeersTable peers={tailscaleData.status?.peers ?? []} />
          </div>
        )}
      </div>
    </PageBody>
  )
}

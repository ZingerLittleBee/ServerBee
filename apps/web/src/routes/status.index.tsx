import { useQuery } from '@tanstack/react-query'
import { createFileRoute } from '@tanstack/react-router'
import { useTranslation } from 'react-i18next'
import { StatusOverviewGridSkeleton, StatusOverviewListSkeleton } from '@/components/boneyard/page-skeletons'
import { LayoutToggle } from '@/components/status/layout-toggle'
import { ServerSummaryCard } from '@/components/status/server-summary-card'
import { ServerSummaryRow } from '@/components/status/server-summary-row'
import { Table, TableBody, TableHead, TableHeader, TableRow } from '@/components/ui/table'
import { usePublicStatusConfig } from '@/hooks/use-public-status'
import { useStatusLayout } from '@/hooks/use-status-layout'
import { api } from '@/lib/api-client'
import type { PublicServerSummary, PublicStatusConfig } from '@/lib/api-schema'

export const Route = createFileRoute('/status/')({
  component: PublicStatusIndex
})

const DEFAULT_THRESHOLDS: Pick<PublicStatusConfig, 'uptime_red_threshold' | 'uptime_yellow_threshold'> = {
  uptime_red_threshold: 95,
  uptime_yellow_threshold: 100
}

function PublicStatusIndex() {
  const { t } = useTranslation('status')
  const { data: config } = usePublicStatusConfig()
  const enabled = config?.enabled !== false

  const {
    data: servers,
    isLoading,
    error
  } = useQuery({
    queryKey: ['public-status', 'servers'],
    queryFn: () => api.get<PublicServerSummary[]>('/api/status'),
    refetchInterval: 30_000,
    enabled
  })

  const [layout, onLayoutChange] = useStatusLayout(config?.default_layout)

  if (config?.enabled === false) {
    return (
      <div className="flex min-h-[300px] items-center justify-center rounded-lg border border-dashed">
        <p className="text-muted-foreground text-sm">{t('site_disabled_notice')}</p>
      </div>
    )
  }

  if (isLoading) {
    // Same effective layout the loaded page will render, so the skeleton
    // matches the incoming content instead of jumping between layouts.
    return layout === 'list' ? <StatusOverviewListSkeleton /> : <StatusOverviewGridSkeleton />
  }

  if (error || !servers) {
    return (
      <div className="flex min-h-[300px] items-center justify-center rounded-lg border border-dashed">
        <p className="text-destructive text-sm">{t('load_failed')}</p>
      </div>
    )
  }

  if (servers.length === 0) {
    return (
      <div className="flex min-h-[300px] items-center justify-center rounded-lg border border-dashed">
        <p className="text-muted-foreground text-sm">{t('no_servers')}</p>
      </div>
    )
  }

  const clickable = !!config?.show_server_detail
  const thresholds = config ?? DEFAULT_THRESHOLDS

  return (
    <div className="space-y-6">
      <div className="flex items-center justify-end">
        <LayoutToggle onChange={onLayoutChange} value={layout} />
      </div>

      {layout === 'grid' ? (
        <div className="grid gap-4" style={{ gridTemplateColumns: 'repeat(auto-fill, minmax(320px, 1fr))' }}>
          {servers.map((s) => (
            <div className="[contain-intrinsic-size:auto_280px] [content-visibility:auto]" key={s.id}>
              <ServerSummaryCard clickable={clickable} server={s} />
            </div>
          ))}
        </div>
      ) : (
        <div className="overflow-hidden rounded-md border">
          <Table className="min-w-[1120px]">
            <TableHeader>
              <TableRow>
                <TableHead className="min-w-[220px]">{t('nav_servers')}</TableHead>
                <TableHead className="w-[180px]">{t('cpu')}</TableHead>
                <TableHead className="w-[180px]">{t('memory')}</TableHead>
                <TableHead className="w-[184px]">{t('disk')}</TableHead>
                <TableHead className="hidden w-[184px] lg:table-cell">{t('network_in')}</TableHead>
                <TableHead className="hidden w-[220px] xl:table-cell">{t('uptime')}</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {servers.map((s) => (
                <ServerSummaryRow clickable={clickable} key={s.id} server={s} thresholds={thresholds} />
              ))}
            </TableBody>
          </Table>
        </div>
      )}
    </div>
  )
}

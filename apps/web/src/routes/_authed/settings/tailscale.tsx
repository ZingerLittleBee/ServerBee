import { useQuery } from '@tanstack/react-query'
import { createFileRoute } from '@tanstack/react-router'
import { RefreshCw } from 'lucide-react'
import { useTranslation } from 'react-i18next'
import { PageBody } from '@/components/layout/page-body'
import { Button } from '@/components/ui/button'
import { useAuth } from '@/hooks/use-auth'
import { api } from '@/lib/api-client'
import type { TailnetOverviewResponse, TailscaleAdminConfigResponse } from '@/lib/api-schema'
import { cn } from '@/lib/utils'
import { TailscaleConfigSection } from './tailscale-config-section'
import { TailscaleDevicesSection } from './tailscale-devices-section'

export const Route = createFileRoute('/_authed/settings/tailscale')({
  component: TailscaleSettingsPage
})

export function TailscaleSettingsPage() {
  const { t } = useTranslation('tailscale')
  const { user } = useAuth()
  const isAdmin = user?.role === 'admin'

  const { data: config, isLoading: configLoading } = useQuery<TailscaleAdminConfigResponse>({
    queryKey: ['tailscale', 'config'],
    queryFn: () => api.get<TailscaleAdminConfigResponse>('/api/tailscale/config'),
    enabled: isAdmin,
    staleTime: 30_000
  })

  const {
    data: overview,
    isLoading: overviewLoading,
    isFetching: overviewFetching,
    refetch: refetchOverview
  } = useQuery<TailnetOverviewResponse>({
    queryKey: ['tailscale', 'overview'],
    queryFn: () => api.get<TailnetOverviewResponse>('/api/tailscale/overview'),
    staleTime: 30_000
  })

  return (
    <PageBody>
      <div className="w-full min-w-0 max-w-[calc(100vw-1.5rem)] space-y-6 sm:max-w-full">
        <div className="flex flex-wrap items-center justify-between gap-3">
          <p className="min-w-0 text-muted-foreground text-sm">{t('settings.subtitle')}</p>
          <Button
            disabled={overviewFetching}
            onClick={() => refetchOverview()}
            size="sm"
            type="button"
            variant="outline"
          >
            <RefreshCw aria-hidden="true" className={cn('mr-1.5 size-4', overviewFetching && 'animate-spin')} />
            {t('refresh')}
          </Button>
        </div>

        {isAdmin && <TailscaleConfigSection config={config} isLoading={configLoading} />}

        <TailscaleDevicesSection isAdmin={isAdmin} isLoading={overviewLoading} overview={overview} />
      </div>
    </PageBody>
  )
}

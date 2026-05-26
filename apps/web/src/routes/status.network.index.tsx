import { useQuery } from '@tanstack/react-query'
import { createFileRoute, useNavigate } from '@tanstack/react-router'
import { useEffect, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { NetworkOverviewContent } from '@/components/status/network-overview-content'
import { usePublicStatusConfig } from '@/hooks/use-public-status'
import { api } from '@/lib/api-client'
import type { PublicNetworkOverview } from '@/lib/api-schema'

export const Route = createFileRoute('/status/network/')({
  component: PublicNetworkOverviewPage
})

function PublicNetworkOverviewPage() {
  const { t } = useTranslation('network')
  const { data: config } = usePublicStatusConfig()
  const navigate = useNavigate()
  const [search, setSearch] = useState('')

  const networkEnabled = config?.show_network !== false
  const { data, isLoading, error } = useQuery({
    queryKey: ['public-status', 'network', 'overview'],
    queryFn: () => api.get<PublicNetworkOverview>('/api/status/network'),
    refetchInterval: 30_000,
    enabled: networkEnabled,
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

  if (error) {
    return (
      <div className="flex min-h-[300px] items-center justify-center rounded-lg border border-dashed">
        <p className="text-destructive text-sm">{t('no_data')}</p>
      </div>
    )
  }

  return (
    <NetworkOverviewContent
      data={data?.servers ?? []}
      isLoading={isLoading}
      onSearchChange={setSearch}
      search={search}
      variant="public"
    />
  )
}

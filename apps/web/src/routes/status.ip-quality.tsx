import { useQuery } from '@tanstack/react-query'
import { createFileRoute, useNavigate } from '@tanstack/react-router'
import { useEffect, useMemo } from 'react'
import { useTranslation } from 'react-i18next'
import { IpQualityContent } from '@/components/status/ip-quality-content'
import { usePublicStatusConfig } from '@/hooks/use-public-status'
import { api } from '@/lib/api-client'
import type { PublicIpQualityOverview, PublicServerSummary } from '@/lib/api-schema'

export const Route = createFileRoute('/status/ip-quality')({
  component: PublicIpQualityPage
})

function PublicIpQualityPage() {
  const { t } = useTranslation('ip-quality')
  const { data: config } = usePublicStatusConfig()
  const navigate = useNavigate()

  const ipQualityEnabled = config?.show_ip_quality !== false

  const { data, isLoading, error } = useQuery({
    queryKey: ['public-status', 'ip-quality'],
    queryFn: () => api.get<PublicIpQualityOverview>('/api/status/ip-quality'),
    refetchInterval: 30_000,
    enabled: ipQualityEnabled,
    retry: false
  })

  // Separate query for server labels (the public IP-quality DTO carries only ids).
  const { data: servers = [] } = useQuery({
    queryKey: ['public-status', 'servers'],
    queryFn: () => api.get<PublicServerSummary[]>('/api/status'),
    refetchInterval: 60_000,
    enabled: ipQualityEnabled,
    retry: false
  })

  const serverNames = useMemo(() => new Map(servers.map((s) => [s.id, s.name])), [servers])

  useEffect(() => {
    if (config && config.show_ip_quality === false) {
      navigate({ to: '/status', replace: true })
    }
  }, [config, navigate])

  if (config?.show_ip_quality === false) {
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
    <IpQualityContent
      data={data ?? { entries: [], services: [] }}
      isLoading={isLoading}
      serverNames={serverNames}
      variant="public"
    />
  )
}

import { createFileRoute } from '@tanstack/react-router'
import { PageBody } from '@/components/layout/page-body'
import { IpQualityContent } from '@/components/status/ip-quality-content'
import { useIpQualityOverview, useIpQualityServices } from '@/hooks/use-ip-quality-api'
import { useServerList } from '@/lib/server-catalog'

export const Route = createFileRoute('/_authed/ip-quality')({
  component: IpQualityOverviewPage
})

function IpQualityOverviewPage() {
  const { data: overview = [], isLoading: overviewLoading } = useIpQualityOverview()
  const { data: services = [], isLoading: servicesLoading } = useIpQualityServices()

  const { data: servers = [], isLoading: serversLoading } = useServerList()

  const isLoading = overviewLoading || servicesLoading || serversLoading

  return (
    <PageBody>
      <IpQualityContent
        isLoading={isLoading}
        overview={overview}
        servers={servers}
        services={services}
        variant="admin"
      />
    </PageBody>
  )
}

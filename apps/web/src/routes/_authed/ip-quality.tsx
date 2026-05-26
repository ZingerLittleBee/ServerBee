import { useQuery } from '@tanstack/react-query'
import { createFileRoute } from '@tanstack/react-router'
import { IpQualityContent } from '@/components/status/ip-quality-content'
import { useIpQualityOverview, useIpQualityServices } from '@/hooks/use-ip-quality-api'
import { api } from '@/lib/api-client'

export const Route = createFileRoute('/_authed/ip-quality')({
  component: IpQualityOverviewPage
})

interface ServerLite {
  id: string
  name: string
}

function IpQualityOverviewPage() {
  const { data: overview = [], isLoading: overviewLoading } = useIpQualityOverview()
  const { data: services = [], isLoading: servicesLoading } = useIpQualityServices()

  const { data: servers = [], isLoading: serversLoading } = useQuery<ServerLite[]>({
    queryKey: ['servers', 'lite'],
    queryFn: () => api.get<ServerLite[]>('/api/servers')
  })

  const isLoading = overviewLoading || servicesLoading || serversLoading

  return (
    <IpQualityContent isLoading={isLoading} overview={overview} servers={servers} services={services} variant="admin" />
  )
}

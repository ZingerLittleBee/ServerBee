import { createFileRoute } from '@tanstack/react-router'
import { NetworkOverviewContent } from '@/components/status/network-overview-content'
import { useNetworkOverview } from '@/hooks/use-network-api'

export const Route = createFileRoute('/_authed/network/')({
  validateSearch: (search: Record<string, unknown>) => ({
    q: (search.q as string) || ''
  }),
  component: NetworkOverviewPage
})

function NetworkOverviewPage() {
  const { data: summaries = [], isLoading } = useNetworkOverview()
  const { q: search } = Route.useSearch()
  const navigate = Route.useNavigate()

  return (
    <NetworkOverviewContent
      data={summaries}
      isLoading={isLoading}
      onSearchChange={(q) => navigate({ search: { q } })}
      search={search}
      variant="admin"
    />
  )
}

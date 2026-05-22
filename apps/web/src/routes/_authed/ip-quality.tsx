import { useQuery } from '@tanstack/react-query'
import { createFileRoute } from '@tanstack/react-router'
import { ShieldCheck } from 'lucide-react'
import { useMemo } from 'react'
import { IpQualityCard } from '@/components/ip-quality/ip-quality-card'
import { UnlockMatrix } from '@/components/ip-quality/unlock-matrix'
import { ScrollArea } from '@/components/ui/scroll-area'
import { Skeleton } from '@/components/ui/skeleton'
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

  // Only services that are enabled show in the matrix
  const enabledServices = useMemo(() => services.filter((s) => s.enabled), [services])

  // Servers that have any IP quality data (appear in overview), ordered by server name
  const serversWithData = useMemo(() => {
    const overviewIds = new Set(overview.map((o) => o.server_id))
    return servers.filter((s) => overviewIds.has(s.id)).sort((a, b) => a.name.localeCompare(b.name))
  }, [servers, overview])

  // Build a map from server_id to the data object for quick lookup
  const overviewByServerId = useMemo(() => {
    const map = new Map(overview.map((o) => [o.server_id, o]))
    return map
  }, [overview])

  const hasServers = servers.length > 0
  const hasData = serversWithData.length > 0

  return (
    <ScrollArea className="h-full w-full">
      <div className="space-y-6 pr-1 pb-4">
        <div>
          <h1 className="font-bold text-2xl">IP Quality</h1>
          <p className="text-muted-foreground text-sm">Egress IP metadata and service unlock status for each server.</p>
        </div>

        {isLoading && (
          <div className="space-y-3">
            {Array.from({ length: 3 }, (_, i) => (
              <Skeleton className="h-28 rounded-xl" key={`skel-${i.toString()}`} />
            ))}
          </div>
        )}

        {!(isLoading || hasServers) && (
          <div className="flex min-h-[240px] items-center justify-center rounded-xl border border-dashed">
            <div className="space-y-2 text-center">
              <ShieldCheck aria-hidden="true" className="mx-auto size-8 text-muted-foreground" />
              <p className="text-muted-foreground text-sm">No servers found.</p>
            </div>
          </div>
        )}

        {!isLoading && hasServers && !hasData && (
          <div className="flex min-h-[240px] items-center justify-center rounded-xl border border-dashed">
            <div className="space-y-2 text-center">
              <ShieldCheck aria-hidden="true" className="mx-auto size-8 text-muted-foreground" />
              <p className="font-medium text-sm">No IP quality data yet</p>
              <p className="max-w-xs text-muted-foreground text-xs">
                Enable the <span className="font-mono">ip_quality</span> capability on a server and start the agent with{' '}
                <span className="font-mono">--allow-cap ip_quality</span> to begin collecting data.
              </p>
            </div>
          </div>
        )}

        {!isLoading && hasData && (
          <>
            {/* Per-server IP quality cards */}
            <div className="grid grid-cols-1 gap-4 sm:grid-cols-2 lg:grid-cols-3">
              {serversWithData.map((server) => {
                const data = overviewByServerId.get(server.id)
                return <IpQualityCard ipQuality={data?.ip_quality ?? null} key={server.id} serverName={server.name} />
              })}
            </div>

            {/* All-servers unlock matrix */}
            {enabledServices.length > 0 && (
              <div className="space-y-2">
                <h2 className="font-semibold text-base">Unlock Matrix</h2>
                <UnlockMatrix overview={overview} servers={serversWithData} services={enabledServices} />
              </div>
            )}
          </>
        )}
      </div>
    </ScrollArea>
  )
}

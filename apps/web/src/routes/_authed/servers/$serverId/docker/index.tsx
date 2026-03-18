import { useQuery } from '@tanstack/react-query'
import { createFileRoute, Link } from '@tanstack/react-router'
import { ArrowLeft, Container, HardDrive, Network } from 'lucide-react'
import { useState } from 'react'
import { Button } from '@/components/ui/button'
import type { ServerMetrics } from '@/hooks/use-servers-ws'
import { api } from '@/lib/api-client'
import { ContainerDetailDialog } from './components/container-detail-dialog'
import { ContainerList } from './components/container-list'
import { DockerEvents } from './components/docker-events'
import { DockerNetworksDialog } from './components/docker-networks-dialog'
import { DockerOverview } from './components/docker-overview'
import { DockerVolumesDialog } from './components/docker-volumes-dialog'
import { useDockerSubscription } from './hooks/use-docker-subscription'
import type { DockerContainer, DockerContainerStats, DockerEventInfo, DockerSystemInfo } from './types'

export const Route = createFileRoute('/_authed/servers/$serverId/docker/')({
  component: DockerPage
})

function DockerPage() {
  const { serverId } = Route.useParams()
  const [selectedContainer, setSelectedContainer] = useState<DockerContainer | null>(null)
  const [networksOpen, setNetworksOpen] = useState(false)
  const [volumesOpen, setVolumesOpen] = useState(false)

  useDockerSubscription(serverId)

  const { data: liveServers } = useQuery<ServerMetrics[]>({
    queryKey: ['servers'],
    queryFn: () => [],
    staleTime: Number.POSITIVE_INFINITY,
    refetchOnMount: false,
    refetchOnWindowFocus: false
  })
  const liveServer = liveServers?.find((s) => s.id === serverId)
  const dockerAvailable = liveServer?.features?.includes('docker') ?? false

  const { data: dockerInfo } = useQuery<DockerSystemInfo>({
    queryKey: ['docker', 'info', serverId],
    queryFn: () => api.get<DockerSystemInfo>(`/api/servers/${serverId}/docker/info`),
    enabled: dockerAvailable,
    staleTime: 60_000
  })

  const { data: containers } = useQuery<DockerContainer[]>({
    queryKey: ['docker', 'containers', serverId],
    queryFn: () => [],
    staleTime: Number.POSITIVE_INFINITY,
    refetchOnMount: false,
    refetchOnWindowFocus: false
  })

  const { data: stats } = useQuery<DockerContainerStats[]>({
    queryKey: ['docker', 'stats', serverId],
    queryFn: () => [],
    staleTime: Number.POSITIVE_INFINITY,
    refetchOnMount: false,
    refetchOnWindowFocus: false
  })

  const { data: events } = useQuery<DockerEventInfo[]>({
    queryKey: ['docker', 'events', serverId],
    queryFn: () => [],
    staleTime: Number.POSITIVE_INFINITY,
    refetchOnMount: false,
    refetchOnWindowFocus: false
  })

  const hasContainers = containers && containers.length > 0

  return (
    <div>
      <div className="mb-6">
        <Link
          className="mb-3 inline-flex items-center gap-1 text-muted-foreground text-sm hover:text-foreground"
          params={{ id: serverId }}
          search={{ range: 'realtime' }}
          to="/servers/$id"
        >
          <ArrowLeft aria-hidden="true" className="size-4" />
          Back to server
        </Link>

        <div className="flex items-center gap-3">
          <Container aria-hidden="true" className="size-6" />
          <h1 className="font-bold text-2xl">Docker</h1>
          <div className="ml-auto flex items-center gap-2">
            <Button onClick={() => setNetworksOpen(true)} size="sm" variant="outline">
              <Network aria-hidden="true" className="mr-1.5 size-4" />
              Networks
            </Button>
            <Button onClick={() => setVolumesOpen(true)} size="sm" variant="outline">
              <HardDrive aria-hidden="true" className="mr-1.5 size-4" />
              Volumes
            </Button>
          </div>
        </div>
      </div>

      {!dockerAvailable && (
        <div className="flex min-h-[400px] items-center justify-center rounded-lg border border-dashed">
          <div className="text-center">
            <Container aria-hidden="true" className="mx-auto mb-3 size-10 text-muted-foreground" />
            <p className="text-muted-foreground text-sm">Docker is not available</p>
            <p className="mt-1 text-muted-foreground text-xs">Waiting for Docker data from the agent...</p>
          </div>
        </div>
      )}

      {dockerAvailable && !hasContainers && (
        <div className="space-y-6">
          <DockerOverview containers={[]} dockerVersion={dockerInfo?.docker_version} stats={[]} />

          <div className="flex min-h-[200px] items-center justify-center rounded-lg border border-dashed">
            <div className="text-center">
              <Container aria-hidden="true" className="mx-auto mb-3 size-8 text-muted-foreground" />
              <p className="text-muted-foreground text-sm">No containers found</p>
              <p className="mt-1 text-muted-foreground text-xs">Docker is running but no containers are present</p>
            </div>
          </div>

          <DockerEvents events={events ?? []} />
        </div>
      )}

      {dockerAvailable && hasContainers && (
        <div className="space-y-6">
          <DockerOverview containers={containers} dockerVersion={dockerInfo?.docker_version} stats={stats ?? []} />

          <ContainerList containers={containers} onSelect={setSelectedContainer} stats={stats ?? []} />

          <DockerEvents events={events ?? []} />
        </div>
      )}

      <ContainerDetailDialog
        container={selectedContainer}
        onOpenChange={(open) => {
          if (!open) {
            setSelectedContainer(null)
          }
        }}
        open={selectedContainer !== null}
        serverId={serverId}
        stats={stats ?? []}
      />

      <DockerNetworksDialog onOpenChange={setNetworksOpen} open={networksOpen} serverId={serverId} />

      <DockerVolumesDialog onOpenChange={setVolumesOpen} open={volumesOpen} serverId={serverId} />
    </div>
  )
}

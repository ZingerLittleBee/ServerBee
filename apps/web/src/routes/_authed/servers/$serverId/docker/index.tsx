import { useQuery } from '@tanstack/react-query'
import { createFileRoute, Link } from '@tanstack/react-router'
import { ArrowLeft, Container } from 'lucide-react'
import { useState } from 'react'
import { ContainerDetailDialog } from './components/container-detail-dialog'
import { ContainerList } from './components/container-list'
import { DockerEvents } from './components/docker-events'
import { DockerOverview } from './components/docker-overview'
import { useDockerSubscription } from './hooks/use-docker-subscription'
import type { DockerContainer, DockerContainerStats, DockerEventInfo } from './types'

export const Route = createFileRoute('/_authed/servers/$serverId/docker/')({
  component: DockerPage
})

function DockerPage() {
  const { serverId } = Route.useParams()
  const [selectedContainer, setSelectedContainer] = useState<DockerContainer | null>(null)

  useDockerSubscription(serverId)

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

  const hasData = containers && containers.length > 0

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
        </div>
      </div>

      {hasData ? (
        <div className="space-y-6">
          <DockerOverview containers={containers} stats={stats ?? []} />

          <ContainerList containers={containers} onSelect={setSelectedContainer} stats={stats ?? []} />

          <DockerEvents events={events ?? []} />
        </div>
      ) : (
        <div className="flex min-h-[400px] items-center justify-center rounded-lg border border-dashed">
          <div className="text-center">
            <Container aria-hidden="true" className="mx-auto mb-3 size-10 text-muted-foreground" />
            <p className="text-muted-foreground text-sm">Docker is not available</p>
            <p className="mt-1 text-muted-foreground text-xs">Waiting for Docker data from the agent...</p>
          </div>
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
    </div>
  )
}

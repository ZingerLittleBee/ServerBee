import { useQuery } from '@tanstack/react-query'
import { createFileRoute, Link } from '@tanstack/react-router'
import { ArrowLeft, Container } from 'lucide-react'
import { DockerEvents } from './components/docker-events'
import { DockerOverview } from './components/docker-overview'
import { useDockerSubscription } from './hooks/use-docker-subscription'
import type { DockerContainer, DockerContainerStats, DockerEventInfo } from './types'

export const Route = createFileRoute('/_authed/servers/$serverId/docker/')({
  component: DockerPage
})

function DockerPage() {
  const { serverId } = Route.useParams()

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

          {/* TODO: ContainerList component will be created in Task 21 */}
          <div className="space-y-2">
            <h3 className="font-semibold text-lg">Containers</h3>
            <div className="overflow-x-auto rounded-lg border">
              <table className="w-full text-sm">
                <thead>
                  <tr className="border-b bg-muted/50">
                    <th className="px-4 py-2 text-left font-medium">Name</th>
                    <th className="px-4 py-2 text-left font-medium">Image</th>
                    <th className="px-4 py-2 text-left font-medium">State</th>
                    <th className="px-4 py-2 text-left font-medium">Status</th>
                  </tr>
                </thead>
                <tbody>
                  {containers.map((container) => (
                    <tr className="border-b last:border-b-0" key={container.id}>
                      <td className="px-4 py-2 font-medium">{container.name}</td>
                      <td className="px-4 py-2 text-muted-foreground">{container.image}</td>
                      <td className="px-4 py-2">
                        <span
                          className={
                            container.state === 'running'
                              ? 'text-emerald-600 dark:text-emerald-400'
                              : 'text-muted-foreground'
                          }
                        >
                          {container.state}
                        </span>
                      </td>
                      <td className="px-4 py-2 text-muted-foreground">{container.status}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          </div>

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
    </div>
  )
}

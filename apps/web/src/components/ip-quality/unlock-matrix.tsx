import { ScrollArea } from '@/components/ui/scroll-area'
import { categoryLabel, categoryRank } from '@/lib/ip-quality-constants'
import type { ServerIpQualityData, UnlockService, UnlockStatus } from '@/lib/ip-quality-types'
import { cn } from '@/lib/utils'
import { UnlockStatusBadge } from './unlock-status-badge'

interface MatrixServer {
  id: string
  name: string
}

interface Props {
  className?: string
  /** One entry per server with its unlock results. */
  overview: ServerIpQualityData[]
  /** Servers to render as rows, in display order. */
  servers: MatrixServer[]
  /** Catalog of services to render as columns. */
  services: UnlockService[]
}

interface CategoryGroup {
  category: string
  services: UnlockService[]
}

/** Group services by category, ordered by CATEGORY_ORDER, services within a
 *  group sorted by popularity descending. */
function groupServices(services: UnlockService[]): CategoryGroup[] {
  const byCategory = new Map<string, UnlockService[]>()
  for (const svc of services) {
    const list = byCategory.get(svc.category) ?? []
    list.push(svc)
    byCategory.set(svc.category, list)
  }

  return [...byCategory.entries()]
    .sort(([a], [b]) => categoryRank(a) - categoryRank(b) || a.localeCompare(b))
    .map(([category, list]) => ({
      category,
      services: [...list].sort((a, b) => b.popularity - a.popularity || a.name.localeCompare(b.name))
    }))
}

export function UnlockMatrix({ overview, servers, services, className }: Props) {
  const groups = groupServices(services)
  const orderedServices = groups.flatMap((g) => g.services)

  if (orderedServices.length === 0 || servers.length === 0) {
    return (
      <div
        className="rounded-xl bg-card px-4 py-8 text-center text-muted-foreground text-sm ring-1 ring-foreground/10"
        data-testid="matrix-empty"
      >
        No services to display.
      </div>
    )
  }

  // server_id -> (service_id -> status)
  const statusByServer = new Map<string, Map<string, UnlockStatus>>()
  for (const row of overview) {
    const serviceMap = new Map<string, UnlockStatus>()
    for (const r of row.unlock_results) {
      serviceMap.set(r.service_id, r.status as UnlockStatus)
    }
    statusByServer.set(row.server_id, serviceMap)
  }

  return (
    <ScrollArea className={cn('w-full rounded-xl bg-card ring-1 ring-foreground/10', className)}>
      <table className="w-full border-collapse text-sm">
        <thead>
          <tr className="border-b">
            <th className="sticky left-0 z-10 bg-card px-3 py-2 text-left font-medium" rowSpan={2}>
              Server
            </th>
            {groups.map((group) => (
              <th
                className="border-l px-3 py-1.5 text-center font-medium text-muted-foreground text-xs"
                colSpan={group.services.length}
                data-category={group.category}
                data-testid="matrix-category-group"
                key={group.category}
              >
                {categoryLabel(group.category)}
              </th>
            ))}
          </tr>
          <tr className="border-b">
            {groups.map((group) =>
              group.services.map((svc) => (
                <th
                  className="border-l px-3 py-2 text-center font-medium"
                  data-category={group.category}
                  data-service-key={svc.key}
                  data-testid="matrix-service-header"
                  key={svc.id}
                >
                  {svc.name}
                </th>
              ))
            )}
          </tr>
        </thead>
        <tbody>
          {servers.map((server) => {
            const serviceMap = statusByServer.get(server.id)
            return (
              <tr className="border-b last:border-b-0" data-testid={`matrix-row-${server.id}`} key={server.id}>
                <td className="sticky left-0 z-10 bg-card px-3 py-2 font-medium">{server.name}</td>
                {orderedServices.map((svc) => {
                  const status = serviceMap?.get(svc.id)
                  return (
                    <td className="border-l px-3 py-2 text-center" key={svc.id}>
                      {status ? (
                        <UnlockStatusBadge status={status} />
                      ) : (
                        <span className="text-muted-foreground text-xs">—</span>
                      )}
                    </td>
                  )
                })}
              </tr>
            )
          })}
        </tbody>
      </table>
    </ScrollArea>
  )
}

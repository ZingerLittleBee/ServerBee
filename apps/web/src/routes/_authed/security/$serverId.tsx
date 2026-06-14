import { useQuery } from '@tanstack/react-query'
import { createFileRoute, Link } from '@tanstack/react-router'
import { useMemo, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { SecurityEventDetailDrawer } from '@/components/security/event-detail-drawer'
import { SecurityEventTable } from '@/components/security/event-table'
import { SecurityKpiCards } from '@/components/security/kpi-cards'
import { SecurityTimelineChart } from '@/components/security/timeline-chart'
import { Button } from '@/components/ui/button'
import { useSecurityEvents } from '@/hooks/use-security-events'
import { api } from '@/lib/api-client'
import type { SecurityEventDto } from '@/lib/api-schema'

export const Route = createFileRoute('/_authed/security/$serverId')({
  component: SecurityServerPage
})

type RangeKey = '24h' | '7d' | '30d'

interface ServerLite {
  id: string
  name: string
}

const RANGE_HOURS: Record<RangeKey, number> = {
  '24h': 24,
  '7d': 24 * 7,
  '30d': 24 * 30
}

function computeSince(range: RangeKey): string {
  return new Date(Date.now() - RANGE_HOURS[range] * 3600 * 1000).toISOString()
}

function SecurityServerPage() {
  const { serverId } = Route.useParams()
  const { t } = useTranslation('security')
  const [range, setRange] = useState<RangeKey>('7d')
  const [activeEvent, setActiveEvent] = useState<SecurityEventDto | null>(null)

  const since = useMemo(() => computeSince(range), [range])
  const eventsQuery = useSecurityEvents({ server_id: serverId, since, limit: 100 })

  const { data: server } = useQuery<ServerLite>({
    queryKey: ['server', 'lite', serverId],
    queryFn: () => api.get<ServerLite>(`/api/servers/${serverId}`)
  })

  const events = useMemo(() => {
    const out: SecurityEventDto[] = []
    for (const page of eventsQuery.data?.pages ?? []) {
      for (const item of page.items) {
        out.push(item)
      }
    }
    return out
  }, [eventsQuery.data])

  return (
    <div className="space-y-4 p-4">
      <div className="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
        <div className="min-w-0">
          <Button
            className="mb-2 -ml-2"
            nativeButton={false}
            render={<Link to="/security" />}
            size="sm"
            variant="ghost"
          >
            {t('per_server.back', { defaultValue: '← Back to Security' })}
          </Button>
          <h1 className="truncate font-semibold text-2xl">{server?.name ?? serverId}</h1>
          <p className="text-muted-foreground text-sm">
            {t('per_server.subtitle', { defaultValue: 'Security events' })}
          </p>
        </div>
        <div className="flex shrink-0 gap-1 rounded-md border bg-card p-1">
          {(['24h', '7d', '30d'] as const).map((key) => (
            <Button key={key} onClick={() => setRange(key)} size="sm" variant={range === key ? 'default' : 'ghost'}>
              {t(`range.${key}`, { defaultValue: key })}
            </Button>
          ))}
        </div>
      </div>

      <SecurityKpiCards serverId={serverId} since={since} />

      <SecurityTimelineChart events={events} isLoading={eventsQuery.isLoading} />

      <SecurityEventTable
        events={events}
        hasNextPage={eventsQuery.hasNextPage}
        isFetchingNextPage={eventsQuery.isFetchingNextPage}
        isLoading={eventsQuery.isLoading}
        onFetchNextPage={() => eventsQuery.fetchNextPage()}
        onRowClick={(event) => setActiveEvent(event)}
      />

      <SecurityEventDetailDrawer event={activeEvent} onOpenChange={(open) => !open && setActiveEvent(null)} />
    </div>
  )
}

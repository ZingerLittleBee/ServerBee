import { useQuery } from '@tanstack/react-query'
import { createFileRoute } from '@tanstack/react-router'
import { useMemo, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { AddBlockDrawer, type AddBlockInitialValues } from '@/components/firewall/add-block-drawer'
import { SecurityEventDetailDrawer } from '@/components/security/event-detail-drawer'
import { SecurityEventTable } from '@/components/security/event-table'
import { SecurityKpiCards } from '@/components/security/kpi-cards'
import { SecurityTimelineChart } from '@/components/security/timeline-chart'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select'
import { useAuth } from '@/hooks/use-auth'
import { type SecurityEventFilters, useSecurityEvents } from '@/hooks/use-security-events'
import { api } from '@/lib/api-client'
import type { SecurityEventDto } from '@/lib/api-schema'

export const Route = createFileRoute('/_authed/security/')({
  component: SecurityIndexPage
})

type RangeKey = '24h' | '7d' | '30d'

interface ServerSummary {
  id: string
  name: string
}

const RANGE_HOURS: Record<RangeKey, number> = {
  '24h': 24,
  '7d': 24 * 7,
  '30d': 24 * 30
}

function computeSince(range: RangeKey): string {
  const hours = RANGE_HOURS[range]
  return new Date(Date.now() - hours * 3600 * 1000).toISOString()
}

function SecurityIndexPage() {
  const { t } = useTranslation('security')
  const [range, setRange] = useState<RangeKey>('24h')
  const [serverId, setServerId] = useState<string>('')
  const [eventType, setEventType] = useState<string>('')
  const [severity, setSeverity] = useState<string>('')
  const [sourceIp, setSourceIp] = useState<string>('')
  const [firstSeenOnly, setFirstSeenOnly] = useState(false)
  const [activeEvent, setActiveEvent] = useState<SecurityEventDto | null>(null)
  const [blockOpen, setBlockOpen] = useState(false)
  const [blockInitial, setBlockInitial] = useState<AddBlockInitialValues | undefined>(undefined)
  const { user } = useAuth()
  const isAdmin = user?.role === 'admin'

  const since = useMemo(() => computeSince(range), [range])

  const filters: SecurityEventFilters = useMemo(
    () => ({
      server_id: serverId || null,
      event_type: eventType || null,
      severity: severity || null,
      source_ip: sourceIp || null,
      since,
      limit: 100
    }),
    [serverId, eventType, severity, sourceIp, since]
  )

  const eventsQuery = useSecurityEvents(filters)

  const { data: servers } = useQuery<ServerSummary[]>({
    queryKey: ['servers', 'lite'],
    queryFn: () => api.get<ServerSummary[]>('/api/servers')
  })

  const allEvents = useMemo(() => {
    const list: SecurityEventDto[] = []
    for (const page of eventsQuery.data?.pages ?? []) {
      for (const item of page.items) {
        list.push(item)
      }
    }
    return firstSeenOnly ? list.filter((event) => event.first_seen) : list
  }, [eventsQuery.data, firstSeenOnly])

  return (
    <div className="space-y-4 p-4">
      <div className="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
        <h1 className="font-semibold text-2xl">{t('page_title', { defaultValue: 'Security Events' })}</h1>
        <div className="flex gap-1 rounded-md border bg-card p-1">
          {(['24h', '7d', '30d'] as const).map((key) => (
            <Button key={key} onClick={() => setRange(key)} size="sm" variant={range === key ? 'default' : 'ghost'}>
              {t(`range.${key}`, { defaultValue: key })}
            </Button>
          ))}
        </div>
      </div>

      <SecurityKpiCards serverId={filters.server_id} since={since} />

      <div className="flex flex-wrap gap-2 rounded-md border bg-card p-3">
        <Select
          items={[
            { value: '__all__', label: t('filter.server_all', { defaultValue: 'All servers' }) },
            ...(servers ?? []).map((s) => ({ value: s.id, label: s.name }))
          ]}
          onValueChange={(v) => setServerId(v === '__all__' ? '' : (v ?? ''))}
          value={serverId || '__all__'}
        >
          <SelectTrigger className="h-9 w-[180px]">
            <SelectValue placeholder={t('filter.server', { defaultValue: 'All servers' })} />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value="__all__">{t('filter.server_all', { defaultValue: 'All servers' })}</SelectItem>
            {servers?.map((s) => (
              <SelectItem key={s.id} value={s.id}>
                {s.name}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>

        <Select
          items={[
            { value: '__all__', label: t('filter.event_type_all', { defaultValue: 'All types' }) },
            { value: 'ssh_brute_force', label: t('event_type.ssh_brute_force', { defaultValue: 'SSH Brute Force' }) },
            { value: 'port_scan', label: t('event_type.port_scan', { defaultValue: 'Port Scan' }) },
            { value: 'ssh_login', label: t('event_type.ssh_login', { defaultValue: 'SSH Login' }) }
          ]}
          onValueChange={(v) => setEventType(v === '__all__' ? '' : (v ?? ''))}
          value={eventType || '__all__'}
        >
          <SelectTrigger className="h-9 w-[180px]">
            <SelectValue placeholder={t('filter.event_type', { defaultValue: 'All types' })} />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value="__all__">{t('filter.event_type_all', { defaultValue: 'All types' })}</SelectItem>
            <SelectItem value="ssh_brute_force">
              {t('event_type.ssh_brute_force', { defaultValue: 'SSH Brute Force' })}
            </SelectItem>
            <SelectItem value="port_scan">{t('event_type.port_scan', { defaultValue: 'Port Scan' })}</SelectItem>
            <SelectItem value="ssh_login">{t('event_type.ssh_login', { defaultValue: 'SSH Login' })}</SelectItem>
          </SelectContent>
        </Select>

        <Select
          items={[
            { value: '__all__', label: t('filter.severity_all', { defaultValue: 'All severities' }) },
            { value: 'critical', label: t('severity.critical', { defaultValue: 'Critical' }) },
            { value: 'high', label: t('severity.high', { defaultValue: 'High' }) },
            { value: 'medium', label: t('severity.medium', { defaultValue: 'Medium' }) },
            { value: 'low', label: t('severity.low', { defaultValue: 'Low' }) }
          ]}
          onValueChange={(v) => setSeverity(v === '__all__' ? '' : (v ?? ''))}
          value={severity || '__all__'}
        >
          <SelectTrigger className="h-9 w-[160px]">
            <SelectValue placeholder={t('filter.severity', { defaultValue: 'All severities' })} />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value="__all__">{t('filter.severity_all', { defaultValue: 'All severities' })}</SelectItem>
            <SelectItem value="critical">{t('severity.critical', { defaultValue: 'Critical' })}</SelectItem>
            <SelectItem value="high">{t('severity.high', { defaultValue: 'High' })}</SelectItem>
            <SelectItem value="medium">{t('severity.medium', { defaultValue: 'Medium' })}</SelectItem>
            <SelectItem value="low">{t('severity.low', { defaultValue: 'Low' })}</SelectItem>
          </SelectContent>
        </Select>

        <Input
          aria-label={t('filter.source_ip', { defaultValue: 'Source IP' })}
          className="w-[180px]"
          onChange={(e) => setSourceIp(e.target.value)}
          placeholder={t('filter.source_ip', { defaultValue: 'Source IP' })}
          value={sourceIp}
        />

        <label className="flex items-center gap-2 text-muted-foreground text-sm">
          <input
            checked={firstSeenOnly}
            className="accent-primary"
            onChange={(e) => setFirstSeenOnly(e.target.checked)}
            type="checkbox"
          />
          {t('filter.first_seen_only', { defaultValue: 'First-seen only' })}
        </label>
      </div>

      <SecurityTimelineChart events={allEvents} isLoading={eventsQuery.isLoading} />

      <SecurityEventTable
        events={allEvents}
        hasNextPage={eventsQuery.hasNextPage}
        isFetchingNextPage={eventsQuery.isFetchingNextPage}
        isLoading={eventsQuery.isLoading}
        onBlockSourceIp={
          isAdmin
            ? (event) => {
                setBlockInitial({ target: event.source_ip, cover_type: 'all' })
                setBlockOpen(true)
              }
            : undefined
        }
        onFetchNextPage={() => eventsQuery.fetchNextPage()}
        onRowClick={(event) => setActiveEvent(event)}
        onSourceIpClick={(ip) => setSourceIp(ip)}
      />

      <SecurityEventDetailDrawer event={activeEvent} onOpenChange={(open) => !open && setActiveEvent(null)} />
      <AddBlockDrawer initialValues={blockInitial} onOpenChange={setBlockOpen} open={blockOpen} />
    </div>
  )
}

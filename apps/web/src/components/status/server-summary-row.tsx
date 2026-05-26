import { Link } from '@tanstack/react-router'
import { Wrench } from 'lucide-react'
import type { ReactNode } from 'react'
import { useTranslation } from 'react-i18next'
import { Badge } from '@/components/ui/badge'
import { UptimeTimeline } from '@/components/uptime/uptime-timeline'
import type { PublicServerSummary, PublicStatusConfig } from '@/lib/api-schema'
import { cn } from '@/lib/utils'
import { computeAggregateUptime } from '@/lib/widget-helpers'

interface Props {
  clickable: boolean
  server: PublicServerSummary
  thresholds: Pick<PublicStatusConfig, 'uptime_red_threshold' | 'uptime_yellow_threshold'>
}

function ServerStatusDot({
  inMaintenance,
  online,
  t
}: {
  inMaintenance: boolean
  online: boolean
  t: (key: string) => string
}) {
  if (inMaintenance) {
    return <span className="inline-block size-3 rounded-full bg-blue-500" title={t('maintenance')} />
  }
  if (online) {
    return <span className="inline-block size-3 rounded-full bg-emerald-500" title={t('online_status')} />
  }
  return <span className="inline-block size-3 rounded-full bg-gray-400" title={t('offline_status')} />
}

export function ServerSummaryRow({ server, clickable, thresholds }: Props) {
  const { t } = useTranslation('status')
  const uptimePct = computeAggregateUptime(server.uptime_daily)

  const inner: ReactNode = (
    <div
      className={cn(
        'flex items-center gap-3 border-b px-4 py-3 last:border-b-0',
        clickable && 'transition-colors hover:bg-accent/40'
      )}
    >
      <ServerStatusDot inMaintenance={server.in_maintenance} online={server.online} t={t} />
      <span className="min-w-0 flex-1 truncate font-medium text-sm">{server.name}</span>
      {server.group_name && (
        <Badge className="shrink-0" variant="outline">
          {server.group_name}
        </Badge>
      )}
      {server.region && (
        <span className="hidden shrink-0 text-muted-foreground text-xs md:inline">{server.region}</span>
      )}
      {server.in_maintenance && (
        <Badge className="shrink-0" variant="secondary">
          <Wrench className="mr-1 size-3" />
          {t('maintenance')}
        </Badge>
      )}
      <div className="hidden w-64 sm:block">
        <UptimeTimeline
          days={server.uptime_daily}
          height={20}
          rangeDays={90}
          redThreshold={thresholds.uptime_red_threshold}
          yellowThreshold={thresholds.uptime_yellow_threshold}
        />
      </div>
      <span className="w-16 shrink-0 text-right text-muted-foreground text-xs">
        {uptimePct !== null ? `${uptimePct.toFixed(1)}%` : '—'}
      </span>
    </div>
  )

  if (clickable) {
    return (
      <Link className="block" params={{ serverId: server.id }} to="/status/server/$serverId">
        {inner}
      </Link>
    )
  }
  return inner
}

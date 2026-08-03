import { useMemo } from 'react'
import { useTranslation } from 'react-i18next'
import { StackedBarPlot, type StackedBarSeries } from '@/components/charts/stacked-bar-plot'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import { Skeleton } from '@/components/ui/skeleton'
import type { SecurityEventDto } from '@/lib/api-schema'

interface Props {
  events: SecurityEventDto[]
  isLoading?: boolean
}

interface TimelinePoint extends Record<string, unknown> {
  day: string
  port_scan: number
  ssh_brute_force: number
  ssh_login: number
}

function formatEventCount(value: number): string {
  return String(value)
}

/** Mirrors Recharts `allowDecimals={false}` — event counts are whole numbers. */
function formatEventCountTick(value: number): string {
  return Number.isInteger(value) ? String(value) : ''
}

function toDay(iso: string): string {
  const d = new Date(iso)
  if (Number.isNaN(d.getTime())) {
    return iso.slice(0, 10)
  }
  return d.toISOString().slice(0, 10)
}

export function SecurityTimelineChart({ events, isLoading }: Props) {
  const { t } = useTranslation('security')

  const series = useMemo<StackedBarSeries[]>(
    () => [
      { dataKey: 'ssh_brute_force', label: t('event_type.ssh_brute_force'), color: 'var(--chart-1, #dc2626)' },
      { dataKey: 'port_scan', label: t('event_type.port_scan'), color: 'var(--chart-2, #ea580c)' },
      { dataKey: 'ssh_login', label: t('event_type.ssh_login'), color: 'var(--chart-3, #2563eb)' }
    ],
    [t]
  )

  const data = useMemo<TimelinePoint[]>(() => {
    const buckets = new Map<string, TimelinePoint>()
    for (const event of events) {
      const day = toDay(event.created_at)
      const existing = buckets.get(day) ?? {
        day,
        port_scan: 0,
        ssh_brute_force: 0,
        ssh_login: 0
      }
      if (event.event_type === 'ssh_brute_force') {
        existing.ssh_brute_force += 1
      } else if (event.event_type === 'port_scan') {
        existing.port_scan += 1
      } else if (event.event_type === 'ssh_login') {
        existing.ssh_login += 1
      }
      buckets.set(day, existing)
    }
    return Array.from(buckets.values()).sort((a, b) => a.day.localeCompare(b.day))
  }, [events])

  let body: React.ReactNode
  if (isLoading) {
    body = <Skeleton className="h-[240px] w-full" />
  } else if (data.length === 0) {
    body = <p className="py-10 text-center text-muted-foreground text-sm">{t('timeline.empty')}</p>
  } else {
    body = (
      <StackedBarPlot
        ariaLabel={t('timeline.title')}
        categoryKey="day"
        categoryLabel={t('timeline.date_label')}
        className="h-[240px] w-full"
        data={data}
        formatAxisValue={formatEventCountTick}
        formatTooltipLabel={(day) => day}
        formatValue={formatEventCount}
        marginLeft={44}
        series={series}
      />
    )
  }

  return (
    <Card>
      <CardHeader>
        <CardTitle>{t('timeline.title')}</CardTitle>
      </CardHeader>
      <CardContent>{body}</CardContent>
    </Card>
  )
}

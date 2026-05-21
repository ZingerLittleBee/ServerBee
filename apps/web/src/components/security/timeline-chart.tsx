import { useMemo } from 'react'
import { useTranslation } from 'react-i18next'
import { Bar, BarChart, CartesianGrid, XAxis, YAxis } from 'recharts'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import {
  type ChartConfig,
  ChartContainer,
  ChartLegend,
  ChartLegendContent,
  ChartTooltip,
  ChartTooltipContent
} from '@/components/ui/chart'
import { Skeleton } from '@/components/ui/skeleton'
import type { SecurityEventDto } from '@/lib/api-schema'

interface Props {
  events: SecurityEventDto[]
  isLoading?: boolean
}

interface TimelinePoint {
  day: string
  port_scan: number
  ssh_brute_force: number
  ssh_login: number
}

const chartConfig = {
  ssh_brute_force: {
    label: 'Brute Force',
    color: 'var(--chart-1, #dc2626)'
  },
  port_scan: {
    label: 'Port Scan',
    color: 'var(--chart-2, #ea580c)'
  },
  ssh_login: {
    label: 'SSH Login',
    color: 'var(--chart-3, #2563eb)'
  }
} satisfies ChartConfig

function toDay(iso: string): string {
  const d = new Date(iso)
  if (Number.isNaN(d.getTime())) {
    return iso.slice(0, 10)
  }
  return d.toISOString().slice(0, 10)
}

export function SecurityTimelineChart({ events, isLoading }: Props) {
  const { t } = useTranslation('security')

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
    body = (
      <p className="py-10 text-center text-muted-foreground text-sm">
        {t('chart.empty', { defaultValue: 'No data to display' })}
      </p>
    )
  } else {
    body = (
      <ChartContainer className="h-[240px] w-full" config={chartConfig}>
        <BarChart accessibilityLayer data={data} maxBarSize={40}>
          <CartesianGrid vertical={false} />
          <XAxis axisLine={false} dataKey="day" tickLine={false} tickMargin={8} />
          <YAxis allowDecimals={false} axisLine={false} tickLine={false} width={40} />
          <ChartTooltip content={<ChartTooltipContent />} cursor={false} />
          <ChartLegend content={<ChartLegendContent />} />
          <Bar dataKey="ssh_brute_force" fill="var(--color-ssh_brute_force)" stackId="events" />
          <Bar dataKey="port_scan" fill="var(--color-port_scan)" stackId="events" />
          <Bar dataKey="ssh_login" fill="var(--color-ssh_login)" stackId="events" />
        </BarChart>
      </ChartContainer>
    )
  }

  return (
    <Card>
      <CardHeader>
        <CardTitle>{t('chart.title', { defaultValue: 'Events over time' })}</CardTitle>
      </CardHeader>
      <CardContent>{body}</CardContent>
    </Card>
  )
}

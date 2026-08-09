import { useTranslation } from 'react-i18next'
import { StackedBarPlot, type StackedBarSeries } from '@/components/charts/stacked-bar-plot'
import { Card, CardAction, CardContent, CardFooter, CardHeader, CardTitle } from '@/components/ui/card'
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs'
import { useTraffic } from '@/hooks/use-traffic'
import { formatBytes } from '@/lib/utils'

const trafficSeries: StackedBarSeries[] = [
  { dataKey: 'bytes_in', label: '↓ In', color: 'var(--chart-1)' },
  { dataKey: 'bytes_out', label: '↑ Out', color: 'var(--chart-2)' }
]

function formatHourTick(hour: string): string {
  const d = new Date(hour)
  return `${d.getHours().toString().padStart(2, '0')}:00`
}

function formatDayTick(date: string): string {
  return date.slice(5)
}

function HourlyTrafficChart({ data, t }: { data: Record<string, unknown>[]; t: (key: string) => string }) {
  return (
    <StackedBarPlot
      ariaLabel={t('traffic_hourly')}
      categoryKey="hour"
      categoryLabel={t('traffic_chart_hour')}
      className="h-[260px] w-full"
      data={data}
      formatCategory={formatHourTick}
      formatTooltipLabel={formatHourTick}
      formatValue={formatBytes}
      series={trafficSeries}
    />
  )
}

function DailyTrafficChart({ data, t }: { data: Record<string, unknown>[]; t: (key: string) => string }) {
  return (
    <StackedBarPlot
      ariaLabel={t('traffic_daily')}
      categoryKey="date"
      categoryLabel={t('traffic_chart_date')}
      className="h-[260px] w-full"
      data={data}
      formatCategory={formatDayTick}
      formatTooltipLabel={(date) => date}
      formatValue={formatBytes}
      series={trafficSeries}
    />
  )
}

export function TrafficCard({ serverId }: { serverId: string }) {
  const { t } = useTranslation('servers')
  const { data, isLoading } = useTraffic(serverId)
  const hourly = data?.hourly ?? []
  const daily = data?.daily ?? []
  const hasDaily = daily.length > 0
  const hasHourly = hourly.length > 0
  const defaultTab = hasHourly ? 'hourly' : 'daily'
  const showTabs = hasDaily && hasHourly

  if (isLoading || !data) {
    return null
  }
  if (data.bytes_total === 0 && !(hasDaily || hasHourly)) {
    return null
  }

  return (
    <Card>
      <Tabs className="gap-0" defaultValue={defaultTab}>
        <CardHeader>
          <CardTitle>{t('traffic_title')}</CardTitle>
          {showTabs && (
            <CardAction>
              <TabsList>
                <TabsTrigger value="hourly">{t('traffic_tab_today')}</TabsTrigger>
                <TabsTrigger value="daily">{t('traffic_tab_cycle')}</TabsTrigger>
              </TabsList>
            </CardAction>
          )}
        </CardHeader>

        <CardContent>
          {showTabs ? (
            <>
              <TabsContent className="mt-0" value="hourly">
                <HourlyTrafficChart data={hourly} t={t} />
              </TabsContent>

              <TabsContent className="mt-0" value="daily">
                <DailyTrafficChart data={daily} t={t} />
              </TabsContent>
            </>
          ) : (
            <>
              {hasHourly && <HourlyTrafficChart data={hourly} t={t} />}
              {!hasHourly && hasDaily && <DailyTrafficChart data={daily} t={t} />}
            </>
          )}
        </CardContent>
      </Tabs>

      <CardFooter className="w-full flex-col items-start gap-3 text-sm sm:flex-row sm:items-center sm:justify-between">
        <div className="text-muted-foreground">
          {data.cycle_start} ~ {data.cycle_end}
        </div>
        <div className="flex flex-wrap gap-4 text-muted-foreground leading-none sm:justify-end">
          <span>↓ In {formatBytes(data.bytes_in)}</span>
          <span>↑ Out {formatBytes(data.bytes_out)}</span>
          <span>Total {formatBytes(data.bytes_total)}</span>
        </div>
      </CardFooter>
    </Card>
  )
}

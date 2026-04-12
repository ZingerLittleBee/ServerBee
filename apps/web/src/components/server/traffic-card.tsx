import { useTranslation } from 'react-i18next'
import { Bar, BarChart, CartesianGrid, XAxis, YAxis } from 'recharts'
import { Card, CardAction, CardContent, CardFooter, CardHeader, CardTitle } from '@/components/ui/card'
import {
  type ChartConfig,
  ChartContainer,
  ChartLegend,
  ChartLegendContent,
  ChartTooltip,
  ChartTooltipContent
} from '@/components/ui/chart'
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs'
import { useTraffic } from '@/hooks/use-traffic'
import { formatBytes } from '@/lib/utils'

const trafficConfig = {
  bytes_in: { label: '↓ In', color: 'var(--chart-1)' },
  bytes_out: { label: '↑ Out', color: 'var(--chart-2)' }
} satisfies ChartConfig

export function TrafficCard({ serverId }: { serverId: string }) {
  const { t } = useTranslation('servers')
  const { data, isLoading } = useTraffic(serverId)
  const hasDaily = (data?.daily.length ?? 0) > 0
  const hasHourly = (data?.hourly.length ?? 0) > 0
  const defaultTab = hasHourly ? 'hourly' : 'daily'
  const showTabs = hasDaily && hasHourly

  if (isLoading || !data) {
    return null
  }
  if (data.bytes_total === 0 && !hasDaily && !hasHourly) {
    return null
  }

  return (
    <Card className="mt-4">
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
              <TabsContent value="hourly">
                <ChartContainer className="h-[260px] w-full" config={trafficConfig}>
                  <BarChart accessibilityLayer data={data.hourly} maxBarSize={40}>
                    <CartesianGrid vertical={false} />
                    <XAxis
                      axisLine={false}
                      dataKey="hour"
                      tickFormatter={(v: string) => {
                        const d = new Date(v)
                        return `${d.getHours().toString().padStart(2, '0')}:00`
                      }}
                      tickLine={false}
                      tickMargin={10}
                    />
                    <YAxis axisLine={false} tickFormatter={formatBytes} tickLine={false} width={60} />
                    <ChartTooltip
                      content={
                        <ChartTooltipContent
                          labelFormatter={(label) => {
                            const d = new Date(String(label))
                            return `${d.getHours().toString().padStart(2, '0')}:00`
                          }}
                          valueFormatter={(v) => formatBytes(v)}
                        />
                      }
                      cursor={false}
                    />
                    <ChartLegend content={<ChartLegendContent />} />
                    <Bar dataKey="bytes_in" fill="var(--color-bytes_in)" radius={[0, 0, 4, 4]} stackId="traffic" />
                    <Bar dataKey="bytes_out" fill="var(--color-bytes_out)" radius={[4, 4, 0, 0]} stackId="traffic" />
                  </BarChart>
                </ChartContainer>
              </TabsContent>

              <TabsContent value="daily">
                <ChartContainer className="h-[260px] w-full" config={trafficConfig}>
                  <BarChart accessibilityLayer data={data.daily} maxBarSize={40}>
                    <CartesianGrid vertical={false} />
                    <XAxis
                      axisLine={false}
                      dataKey="date"
                      tickFormatter={(v: string) => v.slice(5)}
                      tickLine={false}
                      tickMargin={10}
                    />
                    <YAxis axisLine={false} tickFormatter={formatBytes} tickLine={false} width={60} />
                    <ChartTooltip
                      content={<ChartTooltipContent hideLabel valueFormatter={(v) => formatBytes(v)} />}
                      cursor={false}
                    />
                    <ChartLegend content={<ChartLegendContent />} />
                    <Bar dataKey="bytes_in" fill="var(--color-bytes_in)" radius={[0, 0, 4, 4]} stackId="traffic" />
                    <Bar dataKey="bytes_out" fill="var(--color-bytes_out)" radius={[4, 4, 0, 0]} stackId="traffic" />
                  </BarChart>
                </ChartContainer>
              </TabsContent>
            </>
          ) : (
            <>
              {hasHourly && (
                <ChartContainer className="h-[260px] w-full" config={trafficConfig}>
                  <BarChart accessibilityLayer data={data.hourly} maxBarSize={40}>
                    <CartesianGrid vertical={false} />
                    <XAxis
                      axisLine={false}
                      dataKey="hour"
                      tickFormatter={(v: string) => {
                        const d = new Date(v)
                        return `${d.getHours().toString().padStart(2, '0')}:00`
                      }}
                      tickLine={false}
                      tickMargin={10}
                    />
                    <YAxis axisLine={false} tickFormatter={formatBytes} tickLine={false} width={60} />
                    <ChartTooltip
                      content={
                        <ChartTooltipContent
                          labelFormatter={(label) => {
                            const d = new Date(String(label))
                            return `${d.getHours().toString().padStart(2, '0')}:00`
                          }}
                          valueFormatter={(v) => formatBytes(v)}
                        />
                      }
                      cursor={false}
                    />
                    <ChartLegend content={<ChartLegendContent />} />
                    <Bar dataKey="bytes_in" fill="var(--color-bytes_in)" radius={[0, 0, 4, 4]} stackId="traffic" />
                    <Bar dataKey="bytes_out" fill="var(--color-bytes_out)" radius={[4, 4, 0, 0]} stackId="traffic" />
                  </BarChart>
                </ChartContainer>
              )}

              {!hasHourly && hasDaily && (
                <ChartContainer className="h-[260px] w-full" config={trafficConfig}>
                  <BarChart accessibilityLayer data={data.daily} maxBarSize={40}>
                    <CartesianGrid vertical={false} />
                    <XAxis
                      axisLine={false}
                      dataKey="date"
                      tickFormatter={(v: string) => v.slice(5)}
                      tickLine={false}
                      tickMargin={10}
                    />
                    <YAxis axisLine={false} tickFormatter={formatBytes} tickLine={false} width={60} />
                    <ChartTooltip
                      content={<ChartTooltipContent hideLabel valueFormatter={(v) => formatBytes(v)} />}
                      cursor={false}
                    />
                    <ChartLegend content={<ChartLegendContent />} />
                    <Bar dataKey="bytes_in" fill="var(--color-bytes_in)" radius={[0, 0, 4, 4]} stackId="traffic" />
                    <Bar dataKey="bytes_out" fill="var(--color-bytes_out)" radius={[4, 4, 0, 0]} stackId="traffic" />
                  </BarChart>
                </ChartContainer>
              )}
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

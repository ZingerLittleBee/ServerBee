import { useTranslation } from 'react-i18next'
import { Bar, BarChart, CartesianGrid, Line, LineChart, XAxis, YAxis } from 'recharts'
import { Card, CardContent, CardDescription, CardFooter, CardHeader, CardTitle } from '@/components/ui/card'
import {
  type ChartConfig,
  ChartContainer,
  ChartLegend,
  ChartLegendContent,
  ChartTooltip,
  ChartTooltipContent
} from '@/components/ui/chart'
import { useTraffic } from '@/hooks/use-traffic'
import { formatBytes } from '@/lib/utils'

const trafficConfig = {
  bytes_in: { label: '↓ In', color: 'var(--chart-1)' },
  bytes_out: { label: '↑ Out', color: 'var(--chart-2)' }
} satisfies ChartConfig

export function TrafficCard({ serverId }: { serverId: string }) {
  const { t } = useTranslation('servers')
  const { data, isLoading } = useTraffic(serverId)

  if (isLoading || !data) {
    return null
  }
  if (data.bytes_total === 0 && data.daily.length === 0) {
    return null
  }

  return (
    <Card className="mt-4">
      <CardHeader>
        <CardTitle>{t('traffic_title', { defaultValue: 'Traffic Statistics' })}</CardTitle>
        <CardDescription>
          {data.cycle_start} ~ {data.cycle_end}
        </CardDescription>
      </CardHeader>
      <CardContent className="space-y-6">
        {data.daily.length > 0 && (
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
              <ChartTooltip content={<ChartTooltipContent hideLabel valueFormatter={(v) => formatBytes(v)} />} />
              <ChartLegend content={<ChartLegendContent />} />
              <Bar dataKey="bytes_in" fill="var(--color-bytes_in)" radius={[0, 0, 4, 4]} stackId="traffic" />
              <Bar dataKey="bytes_out" fill="var(--color-bytes_out)" radius={[4, 4, 0, 0]} stackId="traffic" />
            </BarChart>
          </ChartContainer>
        )}

        {data.hourly.length > 0 && (
          <div>
            <h4 className="mb-2 font-medium text-muted-foreground text-sm">
              {t('traffic_hourly', { defaultValue: "Today's Hourly Traffic" })}
            </h4>
            <ChartContainer className="h-[160px] w-full" config={trafficConfig}>
              <LineChart accessibilityLayer data={data.hourly}>
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
                <ChartTooltip content={<ChartTooltipContent valueFormatter={(v) => formatBytes(v)} />} />
                <Line dataKey="bytes_in" dot={false} stroke="var(--color-bytes_in)" strokeWidth={1.5} type="monotone" />
                <Line
                  dataKey="bytes_out"
                  dot={false}
                  stroke="var(--color-bytes_out)"
                  strokeWidth={1.5}
                  type="monotone"
                />
              </LineChart>
            </ChartContainer>
          </div>
        )}
      </CardContent>
      <CardFooter className="flex-col items-start gap-2 text-sm">
        <div className="flex gap-4 text-muted-foreground leading-none">
          <span>↓ In {formatBytes(data.bytes_in)}</span>
          <span>↑ Out {formatBytes(data.bytes_out)}</span>
          <span>Total {formatBytes(data.bytes_total)}</span>
        </div>
      </CardFooter>
    </Card>
  )
}

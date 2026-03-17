import { ChevronDown, ChevronUp } from 'lucide-react'
import { useState } from 'react'
import { useTranslation } from 'react-i18next'
import { Bar, BarChart, CartesianGrid, Line, LineChart, XAxis, YAxis } from 'recharts'
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
  const [expanded, setExpanded] = useState(false)

  if (isLoading || !data) {
    return null
  }
  if (data.bytes_total === 0 && data.daily.length === 0) {
    return null
  }

  return (
    <div className="rounded-lg border bg-card">
      <button
        className="flex w-full items-center justify-between p-3 text-sm"
        onClick={() => setExpanded(!expanded)}
        type="button"
      >
        <span className="font-medium">
          {t('traffic_title', { defaultValue: 'Traffic Statistics' })}
          <span className="ml-2 text-muted-foreground">
            {data.cycle_start} ~ {data.cycle_end}
          </span>
        </span>
        <div className="flex items-center gap-3">
          <span className="text-muted-foreground">
            ↑ {formatBytes(data.bytes_out)} ↓ {formatBytes(data.bytes_in)}
          </span>
          {expanded ? <ChevronUp className="size-4" /> : <ChevronDown className="size-4" />}
        </div>
      </button>

      {expanded && (
        <div className="space-y-4 border-t p-4">
          {data.daily.length > 0 && (
            <div>
              <h4 className="mb-2 font-medium text-muted-foreground text-xs">
                {t('traffic_daily', { defaultValue: 'Daily Traffic' })}
              </h4>
              <ChartContainer className="h-[200px] w-full" config={trafficConfig}>
                <BarChart accessibilityLayer data={data.daily}>
                  <CartesianGrid vertical={false} />
                  <XAxis
                    axisLine={false}
                    dataKey="date"
                    fontSize={10}
                    tickFormatter={(v: string) => v.slice(5)}
                    tickLine={false}
                  />
                  <YAxis axisLine={false} fontSize={10} tickFormatter={formatBytes} tickLine={false} width={60} />
                  <ChartTooltip
                    content={
                      <ChartTooltipContent
                        formatter={(value) => formatBytes(Number(value))}
                        labelFormatter={(label) => String(label)}
                      />
                    }
                  />
                  <ChartLegend content={<ChartLegendContent />} />
                  <Bar dataKey="bytes_in" fill="var(--color-bytes_in)" radius={4} stackId="traffic" />
                  <Bar dataKey="bytes_out" fill="var(--color-bytes_out)" radius={4} stackId="traffic" />
                </BarChart>
              </ChartContainer>
            </div>
          )}

          {data.hourly.length > 0 && (
            <div>
              <h4 className="mb-2 font-medium text-muted-foreground text-xs">
                {t('traffic_hourly', { defaultValue: "Today's Hourly Traffic" })}
              </h4>
              <ChartContainer className="h-[160px] w-full" config={trafficConfig}>
                <LineChart accessibilityLayer data={data.hourly}>
                  <CartesianGrid vertical={false} />
                  <XAxis
                    axisLine={false}
                    dataKey="hour"
                    fontSize={10}
                    tickFormatter={(v: string) => {
                      const d = new Date(v)
                      return `${d.getHours().toString().padStart(2, '0')}:00`
                    }}
                    tickLine={false}
                  />
                  <YAxis axisLine={false} fontSize={10} tickFormatter={formatBytes} tickLine={false} width={60} />
                  <ChartTooltip content={<ChartTooltipContent formatter={(value) => formatBytes(Number(value))} />} />
                  <Line
                    dataKey="bytes_in"
                    dot={false}
                    stroke="var(--color-bytes_in)"
                    strokeWidth={1.5}
                    type="monotone"
                  />
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
        </div>
      )}
    </div>
  )
}

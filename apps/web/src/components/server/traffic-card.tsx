import { ChevronDown, ChevronUp } from 'lucide-react'
import { useState } from 'react'
import { useTranslation } from 'react-i18next'
import { Bar, BarChart, CartesianGrid, Line, LineChart, ResponsiveContainer, Tooltip, XAxis, YAxis } from 'recharts'
import { useTraffic } from '@/hooks/use-traffic'
import { formatBytes } from '@/lib/utils'

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
              <ResponsiveContainer height={200} width="100%">
                <BarChart data={data.daily}>
                  <CartesianGrid stroke="hsl(var(--border))" strokeDasharray="3 3" />
                  <XAxis
                    dataKey="date"
                    fontSize={10}
                    stroke="hsl(var(--muted-foreground))"
                    tickFormatter={(v: string) => v.slice(5)}
                  />
                  <YAxis fontSize={10} stroke="hsl(var(--muted-foreground))" tickFormatter={formatBytes} width={60} />
                  <Tooltip
                    contentStyle={{
                      background: 'hsl(var(--card))',
                      border: '1px solid hsl(var(--border))',
                      borderRadius: '6px',
                      fontSize: '12px'
                    }}
                    formatter={(value) => formatBytes(Number(value))}
                    labelFormatter={(label) => String(label)}
                  />
                  <Bar dataKey="bytes_in" fill="hsl(var(--chart-1))" name="↓ In" stackId="traffic" />
                  <Bar dataKey="bytes_out" fill="hsl(var(--chart-2))" name="↑ Out" stackId="traffic" />
                </BarChart>
              </ResponsiveContainer>
            </div>
          )}

          {data.hourly.length > 0 && (
            <div>
              <h4 className="mb-2 font-medium text-muted-foreground text-xs">
                {t('traffic_hourly', { defaultValue: "Today's Hourly Traffic" })}
              </h4>
              <ResponsiveContainer height={160} width="100%">
                <LineChart data={data.hourly}>
                  <CartesianGrid stroke="hsl(var(--border))" strokeDasharray="3 3" />
                  <XAxis
                    dataKey="hour"
                    fontSize={10}
                    stroke="hsl(var(--muted-foreground))"
                    tickFormatter={(v: string) => {
                      const d = new Date(v)
                      return `${d.getHours().toString().padStart(2, '0')}:00`
                    }}
                  />
                  <YAxis fontSize={10} stroke="hsl(var(--muted-foreground))" tickFormatter={formatBytes} width={60} />
                  <Tooltip
                    contentStyle={{
                      background: 'hsl(var(--card))',
                      border: '1px solid hsl(var(--border))',
                      borderRadius: '6px',
                      fontSize: '12px'
                    }}
                    formatter={(value) => formatBytes(Number(value))}
                  />
                  <Line dataKey="bytes_in" dot={false} name="↓ In" stroke="hsl(var(--chart-1))" strokeWidth={1.5} />
                  <Line dataKey="bytes_out" dot={false} name="↑ Out" stroke="hsl(var(--chart-2))" strokeWidth={1.5} />
                </LineChart>
              </ResponsiveContainer>
            </div>
          )}
        </div>
      )}
    </div>
  )
}

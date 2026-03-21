import { useMemo } from 'react'
import { PolarAngleAxis, RadialBar, RadialBarChart, ResponsiveContainer } from 'recharts'
import type { ServerMetrics } from '@/hooks/use-servers-ws'
import { extractLiveMetric, METRIC_LABELS } from '@/lib/widget-helpers'
import type { GaugeConfig } from '@/lib/widget-types'

interface GaugeWidgetProps {
  config: GaugeConfig
  servers: ServerMetrics[]
}

function getGaugeColor(value: number): string {
  if (value >= 90) {
    return 'var(--color-chart-4)'
  }
  if (value >= 70) {
    return 'var(--color-chart-5)'
  }
  return 'var(--color-chart-1)'
}

export function GaugeWidget({ config, servers }: GaugeWidgetProps) {
  const { server_id, metric } = config
  const max = config.max ?? 100

  const server = useMemo(() => servers.find((s) => s.id === server_id), [servers, server_id])

  const value = useMemo(() => {
    if (!server) {
      return 0
    }
    return Math.min(max, Math.max(0, extractLiveMetric(server, metric)))
  }, [server, metric, max])

  const label = config.label ?? METRIC_LABELS[metric] ?? metric
  const color = getGaugeColor(value)
  const data = [{ name: label, value, fill: color }]

  if (!server) {
    return (
      <div className="flex h-full items-center justify-center rounded-lg border bg-card text-muted-foreground text-sm">
        Server not found
      </div>
    )
  }

  return (
    <div className="flex h-full flex-col items-center justify-center rounded-lg border bg-card p-2">
      <ResponsiveContainer height="100%" width="100%">
        <RadialBarChart
          barSize={10}
          cx="50%"
          cy="50%"
          data={data}
          endAngle={-270}
          innerRadius="70%"
          outerRadius="90%"
          startAngle={90}
        >
          <PolarAngleAxis angleAxisId={0} domain={[0, max]} tick={false} type="number" />
          <RadialBar angleAxisId={0} background={{ fill: 'var(--color-muted)' }} cornerRadius={5} dataKey="value" />
          <text dominantBaseline="middle" textAnchor="middle" x="50%" y="45%">
            <tspan className="fill-foreground font-bold text-2xl">{value.toFixed(1)}%</tspan>
          </text>
          <text dominantBaseline="middle" textAnchor="middle" x="50%" y="58%">
            <tspan className="fill-muted-foreground text-xs">{label}</tspan>
          </text>
        </RadialBarChart>
      </ResponsiveContainer>
      <p className="truncate text-muted-foreground text-xs">{server.name}</p>
    </div>
  )
}

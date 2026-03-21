import { useMemo } from 'react'
import { PolarAngleAxis, RadialBar, RadialBarChart, ResponsiveContainer } from 'recharts'
import type { ServerMetrics } from '@/hooks/use-servers-ws'
import type { GaugeConfig } from '@/lib/widget-types'

interface GaugeWidgetProps {
  config: GaugeConfig
  servers: ServerMetrics[]
}

function extractMetric(server: ServerMetrics, metric: string): number {
  switch (metric) {
    case 'cpu':
      return server.cpu
    case 'memory':
      return server.mem_total > 0 ? (server.mem_used / server.mem_total) * 100 : 0
    case 'disk':
      return server.disk_total > 0 ? (server.disk_used / server.disk_total) * 100 : 0
    case 'swap':
      return server.swap_total > 0 ? (server.swap_used / server.swap_total) * 100 : 0
    default:
      return 0
  }
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

const METRIC_LABELS: Record<string, string> = {
  cpu: 'CPU',
  memory: 'Memory',
  disk: 'Disk',
  swap: 'Swap'
}

export function GaugeWidget({ config, servers }: GaugeWidgetProps) {
  const { server_id, metric } = config
  const max = config.max ?? 100

  const server = useMemo(() => servers.find((s) => s.id === server_id), [servers, server_id])

  const value = useMemo(() => {
    if (!server) {
      return 0
    }
    return Math.min(max, Math.max(0, extractMetric(server, metric)))
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

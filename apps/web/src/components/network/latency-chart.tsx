import { useMemo } from 'react'
import { Area, AreaChart, CartesianGrid, ResponsiveContainer, Tooltip, XAxis, YAxis } from 'recharts'
import type { NetworkProbeRecord } from '@/lib/network-types'

interface TargetInfo {
  color: string
  id: string
  name: string
  visible: boolean
}

interface LatencyChartProps {
  isRealtime?: boolean
  records: NetworkProbeRecord[]
  targets: TargetInfo[]
}

function formatTime(timestamp: string, isRealtime: boolean): string {
  const date = new Date(timestamp)
  if (isRealtime) {
    return date.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit', second: '2-digit' })
  }
  return date.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })
}

export function LatencyChart({ records, targets, isRealtime = false }: LatencyChartProps) {
  const visibleTargets = useMemo(() => targets.filter((t) => t.visible), [targets])

  const chartData = useMemo(() => {
    const timeMap = new Map<string, Record<string, unknown>>()

    for (const record of records) {
      const key = record.timestamp
      if (!timeMap.has(key)) {
        timeMap.set(key, { timestamp: key })
      }
      const entry = timeMap.get(key)
      if (entry) {
        entry[record.target_id] = record.avg_latency
      }
    }

    const entries = Array.from(timeMap.values())
    entries.sort((a, b) => {
      const ta = a.timestamp as string
      const tb = b.timestamp as string
      if (ta < tb) {
        return -1
      }
      if (ta > tb) {
        return 1
      }
      return 0
    })
    return entries
  }, [records])

  const targetNameMap = useMemo(() => {
    const map: Record<string, string> = {}
    for (const t of targets) {
      map[t.id] = t.name
    }
    return map
  }, [targets])

  if (chartData.length === 0) {
    return (
      <div className="flex h-[300px] items-center justify-center rounded-lg border bg-card">
        <p className="text-muted-foreground text-sm">No data available</p>
      </div>
    )
  }

  return (
    <div className="rounded-lg border bg-card p-4">
      <h3 className="mb-3 font-semibold text-sm">Latency (ms)</h3>
      <ResponsiveContainer height={300} width="100%">
        <AreaChart data={chartData}>
          <defs>
            {visibleTargets.map((t) => (
              <linearGradient id={`gradient-latency-${t.id}`} key={t.id} x1="0" x2="0" y1="0" y2="1">
                <stop offset="5%" stopColor={t.color} stopOpacity={0.2} />
                <stop offset="95%" stopColor={t.color} stopOpacity={0} />
              </linearGradient>
            ))}
          </defs>
          <CartesianGrid stroke="var(--color-border)" strokeDasharray="3 3" vertical={false} />
          <XAxis
            axisLine={false}
            dataKey="timestamp"
            stroke="var(--color-muted-foreground)"
            tick={{ fontSize: 11 }}
            tickFormatter={(v) => formatTime(v, isRealtime)}
            tickLine={false}
          />
          <YAxis
            axisLine={false}
            stroke="var(--color-muted-foreground)"
            tick={{ fontSize: 11 }}
            tickLine={false}
            unit=" ms"
            width={60}
          />
          <Tooltip
            contentStyle={{
              backgroundColor: 'var(--color-card)',
              border: '1px solid var(--color-border)',
              borderRadius: '8px',
              fontSize: '12px'
            }}
            formatter={(value: number | null, name: string) => {
              const label = targetNameMap[name] ?? name
              return [value != null ? `${value.toFixed(1)} ms` : 'N/A', label]
            }}
            labelFormatter={(label) => {
              const date = new Date(label)
              return date.toLocaleString([], {
                month: 'short',
                day: 'numeric',
                hour: '2-digit',
                minute: '2-digit',
                second: '2-digit'
              })
            }}
          />
          {visibleTargets.map((t) => (
            <Area
              connectNulls={false}
              dataKey={t.id}
              fill={`url(#gradient-latency-${t.id})`}
              key={t.id}
              name={t.id}
              stroke={t.color}
              strokeWidth={2}
              type="monotone"
            />
          ))}
        </AreaChart>
      </ResponsiveContainer>
    </div>
  )
}

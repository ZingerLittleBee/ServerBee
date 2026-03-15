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

function formatTimeFull(timestamp: string): string {
  return new Date(timestamp).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit', second: '2-digit' })
}

function formatTimeShort(timestamp: string): string {
  return new Date(timestamp).toLocaleTimeString([], { minute: '2-digit', second: '2-digit' })
}

function formatTimeHM(timestamp: string): string {
  return new Date(timestamp).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })
}

export function LatencyChart({ records, targets, isRealtime = false }: LatencyChartProps) {
  const visibleTargets = useMemo(() => targets.filter((t) => t.visible), [targets])

  const chartData = useMemo(() => {
    // Bucket records into time slots to align different targets' data points.
    // Without bucketing, each target gets its own timestamp and Recharts
    // only draws a line when the data entry has a value for that target.
    const bucketMs = isRealtime ? 10_000 : 60_000 // 10s realtime, 60s historical
    const bucketMap = new Map<number, Record<string, unknown>>()

    for (const record of records) {
      const ts = new Date(record.timestamp).getTime()
      const bucketKey = Math.floor(ts / bucketMs) * bucketMs

      if (!bucketMap.has(bucketKey)) {
        bucketMap.set(bucketKey, { timestamp: new Date(bucketKey).toISOString() })
      }
      const entry = bucketMap.get(bucketKey)
      if (entry) {
        // If multiple records for same target in same bucket, keep the latest
        entry[record.target_id] = record.avg_latency
      }
    }

    const entries = Array.from(bucketMap.entries())
      .sort((a, b) => a[0] - b[0])
      .map(([, v]) => v)
    return entries
  }, [records, isRealtime])

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
            tickFormatter={(v, index) => {
              if (!isRealtime) {
                return formatTimeHM(v)
              }
              // Realtime: first and last tick show HH:mm:ss, middle ticks show mm:ss
              const isEdge = index === 0
              return isEdge ? formatTimeFull(v) : formatTimeShort(v)
            }}
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
            formatter={(value, name) => {
              const strName = typeof name === 'string' ? name : String(name ?? '')
              const label = targetNameMap[strName] ?? strName
              const numValue = typeof value === 'number' ? value : null
              return [numValue != null ? `${numValue.toFixed(1)} ms` : 'N/A', label]
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

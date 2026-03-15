import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from '@/components/ui/table'
import type { NetworkProbeAnomaly } from '@/lib/network-types'
import { cn } from '@/lib/utils'

interface AnomalyTableProps {
  anomalies: NetworkProbeAnomaly[]
}

function isCritical(type: string): boolean {
  return type === 'unreachable' || type === 'very_high_latency' || type === 'very_high_packet_loss'
}

function formatAnomalyValue(type: string, value: number): string {
  const lower = type.toLowerCase()
  if (lower.includes('latency')) {
    return `${value.toFixed(1)} ms`
  }
  if (lower.includes('loss')) {
    return `${(value * 100).toFixed(1)}%`
  }
  return value.toFixed(2)
}

function formatTimestamp(ts: string): string {
  const date = new Date(ts)
  return date.toLocaleString([], {
    month: 'short',
    day: 'numeric',
    hour: '2-digit',
    minute: '2-digit',
    second: '2-digit'
  })
}

export function AnomalyTable({ anomalies }: AnomalyTableProps) {
  if (anomalies.length === 0) {
    return null
  }

  return (
    <div className="rounded-lg border bg-card">
      <div className="border-b px-4 py-3">
        <h3 className="font-semibold text-sm">Anomalies ({anomalies.length})</h3>
      </div>
      <Table>
        <TableHeader>
          <TableRow>
            <TableHead>Time</TableHead>
            <TableHead>Target</TableHead>
            <TableHead>Type</TableHead>
            <TableHead className="text-right">Value</TableHead>
          </TableRow>
        </TableHeader>
        <TableBody>
          {anomalies.map((a) => {
            const critical = isCritical(a.anomaly_type)
            return (
              <TableRow key={`${a.timestamp}-${a.target_id}-${a.anomaly_type}`}>
                <TableCell className="font-mono text-xs">{formatTimestamp(a.timestamp)}</TableCell>
                <TableCell className="text-sm">{a.target_name}</TableCell>
                <TableCell>
                  <span
                    className={cn(
                      'inline-flex items-center rounded-full px-2 py-0.5 font-medium text-xs',
                      critical
                        ? 'bg-red-100 text-red-700 dark:bg-red-900/30 dark:text-red-400'
                        : 'bg-amber-100 text-amber-700 dark:bg-amber-900/30 dark:text-amber-400'
                    )}
                  >
                    {a.anomaly_type}
                  </span>
                </TableCell>
                <TableCell className="text-right font-mono text-sm">
                  {formatAnomalyValue(a.anomaly_type, a.value)}
                </TableCell>
              </TableRow>
            )
          })}
        </TableBody>
      </Table>
    </div>
  )
}

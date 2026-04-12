import type { ColumnDef } from '@tanstack/react-table'
import { getCoreRowModel, useReactTable } from '@tanstack/react-table'
import { useTranslation } from 'react-i18next'
import { DataTable } from '@/components/ui/data-table'
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

function getColumns(t: (key: string) => string): ColumnDef<NetworkProbeAnomaly>[] {
  return [
    {
      accessorKey: 'timestamp',
      header: t('anomaly_time'),
      enableSorting: false,
      cell: ({ row }) => (
        <span className="font-mono text-xs tabular-nums">{formatTimestamp(row.original.timestamp)}</span>
      )
    },
    {
      accessorKey: 'target_name',
      header: t('anomaly_target'),
      enableSorting: false,
      cell: ({ row }) => <span className="text-sm">{row.original.target_name}</span>
    },
    {
      accessorKey: 'anomaly_type',
      header: t('anomaly_type'),
      enableSorting: false,
      cell: ({ row }) => {
        const critical = isCritical(row.original.anomaly_type)
        return (
          <span
            className={cn(
              'inline-flex items-center rounded-full px-2 py-0.5 font-medium text-xs',
              critical
                ? 'bg-red-100 text-red-700 dark:bg-red-900/30 dark:text-red-400'
                : 'bg-amber-100 text-amber-700 dark:bg-amber-900/30 dark:text-amber-400'
            )}
          >
            {row.original.anomaly_type}
          </span>
        )
      }
    },
    {
      id: 'value',
      header: t('anomaly_value'),
      enableSorting: false,
      cell: ({ row }) => (
        <span className="font-mono text-sm">{formatAnomalyValue(row.original.anomaly_type, row.original.value)}</span>
      ),
      meta: { className: 'text-right' }
    }
  ]
}

export function AnomalyTable({ anomalies }: AnomalyTableProps) {
  const { t } = useTranslation('network')
  const table = useReactTable({
    data: anomalies,
    columns: getColumns(t),
    getCoreRowModel: getCoreRowModel()
  })

  if (anomalies.length === 0) {
    return <p className="py-4 text-muted-foreground text-sm">{t('no_anomalies')}</p>
  }

  return (
    <div className="rounded-lg border bg-card">
      <div className="border-b px-4 py-3">
        <h3 className="font-semibold text-sm">{t('anomaly_count_with_value', { count: anomalies.length })}</h3>
      </div>
      <DataTable className="rounded-none border-0" table={table} />
    </div>
  )
}

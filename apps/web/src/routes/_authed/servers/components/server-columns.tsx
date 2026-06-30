import type { ColumnDef } from '@tanstack/react-table'
import { CircleDot, ExternalLink, Tag } from 'lucide-react'
import { DataTableColumnHeader } from '@/components/data-table/data-table-column-header'
import { CostCell } from '@/components/server/cost-cell'
import { StatusDot } from '@/components/server/status-dot'
import { deriveServerStatus } from '@/components/server/status-dot-utils'
import { Checkbox } from '@/components/ui/checkbox'
import type { ServerMetrics } from '@/hooks/use-servers-ws'
import type { TrafficOverviewItem } from '@/hooks/use-traffic-overview'
import type { ServerCostOverview } from '@/lib/api-schema'
import { cn } from '@/lib/utils'
import { CpuCell, DiskCell, MemoryCell, NameCell, NetworkCell, UptimeCell } from './index-cells'
import { UpgradeBadgeCell } from './upgrade-badge-cell'

const arrayIncludesFilter = (row: { getValue: (id: string) => unknown }, id: string, value: unknown) => {
  if (!Array.isArray(value) || value.length === 0) {
    return true
  }
  return value.includes(String(row.getValue(id) ?? ''))
}

interface ServerColumnsOptions {
  costByServerId: Map<string, ServerCostOverview>
  groupMap: Map<string, string>
  groupOptions: { label: string; value: string }[]
  onEdit: (id: string) => void
  selectMode: boolean
  statusOptions: { label: string; value: string }[]
  t: (key: string) => string
  trafficOverview: TrafficOverviewItem[]
}

// Shared column definitions for the servers table. Used by both the dedicated
// servers page (full interactivity) and the dashboard server-cards widget in
// list layout, so the two stay pixel-identical.
export function buildServerColumns({
  t,
  costByServerId,
  groupMap,
  groupOptions,
  statusOptions,
  selectMode,
  onEdit,
  trafficOverview
}: ServerColumnsOptions): ColumnDef<ServerMetrics>[] {
  return [
    {
      id: 'select',
      enableSorting: false,
      header: ({ table }) => (
        <Checkbox
          aria-label="Select all"
          checked={table.getIsAllPageRowsSelected()}
          onCheckedChange={(checked) => table.toggleAllPageRowsSelected(!!checked)}
        />
      ),
      cell: ({ row }) => (
        <Checkbox
          aria-label="Select row"
          checked={row.getIsSelected()}
          onCheckedChange={(checked) => row.toggleSelected(!!checked)}
        />
      ),
      minSize: 0,
      size: selectMode ? 36 : 0,
      meta: {
        className: cn('overflow-hidden transition-[width,padding] duration-200', !selectMode && 'px-0!')
      }
    },
    {
      id: 'status-dot',
      accessorFn: (row) => (row.online ? 'online' : 'offline'),
      enableSorting: false,
      header: () => null,
      cell: ({ row }) => <StatusDot status={deriveServerStatus(row.original)} />,
      filterFn: arrayIncludesFilter,
      enableColumnFilter: true,
      size: 36,
      meta: {
        className: 'w-9',
        label: t('col_status'),
        variant: 'select',
        options: statusOptions,
        icon: CircleDot
      }
    },
    {
      accessorKey: 'name',
      id: 'name',
      header: ({ column }) => <DataTableColumnHeader column={column} label={t('col_name')} />,
      cell: ({ row }) => <NameCell rightSlot={<UpgradeBadgeCell serverId={row.original.id} />} server={row.original} />,
      size: 240,
      meta: { className: 'min-w-[200px]', label: t('col_name') }
    },
    {
      accessorKey: 'cpu',
      id: 'cpu',
      header: ({ column }) => <DataTableColumnHeader column={column} label={t('col_cpu')} />,
      cell: ({ row }) => <CpuCell server={row.original} />,
      size: 180,
      meta: { className: 'w-[180px]', cellClassName: 'align-top', label: t('col_cpu') }
    },
    {
      accessorFn: (row) => (row.mem_total > 0 ? row.mem_used / row.mem_total : 0),
      id: 'memory',
      header: ({ column }) => <DataTableColumnHeader column={column} label={t('col_memory')} />,
      cell: ({ row }) => <MemoryCell server={row.original} />,
      size: 180,
      meta: { className: 'w-[180px]', cellClassName: 'align-top', label: t('col_memory') }
    },
    {
      accessorFn: (row) => (row.disk_total > 0 ? row.disk_used / row.disk_total : 0),
      id: 'disk',
      header: ({ column }) => <DataTableColumnHeader column={column} label={t('col_disk')} />,
      cell: ({ row }) => <DiskCell server={row.original} />,
      size: 184,
      meta: { className: 'w-[184px]', cellClassName: 'align-top', label: t('col_disk') }
    },
    {
      id: 'network',
      enableSorting: false,
      header: ({ column }) => <DataTableColumnHeader column={column} label={t('col_network')} />,
      cell: ({ row }) => {
        const entry = trafficOverview.find((e) => e.server_id === row.original.id)
        return <NetworkCell entry={entry} server={row.original} />
      },
      size: 184,
      meta: { className: 'hidden lg:table-cell lg:w-[184px]', cellClassName: 'lg:align-top', label: t('col_network') }
    },
    {
      id: 'cost',
      accessorFn: (row) => {
        const entry = costByServerId.get(row.id)
        return entry?.cost_per_month_equivalent ?? entry?.cost_per_day ?? -1
      },
      header: ({ column }) => <DataTableColumnHeader column={column} label={t('col_cost')} />,
      cell: ({ row }) => <CostCell entry={costByServerId.get(row.original.id)} />,
      size: 172,
      meta: { className: 'hidden xl:table-cell xl:w-[172px]', cellClassName: 'xl:align-top', label: t('col_cost') }
    },
    {
      accessorKey: 'uptime',
      id: 'uptime',
      header: ({ column }) => <DataTableColumnHeader column={column} label={t('col_uptime')} />,
      cell: ({ row }) => <UptimeCell server={row.original} />,
      size: 196,
      meta: { className: 'hidden xl:table-cell xl:w-[196px]', label: t('col_uptime') }
    },
    {
      id: 'group',
      accessorFn: (row) => row.group_id ?? '',
      header: ({ column }) => <DataTableColumnHeader column={column} label={t('col_group')} />,
      cell: ({ row }) => {
        const s = row.original
        return (
          <span className="text-muted-foreground text-xs">{s.group_id ? (groupMap.get(s.group_id) ?? '-') : '-'}</span>
        )
      },
      filterFn: arrayIncludesFilter,
      enableColumnFilter: true,
      size: 140,
      meta: {
        className: 'hidden xl:table-cell xl:w-[140px]',
        label: t('col_group'),
        variant: 'multiSelect',
        options: groupOptions,
        icon: Tag
      }
    },
    {
      id: 'actions',
      enableSorting: false,
      cell: ({ row }) => (
        <button
          aria-label={t('detail_edit')}
          className="rounded p-1 text-muted-foreground hover:bg-muted hover:text-foreground"
          onClick={() => onEdit(row.original.id)}
          type="button"
        >
          <ExternalLink aria-hidden="true" className="size-3.5" />
        </button>
      ),
      size: 40,
      meta: { className: 'w-10' }
    }
  ]
}

import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { createFileRoute, Link } from '@tanstack/react-router'
import {
  type ColumnDef,
  getCoreRowModel,
  getSortedRowModel,
  type RowSelectionState,
  type SortingState,
  useReactTable
} from '@tanstack/react-table'
import { ExternalLink, Search, Trash2 } from 'lucide-react'
import { useMemo, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { toast } from 'sonner'
import { ServerEditDialog } from '@/components/server/server-edit-dialog'
import { StatusBadge } from '@/components/server/status-badge'
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
  AlertDialogTrigger
} from '@/components/ui/alert-dialog'
import { Button } from '@/components/ui/button'
import { createSelectColumn, DataTable, DataTableColumnHeader } from '@/components/ui/data-table'
import { Input } from '@/components/ui/input'
import { Skeleton } from '@/components/ui/skeleton'
import { useServer } from '@/hooks/use-api'
import type { ServerMetrics } from '@/hooks/use-servers-ws'
import { api } from '@/lib/api-client'
import type { ServerGroup } from '@/lib/api-schema'
import { cn, countryCodeToFlag, formatBytes, formatSpeed, formatUptime } from '@/lib/utils'

export const Route = createFileRoute('/_authed/servers/')({
  component: ServersListPage,
  validateSearch: (search: Record<string, unknown>) => ({
    q: (search.q as string) || '',
    sort: (search.sort as string) || 'name'
  })
})

function ServersListPage() {
  const { t } = useTranslation(['servers', 'common'])
  const queryClient = useQueryClient()
  const navigate = Route.useNavigate()
  const { q: search, sort } = Route.useSearch()
  const { data: servers = [] } = useQuery<ServerMetrics[]>({
    queryKey: ['servers'],
    queryFn: () => [],
    staleTime: Number.POSITIVE_INFINITY,
    refetchOnMount: false,
    refetchOnWindowFocus: false
  })

  const { data: groups } = useQuery<ServerGroup[]>({
    queryKey: ['server-groups'],
    queryFn: () => api.get<ServerGroup[]>('/api/server-groups'),
    staleTime: 60_000
  })

  const setSearch = (value: string) => navigate({ search: (prev) => ({ ...prev, q: value }) })
  const [sorting, setSorting] = useState<SortingState>([{ id: sort, desc: false }])
  const [rowSelection, setRowSelection] = useState<RowSelectionState>({})
  const [editingId, setEditingId] = useState<string | null>(null)

  const groupMap = useMemo(() => new Map(groups?.map((g) => [g.id, g.name]) ?? []), [groups])

  const filtered = useMemo(() => {
    const q = search.toLowerCase()
    if (!q) {
      return servers
    }
    return servers.filter(
      (s) =>
        s.name.toLowerCase().includes(q) ||
        s.os?.toLowerCase().includes(q) ||
        s.country_code?.toLowerCase().includes(q) ||
        s.region?.toLowerCase().includes(q) ||
        (s.group_id && groupMap.get(s.group_id)?.toLowerCase().includes(q))
    )
  }, [servers, search, groupMap])

  const columns = useMemo<ColumnDef<ServerMetrics>[]>(
    () => [
      createSelectColumn<ServerMetrics>(),
      {
        accessorKey: 'name',
        header: ({ column }) => <DataTableColumnHeader column={column} title={t('col_name')} />,
        cell: ({ row }) => {
          const s = row.original
          const flag = countryCodeToFlag(s.country_code)
          return (
            <Link className="group/link flex items-center gap-1.5" params={{ id: s.id }} to="/servers/$id">
              {flag && <span className="text-xs">{flag}</span>}
              <span className="font-medium group-hover/link:underline">{s.name}</span>
            </Link>
          )
        }
      },
      {
        accessorKey: 'online',
        header: ({ column }) => <DataTableColumnHeader column={column} title={t('col_status')} />,
        cell: ({ row }) => <StatusBadge online={row.original.online} />
      },
      {
        accessorKey: 'cpu',
        header: ({ column }) => <DataTableColumnHeader column={column} title={t('col_cpu')} />,
        cell: ({ row }) => <MiniBar pct={row.original.cpu} />
      },
      {
        accessorFn: (row) => (row.mem_total > 0 ? row.mem_used / row.mem_total : 0),
        id: 'memory',
        header: ({ column }) => <DataTableColumnHeader column={column} title={t('col_memory')} />,
        cell: ({ row }) => {
          const s = row.original
          const memPct = s.mem_total > 0 ? (s.mem_used / s.mem_total) * 100 : 0
          return <MiniBar pct={memPct} sub={formatBytes(s.mem_used)} />
        }
      },
      {
        accessorFn: (row) => (row.disk_total > 0 ? row.disk_used / row.disk_total : 0),
        id: 'disk',
        header: ({ column }) => <DataTableColumnHeader column={column} title={t('col_disk')} />,
        cell: ({ row }) => {
          const s = row.original
          const diskPct = s.disk_total > 0 ? (s.disk_used / s.disk_total) * 100 : 0
          return <MiniBar pct={diskPct} sub={formatBytes(s.disk_used)} />
        }
      },
      {
        id: 'network',
        enableSorting: false,
        header: () => <span className="text-muted-foreground text-xs">{t('col_network')}</span>,
        cell: ({ row }) => {
          const s = row.original
          return (
            <span className="text-muted-foreground text-xs">
              <span>↓{formatSpeed(s.net_in_speed)}</span>
              <span className="ml-2">↑{formatSpeed(s.net_out_speed)}</span>
            </span>
          )
        },
        meta: { className: 'hidden lg:table-cell' }
      },
      {
        accessorKey: 'uptime',
        header: ({ column }) => <DataTableColumnHeader column={column} title={t('col_uptime')} />,
        cell: ({ row }) => {
          const s = row.original
          return <span className="text-muted-foreground text-xs">{s.online ? formatUptime(s.uptime) : '-'}</span>
        },
        meta: { className: 'hidden xl:table-cell' }
      },
      {
        accessorFn: (row) => (row.group_id ? (groupMap.get(row.group_id) ?? '') : ''),
        id: 'group',
        header: ({ column }) => <DataTableColumnHeader column={column} title={t('col_group')} />,
        cell: ({ row }) => {
          const s = row.original
          return (
            <span className="text-muted-foreground text-xs">
              {s.group_id ? (groupMap.get(s.group_id) ?? '-') : '-'}
            </span>
          )
        },
        meta: { className: 'hidden xl:table-cell' }
      },
      {
        id: 'actions',
        enableSorting: false,
        cell: ({ row }) => (
          <button
            aria-label={t('servers:detail_edit')}
            className="rounded p-1 text-muted-foreground hover:bg-muted hover:text-foreground"
            onClick={() => setEditingId(row.original.id)}
            type="button"
          >
            <ExternalLink aria-hidden="true" className="size-3.5" />
          </button>
        ),
        meta: { className: 'w-10' }
      }
    ],
    [t, groupMap]
  )

  const table = useReactTable({
    data: filtered,
    columns,
    state: { sorting, rowSelection },
    onSortingChange: setSorting,
    onRowSelectionChange: setRowSelection,
    getCoreRowModel: getCoreRowModel(),
    getSortedRowModel: getSortedRowModel(),
    getRowId: (row) => row.id
  })

  const selectedIds = table.getSelectedRowModel().rows.map((r) => r.original.id)
  const selectedCount = selectedIds.length

  const batchDeleteMutation = useMutation({
    mutationFn: (ids: string[]) => api.post<{ deleted: number }>('/api/servers/batch-delete', { ids }),
    onSuccess: () => {
      setRowSelection({})
      queryClient.invalidateQueries({ queryKey: ['servers'] })
    }
  })

  const handleBatchDelete = () => {
    if (selectedCount === 0) {
      return
    }
    const count = selectedCount
    batchDeleteMutation.mutate(selectedIds, {
      onSuccess: () => {
        toast.success(`Deleted ${count} server(s)`)
      },
      onError: (err) => {
        toast.error(err instanceof Error ? err.message : 'Operation failed')
      }
    })
  }

  return (
    <div>
      <div className="mb-6 flex items-center justify-between">
        <div>
          <h1 className="font-bold text-2xl">{t('title')}</h1>
          <p className="text-muted-foreground text-sm">
            {t('servers_online', { online: servers.filter((s) => s.online).length, total: servers.length })}
          </p>
        </div>
      </div>

      <div className="mb-4 flex items-center gap-3">
        <div className="relative flex-1">
          <Search className="absolute top-1/2 left-3 size-4 -translate-y-1/2 text-muted-foreground" />
          <Input
            aria-label={t('servers:search_placeholder')}
            autoComplete="off"
            className="pl-9"
            name="search"
            onChange={(e) => setSearch(e.target.value)}
            placeholder={t('servers:search_placeholder')}
            type="text"
            value={search}
          />
        </div>
        {selectedCount > 0 && (
          <AlertDialog>
            <AlertDialogTrigger
              render={
                <Button disabled={batchDeleteMutation.isPending} size="sm" variant="destructive">
                  <Trash2 aria-hidden="true" className="size-3.5" />
                  {t('servers:delete_selected', { count: selectedCount })}
                </Button>
              }
            />
            <AlertDialogContent>
              <AlertDialogHeader>
                <AlertDialogTitle>{t('common:confirm_title')}</AlertDialogTitle>
                <AlertDialogDescription>{t('common:confirm_delete_message')}</AlertDialogDescription>
              </AlertDialogHeader>
              <AlertDialogFooter>
                <AlertDialogCancel>{t('common:cancel')}</AlertDialogCancel>
                <AlertDialogAction onClick={handleBatchDelete} variant="destructive">
                  {t('common:delete')}
                </AlertDialogAction>
              </AlertDialogFooter>
            </AlertDialogContent>
          </AlertDialog>
        )}
      </div>

      {servers.length === 0 ? (
        <div className="flex min-h-[300px] items-center justify-center rounded-lg border border-dashed">
          <div className="text-center">
            <p className="text-muted-foreground text-sm">{t('no_servers_title')}</p>
            <p className="mt-1 text-muted-foreground text-xs">{t('no_servers_description')}</p>
          </div>
        </div>
      ) : (
        <DataTable noResults={t('no_servers_title')} table={table} />
      )}

      {filtered.length > 0 && (
        <p className="mt-3 text-muted-foreground text-xs">
          {t('showing_servers', { shown: filtered.length, total: servers.length })}
          {selectedCount > 0 && ` · ${t('selected_count', { count: selectedCount })}`}
        </p>
      )}

      {editingId !== null && <EditWrapper onClose={() => setEditingId(null)} serverId={editingId} />}
    </div>
  )
}

function getBarColor(p: number): string {
  if (p > 90) {
    return 'bg-red-500'
  }
  if (p > 70) {
    return 'bg-amber-500'
  }
  return 'bg-emerald-500'
}

function MiniBar({ pct, sub }: { pct: number; sub?: string }) {
  const p = Math.min(100, Math.max(0, pct))
  const color = getBarColor(p)
  return (
    <div className="min-w-[80px]">
      <div className="flex items-center gap-2">
        <div className="h-1.5 flex-1 overflow-hidden rounded-full bg-muted">
          <div className={cn('h-full rounded-full', color)} style={{ width: `${p}%` }} />
        </div>
        <span className="w-10 text-right font-mono text-xs tabular-nums">{p.toFixed(0)}%</span>
      </div>
      {sub && <p className="mt-0.5 text-[10px] text-muted-foreground">{sub}</p>}
    </div>
  )
}

function EditWrapper({ serverId, onClose }: { onClose: () => void; serverId: string }) {
  const { data: server, isLoading } = useServer(serverId)

  if (isLoading) {
    return (
      <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50">
        <Skeleton className="size-8 rounded-full" />
      </div>
    )
  }

  if (!server) {
    return null
  }

  return <ServerEditDialog onClose={onClose} open server={server} />
}

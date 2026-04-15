import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { createFileRoute, Link } from '@tanstack/react-router'
import type { ColumnDef } from '@tanstack/react-table'
import { CircleDot, ExternalLink, LayoutGrid, Search, Table2, Tag, Trash2 } from 'lucide-react'
import { useMemo, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { toast } from 'sonner'
import { DataTable } from '@/components/data-table/data-table'
import { DataTableColumnHeader } from '@/components/data-table/data-table-column-header'
import { DataTableToolbar } from '@/components/data-table/data-table-toolbar'
import { ServerCard } from '@/components/server/server-card'
import { ServerEditDialog } from '@/components/server/server-edit-dialog'
import { StatusBadge } from '@/components/server/status-badge'
import { UpgradeJobBadge } from '@/components/server/upgrade-job-badge'
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
import { Checkbox } from '@/components/ui/checkbox'
import { Input } from '@/components/ui/input'
import { Skeleton } from '@/components/ui/skeleton'
import { ToggleGroup, ToggleGroupItem } from '@/components/ui/toggle-group'
import { useServer } from '@/hooks/use-api'
import { useDataTable } from '@/hooks/use-data-table'
import type { ServerMetrics } from '@/hooks/use-servers-ws'
import { api } from '@/lib/api-client'
import type { ServerGroup } from '@/lib/api-schema'
import { countCleanupCandidates } from '@/lib/orphan-server-utils'
import { countryCodeToFlag, formatBytes, formatSpeed, formatUptime } from '@/lib/utils'
import { useUpgradeJobsStore } from '@/stores/upgrade-jobs-store'
import { CpuCell, MiniBar } from './index.cells'

function UpgradeBadgeCell({ serverId }: { serverId: string }) {
  const job = useUpgradeJobsStore((state) => state.jobs.get(serverId))
  return <UpgradeJobBadge job={job} />
}

export const Route = createFileRoute('/_authed/servers/')({
  component: ServersListPage,
  validateSearch: (search: Record<string, unknown>) => ({
    ...search,
    q: (search.q as string) || '',
    view: (search.view === 'grid' ? 'grid' : 'table') as 'table' | 'grid'
  })
})

const arrayIncludesFilter = (row: { getValue: (id: string) => unknown }, id: string, value: unknown) => {
  if (!Array.isArray(value) || value.length === 0) {
    return true
  }
  return value.includes(String(row.getValue(id) ?? ''))
}

function ServersListPage() {
  const { t } = useTranslation(['servers', 'common'])
  const queryClient = useQueryClient()
  const navigate = Route.useNavigate()
  const { q: search, view: viewParam } = Route.useSearch()

  const [viewMode, setViewModeState] = useState<'table' | 'grid'>(() => {
    if (viewParam === 'table' || viewParam === 'grid') {
      return viewParam
    }
    return (localStorage.getItem('serverbee-servers-view-mode') as 'table' | 'grid') || 'table'
  })

  const setViewMode = (value: 'table' | 'grid') => {
    setViewModeState(value)
    localStorage.setItem('serverbee-servers-view-mode', value)
    navigate({ search: (prev) => ({ ...prev, view: value }) })
  }

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

  const groupOptions = useMemo(
    () =>
      (groups ?? []).map((g) => ({
        label: g.name,
        value: g.id
      })),
    [groups]
  )

  const statusOptions = useMemo(
    () => [
      { label: t('servers:status_online'), value: 'online' },
      { label: t('servers:status_offline'), value: 'offline' }
    ],
    [t]
  )

  const columns = useMemo<ColumnDef<ServerMetrics>[]>(
    () => [
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
        size: 36,
        meta: { className: 'w-9' }
      },
      {
        accessorKey: 'name',
        id: 'name',
        header: ({ column }) => <DataTableColumnHeader column={column} label={t('col_name')} />,
        cell: ({ row }) => {
          const s = row.original
          const flag = countryCodeToFlag(s.country_code)
          return (
            <div className="flex min-w-0 items-center gap-1.5">
              <Link
                className="group/link flex min-w-0 items-center gap-1.5"
                params={{ id: s.id }}
                search={{ range: 'realtime' }}
                to="/servers/$id"
              >
                {flag && <span className="text-xs">{flag}</span>}
                <span className="truncate font-medium group-hover/link:underline">{s.name}</span>
              </Link>
              <UpgradeBadgeCell serverId={s.id} />
            </div>
          )
        },
        size: 260,
        meta: { className: 'min-w-[200px]' }
      },
      {
        id: 'status',
        accessorFn: (row) => (row.online ? 'online' : 'offline'),
        header: ({ column }) => <DataTableColumnHeader column={column} label={t('col_status')} />,
        cell: ({ row }) => <StatusBadge online={row.original.online} />,
        filterFn: arrayIncludesFilter,
        enableColumnFilter: true,
        size: 84,
        meta: {
          className: 'w-[84px]',
          label: t('col_status'),
          variant: 'select',
          options: statusOptions,
          icon: CircleDot
        }
      },
      {
        accessorKey: 'cpu',
        id: 'cpu',
        header: ({ column }) => <DataTableColumnHeader column={column} label={t('col_cpu')} />,
        cell: ({ row }) => <CpuCell server={row.original} />,
        size: 160,
        meta: { className: 'w-[160px]' }
      },
      {
        accessorFn: (row) => (row.mem_total > 0 ? row.mem_used / row.mem_total : 0),
        id: 'memory',
        header: ({ column }) => <DataTableColumnHeader column={column} label={t('col_memory')} />,
        cell: ({ row }) => {
          const s = row.original
          const memPct = s.mem_total > 0 ? (s.mem_used / s.mem_total) * 100 : 0
          return <MiniBar pct={memPct} sub={<span>{formatBytes(s.mem_used)}</span>} />
        },
        size: 160,
        meta: { className: 'w-[160px]' }
      },
      {
        accessorFn: (row) => (row.disk_total > 0 ? row.disk_used / row.disk_total : 0),
        id: 'disk',
        header: ({ column }) => <DataTableColumnHeader column={column} label={t('col_disk')} />,
        cell: ({ row }) => {
          const s = row.original
          const diskPct = s.disk_total > 0 ? (s.disk_used / s.disk_total) * 100 : 0
          return <MiniBar pct={diskPct} sub={<span>{formatBytes(s.disk_used)}</span>} />
        },
        size: 160,
        meta: { className: 'w-[160px]' }
      },
      {
        id: 'network',
        enableSorting: false,
        header: () => <span className="text-muted-foreground text-xs">{t('col_network')}</span>,
        cell: ({ row }) => {
          const s = row.original
          return (
            <span className="font-mono text-muted-foreground text-xs tabular-nums">
              <span className="inline-block min-w-[64px]">↓{formatSpeed(s.net_in_speed)}</span>
              <span className="ml-2 inline-block min-w-[64px]">↑{formatSpeed(s.net_out_speed)}</span>
            </span>
          )
        },
        size: 160,
        meta: { className: 'hidden lg:table-cell lg:w-[160px]' }
      },
      {
        accessorKey: 'uptime',
        id: 'uptime',
        header: ({ column }) => <DataTableColumnHeader column={column} label={t('col_uptime')} />,
        cell: ({ row }) => {
          const s = row.original
          return (
            <span className="font-mono text-muted-foreground text-xs tabular-nums">
              {s.online ? formatUptime(s.uptime) : '-'}
            </span>
          )
        },
        size: 100,
        meta: { className: 'hidden xl:table-cell xl:w-[100px]' }
      },
      {
        id: 'group',
        accessorFn: (row) => row.group_id ?? '',
        header: ({ column }) => <DataTableColumnHeader column={column} label={t('col_group')} />,
        cell: ({ row }) => {
          const s = row.original
          return (
            <span className="text-muted-foreground text-xs">
              {s.group_id ? (groupMap.get(s.group_id) ?? '-') : '-'}
            </span>
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
            aria-label={t('servers:detail_edit')}
            className="rounded p-1 text-muted-foreground hover:bg-muted hover:text-foreground"
            onClick={() => setEditingId(row.original.id)}
            type="button"
          >
            <ExternalLink aria-hidden="true" className="size-3.5" />
          </button>
        ),
        size: 40,
        meta: { className: 'w-10' }
      }
    ],
    [t, groupMap, groupOptions, statusOptions]
  )

  const { table } = useDataTable({
    data: filtered,
    columns,
    pageCount: -1,
    initialState: {
      sorting: [{ id: 'name', desc: false }],
      pagination: { pageIndex: 0, pageSize: 20 }
    },
    getRowId: (row) => row.id
  })

  const selectedIds = table.getSelectedRowModel().rows.map((r) => r.original.id)
  const selectedCount = selectedIds.length

  const orphanCount = countCleanupCandidates(servers)

  const cleanupMutation = useMutation({
    mutationFn: () => api.delete<{ deleted_count: number }>('/api/servers/cleanup'),
    onSuccess: (data) => {
      queryClient.invalidateQueries({ queryKey: ['servers'] })
      toast.success(t('servers:cleanup_success', { count: data.deleted_count }))
    },
    onError: (err) => {
      toast.error(err instanceof Error ? err.message : t('common:errors.operation_failed'))
    }
  })

  const batchDeleteMutation = useMutation({
    mutationFn: (ids: string[]) => api.post<{ deleted: number }>('/api/servers/batch-delete', { ids }),
    onSuccess: () => {
      table.toggleAllRowsSelected(false)
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
        toast.success(t('toast_deleted', { count }))
      },
      onError: (err) => {
        toast.error(err instanceof Error ? err.message : t('toast_batch_delete_failed'))
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
        <ToggleGroup
          multiple={false}
          onValueChange={(value) => value.length > 0 && setViewMode(value[0] as 'table' | 'grid')}
          size="sm"
          value={[viewMode]}
          variant="outline"
        >
          <ToggleGroupItem aria-label={t('common:a11y.table_view')} value="table">
            <Table2 className="size-4" />
          </ToggleGroupItem>
          <ToggleGroupItem aria-label={t('common:a11y.grid_view')} value="grid">
            <LayoutGrid className="size-4" />
          </ToggleGroupItem>
        </ToggleGroup>
        {orphanCount > 0 && (
          <AlertDialog>
            <AlertDialogTrigger
              render={
                <Button disabled={cleanupMutation.isPending} size="sm" variant="outline">
                  {t('servers:cleanup_orphans')} ({orphanCount})
                </Button>
              }
            />
            <AlertDialogContent>
              <AlertDialogHeader>
                <AlertDialogTitle>{t('servers:cleanup_confirm_title')}</AlertDialogTitle>
                <AlertDialogDescription>
                  {t('servers:cleanup_confirm_description', { count: orphanCount })}
                </AlertDialogDescription>
              </AlertDialogHeader>
              <AlertDialogFooter>
                <AlertDialogCancel>{t('common:cancel')}</AlertDialogCancel>
                <AlertDialogAction onClick={() => cleanupMutation.mutate()} variant="destructive">
                  {t('common:delete')}
                </AlertDialogAction>
              </AlertDialogFooter>
            </AlertDialogContent>
          </AlertDialog>
        )}
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

      {servers.length === 0 && (
        <div className="flex min-h-[300px] items-center justify-center rounded-lg border border-dashed">
          <div className="text-center">
            <p className="text-muted-foreground text-sm">{t('no_servers_title')}</p>
            <p className="mt-1 text-muted-foreground text-xs">{t('no_servers_description')}</p>
          </div>
        </div>
      )}
      {servers.length > 0 && viewMode === 'table' && (
        <DataTable table={table}>
          <DataTableToolbar table={table} />
        </DataTable>
      )}
      {servers.length > 0 && viewMode === 'grid' && (
        <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-3">
          {filtered.map((server) => (
            <div className="[contain-intrinsic-size:auto_280px] [content-visibility:auto]" key={server.id}>
              <ServerCard server={server} />
            </div>
          ))}
        </div>
      )}

      {editingId !== null && <EditWrapper onClose={() => setEditingId(null)} serverId={editingId} />}
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

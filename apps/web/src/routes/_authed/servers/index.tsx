import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { createFileRoute } from '@tanstack/react-router'
import type { ColumnDef } from '@tanstack/react-table'
import { CircleDot, ExternalLink, LayoutGrid, ListChecks, Plus, Search, Table2, Tag, Trash2 } from 'lucide-react'
import { useMemo, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { toast } from 'sonner'
import { DataTable } from '@/components/data-table/data-table'
import { DataTableColumnHeader } from '@/components/data-table/data-table-column-header'
import { DataTableToolbar } from '@/components/data-table/data-table-toolbar'
import { AddServerDialog } from '@/components/server/add-server-dialog'
import { CostCell } from '@/components/server/cost-cell'
import { ServerCard } from '@/components/server/server-card'
import { ServerEditDialog } from '@/components/server/server-edit-dialog'
import { deriveServerStatus, StatusDot } from '@/components/server/status-dot'
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
import { useAuth } from '@/hooks/use-auth'
import { useCostOverview } from '@/hooks/use-cost'
import { useDataTable } from '@/hooks/use-data-table'
import { useNetworkOverview, useNetworkSetting } from '@/hooks/use-network-api'
import { useScrollViewportHeight } from '@/hooks/use-scroll-viewport-height'
import { reconcileServersFromRest, type ServerMetrics } from '@/hooks/use-servers-ws'
import { useTrafficOverview } from '@/hooks/use-traffic-overview'
import { api } from '@/lib/api-client'
import type { ServerGroup, ServerResponse } from '@/lib/api-schema'
import { withMockServers } from '@/lib/dev-mock-servers'
import { countCleanupCandidates } from '@/lib/orphan-server-utils'
import { cn } from '@/lib/utils'
import { useUpgradeJobsStore } from '@/stores/upgrade-jobs-store'
import { CpuCell, DiskCell, MemoryCell, NameCell, NetworkCell, UptimeCell } from './components/index-cells'
import { getInitialServersView } from './components/mobile-view'

function UpgradeBadgeCell({ serverId }: { serverId: string }) {
  const job = useUpgradeJobsStore((state) => state.jobs.get(serverId))
  return <UpgradeJobBadge job={job} />
}

export const Route = createFileRoute('/_authed/servers/')({
  component: ServersListPage,
  validateSearch: (search: Record<string, unknown>) => ({
    ...search,
    q: (search.q as string) || '',
    view: search.view === 'grid' || search.view === 'table' ? search.view : undefined
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
  const { user } = useAuth()
  const isAdmin = user?.role === 'admin'
  const [addOpen, setAddOpen] = useState(false)
  const [selectMode, setSelectMode] = useState(false)
  const navigate = Route.useNavigate()
  const { q: search, view: viewParam } = Route.useSearch()
  const { ref: fillRef, height: viewportHeight } = useScrollViewportHeight<HTMLDivElement>()

  const [viewMode, setViewModeState] = useState<'table' | 'grid'>(() =>
    getInitialServersView(viewParam === 'grid' || viewParam === 'table' ? viewParam : undefined)
  )

  const setViewMode = (value: 'table' | 'grid') => {
    setViewModeState(value)
    localStorage.setItem('serverbee-servers-view-mode', value)
    navigate({ search: (prev) => ({ ...prev, view: value }) })
  }

  const { data: rawServers = [] } = useQuery<ServerMetrics[]>({
    queryKey: ['servers'],
    queryFn: () => [],
    staleTime: Number.POSITIVE_INFINITY,
    refetchOnMount: false,
    refetchOnWindowFocus: false
  })
  const servers = useMemo(() => withMockServers(rawServers), [rawServers])

  const { data: groups } = useQuery<ServerGroup[]>({
    queryKey: ['server-groups'],
    queryFn: () => api.get<ServerGroup[]>('/api/server-groups'),
    staleTime: 60_000
  })

  const { data: trafficOverview = [] } = useTrafficOverview()
  const { data: costOverview } = useCostOverview()
  const { data: networkOverview = [] } = useNetworkOverview()
  const { data: networkSetting } = useNetworkSetting()

  const setSearch = (value: string) => navigate({ search: (prev) => ({ ...prev, q: value }) })
  const [editingId, setEditingId] = useState<string | null>(null)

  const groupMap = useMemo(() => new Map(groups?.map((g) => [g.id, g.name]) ?? []), [groups])
  const costByServerId = useMemo(() => {
    const entries = costOverview?.servers ?? []
    return new Map(entries.map((entry) => [entry.server_id, entry]))
  }, [costOverview])
  const trafficByServerId = useMemo(
    () => new Map(trafficOverview.map((entry) => [entry.server_id, entry])),
    [trafficOverview]
  )
  const networkSummaryByServerId = useMemo(
    () => new Map(networkOverview.map((entry) => [entry.server_id, entry])),
    [networkOverview]
  )
  const networkBucketSeconds = Math.max(networkSetting?.interval ?? 60, 60)

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
        cell: ({ row }) => (
          <NameCell rightSlot={<UpgradeBadgeCell serverId={row.original.id} />} server={row.original} />
        ),
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
    [t, costByServerId, groupMap, groupOptions, statusOptions, trafficOverview, selectMode]
  )

  const { table } = useDataTable({
    data: filtered,
    columns,
    pageCount: -1,
    initialState: {
      sorting: [{ id: 'name', desc: false }],
      pagination: { pageIndex: 0, pageSize: 20 },
      columnVisibility: { group: false, 'status-dot': false }
    },
    getRowId: (row) => row.id
  })

  const selectedIds = table.getSelectedRowModel().rows.map((r) => r.original.id)
  const selectedCount = selectedIds.length

  const orphanCount = countCleanupCandidates(servers)

  const cleanupMutation = useMutation({
    mutationFn: () => api.delete<{ deleted_count: number }>('/api/servers/cleanup'),
    onSuccess: async (data) => {
      // ['servers'] is a WS-fed cache (queryFn: () => []); refresh membership
      // from REST instead of invalidating (which would wipe the visible list).
      try {
        const fresh = await api.get<ServerResponse[]>('/api/servers')
        queryClient.setQueryData<ServerMetrics[]>(['servers'], (prev) =>
          reconcileServersFromRest(prev, fresh as unknown as Array<Partial<ServerMetrics> & { id: string }>)
        )
      } catch {
        // Best-effort: next WS full_sync will reconcile.
      }
      toast.success(t('servers:cleanup_success', { count: data.deleted_count }))
    },
    onError: (err) => {
      toast.error(err instanceof Error ? err.message : t('common:errors.operation_failed'))
    }
  })

  const batchDeleteMutation = useMutation({
    mutationFn: (ids: string[]) => api.post<{ deleted: number }>('/api/servers/batch-delete', { ids }),
    onSuccess: (_data, ids) => {
      table.toggleAllRowsSelected(false)
      const removed = new Set(ids)
      // Same WS-cache caveat: filter the deleted ids out in place.
      queryClient.setQueryData<ServerMetrics[]>(['servers'], (prev) => prev?.filter((s) => !removed.has(s.id)))
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

  const viewToggle = (
    <ToggleGroup
      multiple={false}
      onValueChange={(value) => value.length > 0 && setViewMode(value[0] as 'table' | 'grid')}
      size="default"
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
  )

  const cleanupButton = orphanCount > 0 && (
    <AlertDialog>
      <AlertDialogTrigger
        render={
          <Button disabled={cleanupMutation.isPending} size="default" variant="outline">
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
  )

  const batchDeleteButton = selectedCount > 0 && (
    <AlertDialog>
      <AlertDialogTrigger
        render={
          <Button disabled={batchDeleteMutation.isPending} size="default" variant="destructive">
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
  )

  const toggleSelectMode = () => {
    setSelectMode((prev) => {
      if (prev) {
        table.toggleAllRowsSelected(false)
      }
      return !prev
    })
  }

  const selectModeButton = viewMode === 'table' && (
    <Button onClick={toggleSelectMode} size="default" variant={selectMode ? 'secondary' : 'outline'}>
      <ListChecks aria-hidden="true" className="size-4" />
      {selectMode ? t('servers:batch_select_exit') : t('servers:batch_select')}
    </Button>
  )

  const addServerButton = isAdmin && (
    <Button onClick={() => setAddOpen(true)} size="default">
      <Plus className="size-4" />
      {t('add_server.button')}
    </Button>
  )

  const rowActions = (
    <>
      {viewToggle}
      {cleanupButton}
      {batchDeleteButton}
      {addServerButton}
    </>
  )

  return (
    <div
      className={cn(
        'w-full min-w-0 max-w-[calc(100vw-1.5rem)] overflow-hidden sm:max-w-full',
        viewMode === 'table' && 'flex min-h-0 flex-col'
      )}
      ref={fillRef}
      style={viewMode === 'table' && viewportHeight ? { height: viewportHeight } : undefined}
    >
      <div className="mb-4 flex flex-col gap-3 sm:flex-row sm:items-center">
        <div className="relative min-w-0 flex-1 sm:max-w-sm">
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
        {viewMode === 'table' ? (
          <DataTableToolbar className="w-full p-0 sm:w-auto sm:flex-1" table={table} trailingActions={selectModeButton}>
            {rowActions}
          </DataTableToolbar>
        ) : (
          <div className="flex flex-wrap items-center gap-2 sm:ml-auto sm:justify-end">{rowActions}</div>
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
        <DataTable fillHeight rowClassName={(row) => !row.original.online && 'opacity-45 grayscale'} table={table} />
      )}
      {servers.length > 0 && viewMode === 'grid' && (
        <div className="grid gap-4" style={{ gridTemplateColumns: 'repeat(auto-fill, minmax(320px, 1fr))' }}>
          {filtered.map((server) => (
            <div className="[contain-intrinsic-size:auto_280px] [content-visibility:auto]" key={server.id}>
              <ServerCard
                costEntry={costByServerId.get(server.id)}
                networkBucketSeconds={networkBucketSeconds}
                networkSummary={networkSummaryByServerId.get(server.id)}
                server={server}
                trafficEntry={trafficByServerId.get(server.id)}
              />
            </div>
          ))}
        </div>
      )}

      {editingId !== null && <EditWrapper onClose={() => setEditingId(null)} serverId={editingId} />}
      {isAdmin && <AddServerDialog onClose={() => setAddOpen(false)} open={addOpen} />}
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

import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { createFileRoute } from '@tanstack/react-router'
import type { ColumnDef } from '@tanstack/react-table'
import { useMemo, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { toast } from 'sonner'
import { DataTable } from '@/components/data-table/data-table'
import { PageBody } from '@/components/layout/page-body'
import { AddServerDialog } from '@/components/server/add-server-dialog'
import { ServerCard } from '@/components/server/server-card'
import { ServerEditDialog } from '@/components/server/server-edit-dialog'
import { Skeleton } from '@/components/ui/skeleton'
import { useServer } from '@/hooks/use-api'
import { useAuth } from '@/hooks/use-auth'
import { useCostOverview } from '@/hooks/use-cost'
import { useDataTable } from '@/hooks/use-data-table'
import { useNetworkOverview, useNetworkSetting } from '@/hooks/use-network-api'
import { useScrollViewportHeight } from '@/hooks/use-scroll-viewport-height'
import { useTrafficOverview } from '@/hooks/use-traffic-overview'
import { api } from '@/lib/api-client'
import type { ServerGroup } from '@/lib/api-schema'
import { withMockServers } from '@/lib/dev-mock-servers'
import { countCleanupCandidates } from '@/lib/orphan-server-utils'
import { projectServerCatalog, refreshServerCatalog, type ServerMetrics, useLiveServers } from '@/lib/server-catalog'
import { cn } from '@/lib/utils'
import { getInitialServersView } from './components/mobile-view'
import { buildServerColumns } from './components/server-columns'
import { ServersEmptyState, ServersNoResults } from './components/servers-empty-state'
import { ServersPageToolbar } from './components/servers-page-toolbar'

export const Route = createFileRoute('/_authed/servers/')({
  component: ServersListPage,
  validateSearch: (search: Record<string, unknown>) => ({
    ...search,
    // Coerce at runtime, not just via a type assertion: the router parses a
    // numeric URL param (e.g. ?q=1) into a number, and `as string` would let it
    // through, crashing the filter's `search.toLowerCase()`.
    q: String(search.q ?? ''),
    view: search.view === 'grid' || search.view === 'table' ? search.view : undefined
  })
})

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
    try {
      localStorage.setItem('serverbee-servers-view-mode', value)
    } catch {
      // ignore storage failures (private mode / quota)
    }
    navigate({ search: (prev) => ({ ...prev, view: value }) })
  }

  const { data: rawServers = [] } = useLiveServers()
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
    () =>
      buildServerColumns({
        t,
        costByServerId,
        groupMap,
        groupOptions,
        statusOptions,
        selectMode,
        onEdit: setEditingId,
        trafficOverview
      }),
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
  const hasNoMatches = servers.length > 0 && filtered.length === 0
  const hasResults = filtered.length > 0

  const cleanupMutation = useMutation({
    mutationFn: () => api.delete<{ deleted_count: number }>('/api/servers/cleanup'),
    onSuccess: async (data) => {
      try {
        await refreshServerCatalog(queryClient)
      } catch {
        // Best-effort: next WS full_sync will reconcile.
      }
      toast.success(t('servers:cleanup_success', { count: data.deleted_count }))
    },
    onError: () => {
      toast.error(t('toast_cleanup_failed'))
    }
  })

  const batchDeleteMutation = useMutation({
    mutationFn: (ids: string[]) => api.post<{ deleted: number }>('/api/servers/batch-delete', { ids }),
    onSuccess: (_data, ids) => {
      table.toggleAllRowsSelected(false)
      projectServerCatalog(queryClient, { kind: 'servers_removed', serverIds: ids })
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
      onError: () => {
        toast.error(t('toast_batch_delete_failed'))
      }
    })
  }

  const toggleSelectMode = () => {
    setSelectMode((prev) => {
      if (prev) {
        table.toggleAllRowsSelected(false)
      }
      return !prev
    })
  }

  return (
    <PageBody>
      <div
        className={cn(
          // The mobile max-w is the shared page-root clamp: without a definite
          // width ceiling the table's intrinsic min-w-max propagates up through
          // <main> and the DataTable's ScrollArea stops scrolling horizontally.
          // It matches PageBody's p-3 exactly, so it never narrows the grid view.
          'w-full min-w-0 max-w-[calc(100vw-1.5rem)] sm:max-w-full',
          viewMode === 'table' && 'flex min-h-0 flex-col'
        )}
        ref={fillRef}
        style={viewMode === 'table' && viewportHeight ? { height: viewportHeight } : undefined}
      >
        {/* The toolbar carries no visible title; without it the heading outline
          starts at the card/table headings. */}
        <h1 className="sr-only">{t('servers:title')}</h1>
        <ServersPageToolbar
          batchDeletePending={batchDeleteMutation.isPending}
          cleanupPending={cleanupMutation.isPending}
          isAdmin={isAdmin}
          onAddServer={() => setAddOpen(true)}
          onBatchDelete={handleBatchDelete}
          onCleanup={() => cleanupMutation.mutate()}
          onSearchChange={setSearch}
          onToggleSelectMode={toggleSelectMode}
          onViewModeChange={setViewMode}
          orphanCount={orphanCount}
          search={search}
          selectedCount={selectedCount}
          selectMode={selectMode}
          table={table}
          viewMode={viewMode}
        />

        {servers.length === 0 && <ServersEmptyState />}
        {hasNoMatches && <ServersNoResults onClear={() => setSearch('')} query={search} />}
        {hasResults && viewMode === 'table' && (
          <DataTable fillHeight rowClassName={(row) => !row.original.online && 'grayscale'} table={table} />
        )}
        {hasResults && viewMode === 'grid' && (
          <div
            className="grid gap-4"
            // min(320px, 100%) keeps the track from overflowing viewports narrower
            // than the ideal card width instead of clipping the cards.
            style={{ gridTemplateColumns: 'repeat(auto-fill, minmax(min(320px, 100%), 1fr))' }}
          >
            {filtered.map((server) => (
              <div className="-m-1 p-1 [contain-intrinsic-size:auto_280px] [content-visibility:auto]" key={server.id}>
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
    </PageBody>
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

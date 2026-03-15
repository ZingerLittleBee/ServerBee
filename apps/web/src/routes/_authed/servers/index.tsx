import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { createFileRoute, Link } from '@tanstack/react-router'
import { ExternalLink, Search, Trash2 } from 'lucide-react'
import { useMemo, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { ServerEditDialog } from '@/components/server/server-edit-dialog'
import { StatusBadge } from '@/components/server/status-badge'
import { useServer } from '@/hooks/use-api'
import type { ServerMetrics } from '@/hooks/use-servers-ws'
import { api } from '@/lib/api-client'
import type { ServerGroup } from '@/lib/api-schema'
import { cn, countryCodeToFlag, formatBytes, formatSpeed, formatUptime } from '@/lib/utils'

export const Route = createFileRoute('/_authed/servers/')({
  component: ServersListPage
})

type SortKey = 'name' | 'status' | 'cpu' | 'memory' | 'disk' | 'uptime' | 'group'
type SortDir = 'asc' | 'desc'

function memPctOf(s: ServerMetrics): number {
  return s.mem_total > 0 ? s.mem_used / s.mem_total : 0
}

function diskPctOf(s: ServerMetrics): number {
  return s.disk_total > 0 ? s.disk_used / s.disk_total : 0
}

function compareServers(a: ServerMetrics, b: ServerMetrics, sortKey: SortKey, groupMap: Map<string, string>): number {
  switch (sortKey) {
    case 'name':
      return a.name.localeCompare(b.name)
    case 'status':
      return Number(b.online) - Number(a.online)
    case 'cpu':
      return a.cpu - b.cpu
    case 'memory':
      return memPctOf(a) - memPctOf(b)
    case 'disk':
      return diskPctOf(a) - diskPctOf(b)
    case 'uptime':
      return a.uptime - b.uptime
    case 'group': {
      const aG = (a.group_id && groupMap.get(a.group_id)) || ''
      const bG = (b.group_id && groupMap.get(b.group_id)) || ''
      return aG.localeCompare(bG)
    }
    default:
      return 0
  }
}

function ServersListPage() {
  const { t } = useTranslation('servers')
  const queryClient = useQueryClient()
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

  const [search, setSearch] = useState('')
  const [sortKey, setSortKey] = useState<SortKey>('name')
  const [sortDir, setSortDir] = useState<SortDir>('asc')
  const [selected, setSelected] = useState<Set<string>>(new Set())
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

  const sorted = useMemo(() => {
    const list = [...filtered]
    const dir = sortDir === 'asc' ? 1 : -1
    list.sort((a, b) => compareServers(a, b, sortKey, groupMap) * dir)
    return list
  }, [filtered, sortKey, sortDir, groupMap])

  const toggleSort = (key: SortKey) => {
    if (sortKey === key) {
      setSortDir(sortDir === 'asc' ? 'desc' : 'asc')
    } else {
      setSortKey(key)
      setSortDir('asc')
    }
  }

  const allSelected = sorted.length > 0 && selected.size === sorted.length
  const toggleAll = () => {
    if (allSelected) {
      setSelected(new Set())
    } else {
      setSelected(new Set(sorted.map((s) => s.id)))
    }
  }
  const toggleOne = (id: string) => {
    const next = new Set(selected)
    if (next.has(id)) {
      next.delete(id)
    } else {
      next.add(id)
    }
    setSelected(next)
  }

  const batchDeleteMutation = useMutation({
    mutationFn: (ids: string[]) => api.post<{ deleted: number }>('/api/servers/batch-delete', { ids }),
    onSuccess: () => {
      setSelected(new Set())
      queryClient.invalidateQueries({ queryKey: ['servers'] })
    }
  })

  const handleBatchDelete = () => {
    if (selected.size === 0) {
      return
    }
    batchDeleteMutation.mutate([...selected])
  }

  const sortIcon = (key: SortKey) => {
    if (sortKey !== key) {
      return null
    }
    return sortDir === 'asc' ? ' ↑' : ' ↓'
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
          <input
            className="h-9 w-full rounded-md border bg-background pr-3 pl-9 text-sm outline-none focus:ring-2 focus:ring-ring"
            onChange={(e) => setSearch(e.target.value)}
            placeholder={t('search_placeholder')}
            type="text"
            value={search}
          />
        </div>
        {selected.size > 0 && (
          <button
            className="inline-flex h-9 items-center gap-1.5 rounded-md bg-destructive px-3 text-destructive-foreground text-sm hover:bg-destructive/90 disabled:opacity-50"
            disabled={batchDeleteMutation.isPending}
            onClick={handleBatchDelete}
            type="button"
          >
            <Trash2 className="size-3.5" />
            {t('delete_selected', { count: selected.size })}
          </button>
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
        <div className="overflow-hidden rounded-lg border">
          <table className="w-full text-sm">
            <thead>
              <tr className="border-b bg-muted/50">
                <th className="w-10 px-3 py-2.5">
                  <input checked={allSelected} className="rounded" onChange={toggleAll} type="checkbox" />
                </th>
                <Th
                  active={sortKey === 'name'}
                  label={t('col_name')}
                  onClick={() => toggleSort('name')}
                  suffix={sortIcon('name')}
                />
                <Th
                  active={sortKey === 'status'}
                  label={t('col_status')}
                  onClick={() => toggleSort('status')}
                  suffix={sortIcon('status')}
                />
                <Th
                  active={sortKey === 'cpu'}
                  label={t('col_cpu')}
                  onClick={() => toggleSort('cpu')}
                  suffix={sortIcon('cpu')}
                />
                <Th
                  active={sortKey === 'memory'}
                  label={t('col_memory')}
                  onClick={() => toggleSort('memory')}
                  suffix={sortIcon('memory')}
                />
                <Th
                  active={sortKey === 'disk'}
                  label={t('col_disk')}
                  onClick={() => toggleSort('disk')}
                  suffix={sortIcon('disk')}
                />
                <th className="hidden px-3 py-2.5 text-left font-medium text-muted-foreground lg:table-cell">
                  {t('col_network')}
                </th>
                <Th
                  active={sortKey === 'uptime'}
                  className="hidden xl:table-cell"
                  label={t('col_uptime')}
                  onClick={() => toggleSort('uptime')}
                  suffix={sortIcon('uptime')}
                />
                <Th
                  active={sortKey === 'group'}
                  className="hidden xl:table-cell"
                  label={t('col_group')}
                  onClick={() => toggleSort('group')}
                  suffix={sortIcon('group')}
                />
                <th className="w-10 px-3 py-2.5" />
              </tr>
            </thead>
            <tbody>
              {sorted.map((s) => {
                const memPct = s.mem_total > 0 ? (s.mem_used / s.mem_total) * 100 : 0
                const diskPct = s.disk_total > 0 ? (s.disk_used / s.disk_total) * 100 : 0
                const flag = countryCodeToFlag(s.country_code)
                return (
                  <tr
                    className={cn(
                      'border-b transition-colors last:border-b-0 hover:bg-muted/30',
                      selected.has(s.id) && 'bg-muted/20'
                    )}
                    key={s.id}
                  >
                    <td className="px-3 py-2">
                      <input
                        checked={selected.has(s.id)}
                        className="rounded"
                        onChange={() => toggleOne(s.id)}
                        type="checkbox"
                      />
                    </td>
                    <td className="px-3 py-2">
                      <Link className="group/link flex items-center gap-1.5" params={{ id: s.id }} to="/servers/$id">
                        {flag && <span className="text-xs">{flag}</span>}
                        <span className="font-medium group-hover/link:underline">{s.name}</span>
                      </Link>
                    </td>
                    <td className="px-3 py-2">
                      <StatusBadge online={s.online} />
                    </td>
                    <td className="px-3 py-2">
                      <MiniBar pct={s.cpu} />
                    </td>
                    <td className="px-3 py-2">
                      <MiniBar pct={memPct} sub={formatBytes(s.mem_used)} />
                    </td>
                    <td className="px-3 py-2">
                      <MiniBar pct={diskPct} sub={formatBytes(s.disk_used)} />
                    </td>
                    <td className="hidden px-3 py-2 text-muted-foreground text-xs lg:table-cell">
                      <span>↓{formatSpeed(s.net_in_speed)}</span>
                      <span className="ml-2">↑{formatSpeed(s.net_out_speed)}</span>
                    </td>
                    <td className="hidden px-3 py-2 text-muted-foreground text-xs xl:table-cell">
                      {s.online ? formatUptime(s.uptime) : '-'}
                    </td>
                    <td className="hidden px-3 py-2 text-muted-foreground text-xs xl:table-cell">
                      {s.group_id ? (groupMap.get(s.group_id) ?? '-') : '-'}
                    </td>
                    <td className="px-3 py-2">
                      <button
                        className="rounded p-1 text-muted-foreground hover:bg-muted hover:text-foreground"
                        onClick={() => setEditingId(s.id)}
                        title="Edit server"
                        type="button"
                      >
                        <ExternalLink className="size-3.5" />
                      </button>
                    </td>
                  </tr>
                )
              })}
            </tbody>
          </table>
        </div>
      )}

      {filtered.length > 0 && (
        <p className="mt-3 text-muted-foreground text-xs">
          {t('showing_servers', { shown: sorted.length, total: servers.length })}
          {selected.size > 0 && ` · ${t('selected_count', { count: selected.size })}`}
        </p>
      )}

      {editingId !== null && <EditWrapper onClose={() => setEditingId(null)} serverId={editingId} />}
    </div>
  )
}

function Th({
  label,
  active,
  suffix,
  onClick,
  className
}: {
  active: boolean
  className?: string
  label: string
  onClick: () => void
  suffix: string | null
}) {
  return (
    <th className={cn('px-3 py-2.5 text-left', className)}>
      <button
        className={cn(
          'font-medium text-xs',
          active ? 'text-foreground' : 'text-muted-foreground hover:text-foreground'
        )}
        onClick={onClick}
        type="button"
      >
        {label}
        {suffix}
      </button>
    </th>
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
        <span className="w-10 text-right font-mono text-xs">{p.toFixed(0)}%</span>
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
        <div className="size-8 animate-spin rounded-full border-4 border-muted border-t-primary" />
      </div>
    )
  }

  if (!server) {
    return null
  }

  return <ServerEditDialog onClose={onClose} open server={server} />
}

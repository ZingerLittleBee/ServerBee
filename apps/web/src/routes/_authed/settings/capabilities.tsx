import { useQuery } from '@tanstack/react-query'
import { createFileRoute } from '@tanstack/react-router'
import { type ColumnDef, getCoreRowModel, useReactTable } from '@tanstack/react-table'
import { Check, Minus, Search, ShieldAlert } from 'lucide-react'
import { useMemo } from 'react'
import { useTranslation } from 'react-i18next'
import { TemporaryBadge } from '@/components/server/temporary-badge'
import { DataTable } from '@/components/ui/data-table'
import { Input } from '@/components/ui/input'
import { Skeleton } from '@/components/ui/skeleton'
import type { ServerMetrics } from '@/hooks/use-servers-ws'
import { CAPABILITIES, classifyCapability, temporaryGrantFor } from '@/lib/capabilities'

export const Route = createFileRoute('/_authed/settings/capabilities')({
  validateSearch: (search: Record<string, unknown>) => ({
    q: (search.q as string) || ''
  }),
  component: CapabilitiesPage
})

type ServerInfo = ServerMetrics

const ORDERED_CAPABILITIES = [
  ...CAPABILITIES.filter(({ risk }) => risk === 'high'),
  ...CAPABILITIES.filter(({ risk }) => risk !== 'high')
]

export function CapabilitiesPage() {
  const { t } = useTranslation(['settings', 'servers'])
  const { q: search } = Route.useSearch()
  const navigate = Route.useNavigate()

  // The ['servers'] cache is fed by the global WebSocket layer. Capabilities are
  // agent-owned and reported by the agent over that channel, so this page is a
  // read-only mirror of what each agent has enabled in its config file.
  const { data: servers = [], isLoading } = useQuery<ServerInfo[]>({
    queryKey: ['servers'],
    queryFn: () => [],
    staleTime: Number.POSITIVE_INFINITY,
    refetchOnMount: false,
    refetchOnWindowFocus: false
  })

  const filtered = useMemo(
    () => servers.filter((s) => s.name.toLowerCase().includes(search.toLowerCase())),
    [servers, search]
  )

  const onlineCount = useMemo(() => servers.filter((s) => s.online).length, [servers])

  const columns = useMemo<ColumnDef<ServerInfo>[]>(
    () => [
      {
        accessorKey: 'name',
        header: () => t('capabilities.server'),
        cell: ({ row }) => {
          const hasOldAgent = row.original.protocol_version != null && row.original.protocol_version < 2
          const isOffline = !row.original.online
          return (
            <div className="flex items-center gap-2">
              <span
                aria-hidden="true"
                className={`size-2 rounded-full ${isOffline ? 'bg-muted-foreground/40' : 'bg-emerald-500'}`}
              />
              <span className={`font-medium ${isOffline ? 'text-muted-foreground' : ''}`}>{row.original.name}</span>
              {isOffline && (
                <span className="rounded bg-muted px-1.5 py-0.5 text-[10px] text-muted-foreground uppercase">
                  {t('capabilities.offline')}
                </span>
              )}
              {hasOldAgent && (
                <span title={t('cap_upgrade_warning', { ns: 'servers' })}>
                  <ShieldAlert aria-hidden="true" className="size-3.5 text-amber-500" />
                </span>
              )}
            </div>
          )
        },
        enableSorting: false
      },
      ...ORDERED_CAPABILITIES.map(
        ({ bit, key, labelKey, risk }) =>
          ({
            id: `cap_${key}`,
            header: () => (
              <div className="text-center">
                <div>{t(labelKey, { ns: 'servers' })}</div>
                <div className={`text-[10px] ${risk === 'high' ? 'text-red-500' : 'text-muted-foreground'}`}>
                  {t(risk === 'high' ? 'cap_high_risk' : 'cap_low_risk', { ns: 'servers' })}
                </div>
              </div>
            ),
            cell: ({ row }) => {
              const state = classifyCapability(row.original, bit)
              const label = `${t(labelKey, { ns: 'servers' })} - ${row.original.name}`
              return (
                <div className="flex justify-center">
                  {(() => {
                    if (state === 'temporary') {
                      return <TemporaryBadge expiresAt={temporaryGrantFor(row.original, bit)?.expires_at ?? null} />
                    }
                    if (state === 'enabled') {
                      return (
                        <Check
                          aria-label={`${label}: ${t('cap_enabled', { ns: 'servers' })}`}
                          className="size-4 text-emerald-500"
                        />
                      )
                    }
                    return (
                      <Minus
                        aria-label={`${label}: ${t('cap_disabled', { ns: 'servers' })}`}
                        className="size-4 text-muted-foreground/40"
                      />
                    )
                  })()}
                </div>
              )
            },
            enableSorting: false,
            meta: { className: 'text-center' }
          }) satisfies ColumnDef<ServerInfo>
      )
    ],
    [t]
  )

  const table = useReactTable({
    data: filtered,
    columns,
    getCoreRowModel: getCoreRowModel(),
    getRowId: (row) => row.id
  })

  const renderTableContent = () => {
    if (isLoading) {
      return (
        <div className="space-y-3">
          <Skeleton className="h-10 w-full" />
          <Skeleton className="h-12 w-full" />
          <Skeleton className="h-12 w-full" />
          <Skeleton className="h-12 w-full" />
        </div>
      )
    }
    if (servers.length === 0) {
      return (
        <div className="flex min-h-[200px] items-center justify-center rounded-lg border border-dashed">
          <p className="text-muted-foreground text-sm">{t('capabilities.no_servers')}</p>
        </div>
      )
    }
    return <DataTable noResults={t('capabilities.no_servers')} table={table} />
  }

  return (
    <div className="w-full min-w-0 max-w-[calc(100vw-1.5rem)] overflow-hidden sm:max-w-full">
      <p className="mb-6 min-w-0 text-muted-foreground text-sm">{t('capabilities.description')}</p>

      <div className="mb-4 flex min-w-0 flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
        <div className="relative w-full min-w-0 sm:max-w-sm sm:flex-1">
          <Search
            aria-hidden="true"
            className="absolute top-1/2 left-3 size-4 -translate-y-1/2 text-muted-foreground"
          />
          <Input
            autoComplete="off"
            className="pl-9"
            name="search"
            onChange={(e) => navigate({ search: { q: e.target.value } })}
            placeholder={t('capabilities.search')}
            type="text"
            value={search}
          />
        </div>
        <p className="text-muted-foreground text-sm sm:shrink-0">
          {t('capabilities.summary', { total: servers.length, online: onlineCount })}
        </p>
      </div>

      {renderTableContent()}

      {filtered.length > 0 && search.length > 0 && (
        <p className="mt-3 text-muted-foreground text-xs">
          {t('capabilities.footer_showing', { filtered: filtered.length, total: servers.length })}
        </p>
      )}
    </div>
  )
}

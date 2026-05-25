import { useMutation, useQuery } from '@tanstack/react-query'
import { createFileRoute } from '@tanstack/react-router'
import { type ColumnDef, getCoreRowModel, type RowSelectionState, useReactTable } from '@tanstack/react-table'
import { ChevronDown, RotateCcw, Search, ShieldAlert, X } from 'lucide-react'
import { useCallback, useMemo, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { toast } from 'sonner'
import { Button } from '@/components/ui/button'
import { createSelectColumn, DataTable } from '@/components/ui/data-table'
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSub,
  DropdownMenuSubContent,
  DropdownMenuSubTrigger,
  DropdownMenuTrigger
} from '@/components/ui/dropdown-menu'
import { Input } from '@/components/ui/input'
import { Skeleton } from '@/components/ui/skeleton'
import { Switch } from '@/components/ui/switch'
import type { ServerMetrics } from '@/hooks/use-servers-ws'
import { api } from '@/lib/api-client'
import { CAP_DEFAULT, CAPABILITIES, getEffectiveCapabilityEnabled, isClientCapabilityLocked } from '@/lib/capabilities'

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
  const [rowSelection, setRowSelection] = useState<RowSelectionState>({})

  const { data: servers = [], isLoading } = useQuery<ServerInfo[]>({
    queryKey: ['servers'],
    queryFn: () => [],
    staleTime: Number.POSITIVE_INFINITY,
    refetchOnMount: false,
    refetchOnWindowFocus: false
  })

  const { mutate: mutateSingleCap, isPending: isSinglePending } = useMutation({
    mutationFn: ({ id, capabilities }: { capabilities: number; id: string }) =>
      api.put(`/api/servers/${id}`, { capabilities }),
    onSuccess: () => {
      toast.success(t('capabilities.toast_updated'))
    },
    onError: (err) => {
      toast.error(err instanceof Error ? err.message : t('common:errors.operation_failed'))
    }
  })

  const { mutate: mutateBatchCap, isPending: isBatchPending } = useMutation({
    mutationFn: ({ ids, capabilities }: { capabilities: number; ids: string[] }) =>
      api.put('/api/servers/batch-capabilities', { ids, capabilities }),
    onSuccess: () => {
      setRowSelection({})
      toast.success(t('capabilities.toast_batch_updated'))
    },
    onError: (err) => {
      toast.error(err instanceof Error ? err.message : t('common:errors.operation_failed'))
    }
  })

  const filtered = useMemo(
    () => servers.filter((s) => s.name.toLowerCase().includes(search.toLowerCase())),
    [servers, search]
  )

  const toggleCap = useCallback(
    (server: ServerInfo, bit: number) => {
      const caps = server.capabilities ?? CAP_DEFAULT
      // biome-ignore lint/suspicious/noBitwiseOperators: intentional capability bitmask toggle
      const newCaps = caps & bit ? caps & ~bit : caps | bit
      mutateSingleCap({ id: server.id, capabilities: newCaps })
    },
    [mutateSingleCap]
  )

  const isPending = isSinglePending || isBatchPending
  const onlineCount = useMemo(() => servers.filter((s) => s.online).length, [servers])

  const columns = useMemo<ColumnDef<ServerInfo>[]>(
    () => [
      createSelectColumn<ServerInfo>(),
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
              const isEnabled = getEffectiveCapabilityEnabled(
                row.original.effective_capabilities,
                row.original.capabilities,
                bit
              )
              const isLocked = isClientCapabilityLocked(row.original.agent_local_capabilities, bit)
              const isOffline = !row.original.online
              let disabledReason: string | undefined
              if (isOffline) {
                disabledReason = t('capabilities.offline_disabled')
              } else if (isLocked) {
                disabledReason = t('capabilities.client_disabled')
              }
              return (
                <div className="text-center">
                  <Switch
                    aria-label={`${t(labelKey, { ns: 'servers' })} - ${row.original.name}`}
                    checked={isEnabled}
                    disabled={isPending || isLocked || isOffline}
                    onCheckedChange={() => toggleCap(row.original, bit)}
                    title={disabledReason}
                  />
                </div>
              )
            },
            enableSorting: false,
            meta: { className: 'text-center' }
          }) satisfies ColumnDef<ServerInfo>
      )
    ],
    [isPending, toggleCap, t]
  )

  const table = useReactTable({
    data: filtered,
    columns,
    state: { rowSelection },
    onRowSelectionChange: setRowSelection,
    getCoreRowModel: getCoreRowModel(),
    getRowId: (row) => row.id
  })

  const selectedIds = table.getSelectedRowModel().rows.map((r) => r.original.id)

  const batchEnable = (bit: number) => {
    const firstServer = servers.find((s) => s.id === selectedIds[0])
    const baseCaps = firstServer?.capabilities ?? CAP_DEFAULT
    // biome-ignore lint/suspicious/noBitwiseOperators: intentional capability bitmask enable
    mutateBatchCap({ ids: selectedIds, capabilities: baseCaps | bit })
  }

  const batchDisable = (bit: number) => {
    const firstServer = servers.find((s) => s.id === selectedIds[0])
    const baseCaps = firstServer?.capabilities ?? CAP_DEFAULT
    // biome-ignore lint/suspicious/noBitwiseOperators: intentional capability bitmask disable
    mutateBatchCap({ ids: selectedIds, capabilities: baseCaps & ~bit })
  }

  const batchReset = () => {
    mutateBatchCap({ ids: selectedIds, capabilities: CAP_DEFAULT })
  }

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

      {selectedIds.length > 0 && (
        <div className="mb-4 flex flex-wrap items-center gap-2 rounded-lg border bg-muted/30 px-3 py-2">
          <span className="font-medium text-sm">{t('capabilities.selected', { count: selectedIds.length })}</span>
          <span className="text-muted-foreground text-xs">·</span>
          <DropdownMenu>
            <DropdownMenuTrigger render={<Button disabled={isPending} size="sm" variant="outline" />}>
              {t('capabilities.batch_actions')}
              <ChevronDown className="ml-1 size-3.5" />
            </DropdownMenuTrigger>
            <DropdownMenuContent align="start" className="w-56">
              {ORDERED_CAPABILITIES.map(({ bit, key, labelKey, risk }) => (
                <DropdownMenuSub key={key}>
                  <DropdownMenuSubTrigger>
                    <div className="flex flex-col">
                      <span>{t(labelKey, { ns: 'servers' })}</span>
                      <span className={`text-[10px] ${risk === 'high' ? 'text-red-500' : 'text-muted-foreground'}`}>
                        {t(risk === 'high' ? 'cap_high_risk' : 'cap_low_risk', { ns: 'servers' })}
                      </span>
                    </div>
                  </DropdownMenuSubTrigger>
                  <DropdownMenuSubContent>
                    <DropdownMenuItem disabled={isPending} onClick={() => batchEnable(bit)}>
                      {t('capabilities.enable')}
                    </DropdownMenuItem>
                    <DropdownMenuItem disabled={isPending} onClick={() => batchDisable(bit)}>
                      {t('capabilities.disable')}
                    </DropdownMenuItem>
                  </DropdownMenuSubContent>
                </DropdownMenuSub>
              ))}
            </DropdownMenuContent>
          </DropdownMenu>
          <Button disabled={isPending} onClick={batchReset} size="sm" variant="outline">
            <RotateCcw className="mr-1 size-3.5" />
            {t('capabilities.reset_default')}
          </Button>
          <Button
            className="ml-auto"
            disabled={isPending}
            onClick={() => setRowSelection({})}
            size="sm"
            variant="ghost"
          >
            <X className="mr-1 size-3.5" />
            {t('capabilities.clear_selection')}
          </Button>
        </div>
      )}

      {renderTableContent()}

      {filtered.length > 0 && search.length > 0 && (
        <p className="mt-3 text-muted-foreground text-xs">
          {t('capabilities.footer_showing', { filtered: filtered.length, total: servers.length })}
        </p>
      )}
    </div>
  )
}

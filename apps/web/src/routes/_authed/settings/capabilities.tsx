import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { createFileRoute } from '@tanstack/react-router'
import { type ColumnDef, getCoreRowModel, type RowSelectionState, useReactTable } from '@tanstack/react-table'
import { RotateCcw, Search, ShieldAlert } from 'lucide-react'
import { useCallback, useMemo, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { toast } from 'sonner'
import { Button } from '@/components/ui/button'
import { createSelectColumn, DataTable } from '@/components/ui/data-table'
import { Input } from '@/components/ui/input'
import { Skeleton } from '@/components/ui/skeleton'
import { Switch } from '@/components/ui/switch'
import { api } from '@/lib/api-client'
import { CAP_DEFAULT, CAPABILITIES, getEffectiveCapabilityEnabled, isClientCapabilityLocked } from '@/lib/capabilities'

export const Route = createFileRoute('/_authed/settings/capabilities')({
  validateSearch: (search: Record<string, unknown>) => ({
    q: (search.q as string) || ''
  }),
  component: CapabilitiesPage
})

interface ServerInfo {
  agent_local_capabilities?: number | null
  capabilities?: number | null
  effective_capabilities?: number | null
  id: string
  name: string
  protocol_version?: number | null
}

const ORDERED_CAPABILITIES = [
  ...CAPABILITIES.filter(({ risk }) => risk === 'high'),
  ...CAPABILITIES.filter(({ risk }) => risk === 'low')
]

function CapabilitiesPage() {
  const { t } = useTranslation(['settings', 'servers'])
  const queryClient = useQueryClient()
  const { q: search } = Route.useSearch()
  const navigate = Route.useNavigate()
  const [rowSelection, setRowSelection] = useState<RowSelectionState>({})

  const { data: servers = [], isLoading } = useQuery<ServerInfo[]>({
    queryKey: ['servers-list'],
    queryFn: () => api.get<ServerInfo[]>('/api/servers')
  })

  const { mutate: mutateSingleCap, isPending: isSinglePending } = useMutation({
    mutationFn: ({ id, capabilities }: { capabilities: number; id: string }) =>
      api.put(`/api/servers/${id}`, { capabilities }),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['servers-list'] })
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
      queryClient.invalidateQueries({ queryKey: ['servers-list'] })
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

  const columns = useMemo<ColumnDef<ServerInfo>[]>(
    () => [
      createSelectColumn<ServerInfo>(),
      {
        accessorKey: 'name',
        header: () => t('capabilities.server'),
        cell: ({ row }) => {
          const hasOldAgent = row.original.protocol_version != null && row.original.protocol_version < 2
          return (
            <div className="flex items-center gap-2">
              <span className="font-medium">{row.original.name}</span>
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
              return (
                <div className="text-center">
                  <Switch
                    aria-label={`${t(labelKey, { ns: 'servers' })} - ${row.original.name}`}
                    checked={isEnabled}
                    disabled={isPending || isLocked}
                    onCheckedChange={() => toggleCap(row.original, bit)}
                    title={isLocked ? '客户端关闭' : undefined}
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
    <div>
      <div className="mb-6">
        <h1 className="font-bold text-2xl">{t('capabilities.title')}</h1>
        <p className="text-muted-foreground text-sm">{t('capabilities.description')}</p>
      </div>

      <div className="mb-4 flex items-center gap-3">
        <div className="relative max-w-sm flex-1">
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
        {selectedIds.length > 0 && (
          <div className="flex items-center gap-2">
            <span className="text-muted-foreground text-sm">
              {t('capabilities.selected', { count: selectedIds.length })}
            </span>
            <Button disabled={isPending} onClick={batchReset} size="sm" variant="outline">
              <RotateCcw className="mr-1 size-3.5" />
              {t('capabilities.reset_default')}
            </Button>
          </div>
        )}
      </div>

      {selectedIds.length > 0 && (
        <div className="mb-4 flex flex-wrap gap-2 rounded-lg border bg-muted/30 p-3">
          <span className="self-center text-muted-foreground text-sm">{t('capabilities.batch_toggle')}</span>
          {ORDERED_CAPABILITIES.map(({ bit, labelKey }) => (
            <div className="flex gap-1" key={bit}>
              <Button disabled={isPending} onClick={() => batchEnable(bit)} size="sm" variant="outline">
                {t('capabilities.batch_enable', { capability: t(labelKey, { ns: 'servers' }) })}
              </Button>
              <Button disabled={isPending} onClick={() => batchDisable(bit)} size="sm" variant="outline">
                {t('capabilities.batch_disable', { capability: t(labelKey, { ns: 'servers' }) })}
              </Button>
            </div>
          ))}
        </div>
      )}

      {renderTableContent()}

      {filtered.length > 0 && (
        <p className="mt-3 text-muted-foreground text-xs">
          {t('capabilities.footer_showing', { filtered: filtered.length, total: servers.length })}
          {selectedIds.length > 0 && ` · ${t('capabilities.footer_selected', { count: selectedIds.length })}`}
        </p>
      )}
    </div>
  )
}

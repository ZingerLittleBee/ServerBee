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
import { CAP_DEFAULT, CAPABILITIES } from '@/lib/capabilities'

export const Route = createFileRoute('/_authed/settings/capabilities')({
  component: CapabilitiesPage
})

interface ServerInfo {
  capabilities?: number | null
  id: string
  name: string
  protocol_version?: number | null
}

function CapabilitiesPage() {
  const { t } = useTranslation(['settings', 'servers'])
  const queryClient = useQueryClient()
  const [search, setSearch] = useState('')
  const [rowSelection, setRowSelection] = useState<RowSelectionState>({})

  const capLabelMap = useMemo<Record<string, string>>(
    () => ({
      terminal: t('cap_terminal', { ns: 'servers' }),
      exec: t('cap_exec', { ns: 'servers' }),
      upgrade: t('cap_upgrade', { ns: 'servers' }),
      ping_icmp: t('cap_ping_icmp', { ns: 'servers' }),
      ping_tcp: t('cap_ping_tcp', { ns: 'servers' }),
      ping_http: t('cap_ping_http', { ns: 'servers' })
    }),
    [t]
  )

  const { data: servers = [], isLoading } = useQuery<ServerInfo[]>({
    queryKey: ['servers-list'],
    queryFn: () => api.get<ServerInfo[]>('/api/servers')
  })

  const singleMutation = useMutation({
    mutationFn: ({ id, capabilities }: { capabilities: number; id: string }) =>
      api.put(`/api/servers/${id}`, { capabilities }),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['servers-list'] })
      toast.success('Capability updated')
    },
    onError: (err) => {
      toast.error(err instanceof Error ? err.message : 'Failed to update capability')
    }
  })

  const batchMutation = useMutation({
    mutationFn: ({ ids, capabilities }: { capabilities: number; ids: string[] }) =>
      api.put('/api/servers/batch-capabilities', { ids, capabilities }),
    onSuccess: () => {
      setRowSelection({})
      queryClient.invalidateQueries({ queryKey: ['servers-list'] })
      toast.success('Batch capabilities updated')
    },
    onError: (err) => {
      toast.error(err instanceof Error ? err.message : 'Failed to update batch capabilities')
    }
  })

  const filtered = servers.filter((s) => s.name.toLowerCase().includes(search.toLowerCase()))

  const toggleCap = useCallback(
    (server: ServerInfo, bit: number) => {
      const caps = server.capabilities ?? CAP_DEFAULT
      // biome-ignore lint/suspicious/noBitwiseOperators: intentional capability bitmask toggle
      const newCaps = caps & bit ? caps & ~bit : caps | bit
      singleMutation.mutate({ id: server.id, capabilities: newCaps })
    },
    [singleMutation]
  )

  const isPending = singleMutation.isPending || batchMutation.isPending

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
                  <ShieldAlert className="size-3.5 text-amber-500" />
                </span>
              )}
            </div>
          )
        },
        enableSorting: false
      },
      ...CAPABILITIES.map(
        ({ bit, key, risk }) =>
          ({
            id: `cap_${key}`,
            header: () => (
              <div className="text-center">
                <div>{capLabelMap[key]}</div>
                <div className={`text-[10px] ${risk === 'high' ? 'text-red-500' : 'text-muted-foreground'}`}>
                  {t(risk === 'high' ? 'cap_high_risk' : 'cap_low_risk', { ns: 'servers' })}
                </div>
              </div>
            ),
            cell: ({ row }) => {
              const caps = row.original.capabilities ?? CAP_DEFAULT
              // biome-ignore lint/suspicious/noBitwiseOperators: bitmask check
              const isEnabled = (caps & bit) !== 0
              return (
                <div className="text-center">
                  <Switch
                    checked={isEnabled}
                    disabled={isPending}
                    onCheckedChange={() => toggleCap(row.original, bit)}
                  />
                </div>
              )
            },
            enableSorting: false,
            meta: { className: 'text-center' }
          }) satisfies ColumnDef<ServerInfo>
      )
    ],
    [capLabelMap, isPending, toggleCap, t]
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
    batchMutation.mutate({ ids: selectedIds, capabilities: baseCaps | bit })
  }

  const batchDisable = (bit: number) => {
    const firstServer = servers.find((s) => s.id === selectedIds[0])
    const baseCaps = firstServer?.capabilities ?? CAP_DEFAULT
    // biome-ignore lint/suspicious/noBitwiseOperators: intentional capability bitmask disable
    batchMutation.mutate({ ids: selectedIds, capabilities: baseCaps & ~bit })
  }

  const batchReset = () => {
    batchMutation.mutate({ ids: selectedIds, capabilities: CAP_DEFAULT })
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
          <Search className="absolute top-1/2 left-3 size-4 -translate-y-1/2 text-muted-foreground" />
          <Input
            className="pl-9"
            onChange={(e) => setSearch(e.target.value)}
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
          {CAPABILITIES.map(({ bit, key }) => (
            <div className="flex gap-1" key={bit}>
              <Button disabled={isPending} onClick={() => batchEnable(bit)} size="sm" variant="outline">
                +{capLabelMap[key]}
              </Button>
              <Button disabled={isPending} onClick={() => batchDisable(bit)} size="sm" variant="outline">
                -{capLabelMap[key]}
              </Button>
            </div>
          ))}
        </div>
      )}

      {renderTableContent()}

      {filtered.length > 0 && (
        <p className="mt-3 text-muted-foreground text-xs">
          Showing {filtered.length} of {servers.length} servers
          {selectedIds.length > 0 && ` · ${selectedIds.length} selected`}
        </p>
      )}
    </div>
  )
}

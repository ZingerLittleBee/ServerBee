import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { createFileRoute } from '@tanstack/react-router'
import { RotateCcw, Search, ShieldAlert } from 'lucide-react'
import { useState } from 'react'
import { useTranslation } from 'react-i18next'
import { toast } from 'sonner'
import { Button } from '@/components/ui/button'
import { Checkbox } from '@/components/ui/checkbox'
import { Input } from '@/components/ui/input'
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
  const [selected, setSelected] = useState<Set<string>>(new Set())

  const capLabelMap: Record<string, string> = {
    terminal: t('cap_terminal', { ns: 'servers' }),
    exec: t('cap_exec', { ns: 'servers' }),
    upgrade: t('cap_upgrade', { ns: 'servers' }),
    ping_icmp: t('cap_ping_icmp', { ns: 'servers' }),
    ping_tcp: t('cap_ping_tcp', { ns: 'servers' }),
    ping_http: t('cap_ping_http', { ns: 'servers' })
  }

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
      setSelected(new Set())
      queryClient.invalidateQueries({ queryKey: ['servers-list'] })
      toast.success('Batch capabilities updated')
    },
    onError: (err) => {
      toast.error(err instanceof Error ? err.message : 'Failed to update batch capabilities')
    }
  })

  const filtered = servers.filter((s) => s.name.toLowerCase().includes(search.toLowerCase()))
  const allSelected = filtered.length > 0 && selected.size === filtered.length

  const toggleAll = () => {
    if (allSelected) {
      setSelected(new Set())
    } else {
      setSelected(new Set(filtered.map((s) => s.id)))
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

  const toggleCap = (server: ServerInfo, bit: number) => {
    const caps = server.capabilities ?? CAP_DEFAULT
    // biome-ignore lint/suspicious/noBitwiseOperators: intentional capability bitmask toggle
    const newCaps = caps & bit ? caps & ~bit : caps | bit
    singleMutation.mutate({ id: server.id, capabilities: newCaps })
  }

  const batchEnable = (bit: number) => {
    const ids = [...selected]
    const firstServer = servers.find((s) => s.id === ids[0])
    const baseCaps = firstServer?.capabilities ?? CAP_DEFAULT
    // biome-ignore lint/suspicious/noBitwiseOperators: intentional capability bitmask enable
    batchMutation.mutate({ ids, capabilities: baseCaps | bit })
  }

  const batchDisable = (bit: number) => {
    const ids = [...selected]
    const firstServer = servers.find((s) => s.id === ids[0])
    const baseCaps = firstServer?.capabilities ?? CAP_DEFAULT
    // biome-ignore lint/suspicious/noBitwiseOperators: intentional capability bitmask disable
    batchMutation.mutate({ ids, capabilities: baseCaps & ~bit })
  }

  const batchReset = () => {
    batchMutation.mutate({ ids: [...selected], capabilities: CAP_DEFAULT })
  }

  const isPending = singleMutation.isPending || batchMutation.isPending

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
        {selected.size > 0 && (
          <div className="flex items-center gap-2">
            <span className="text-muted-foreground text-sm">
              {t('capabilities.selected', { count: selected.size })}
            </span>
            <Button disabled={isPending} onClick={batchReset} size="sm" variant="outline">
              <RotateCcw className="mr-1 size-3.5" />
              {t('capabilities.reset_default')}
            </Button>
          </div>
        )}
      </div>

      {selected.size > 0 && (
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

      {isLoading && (
        <div className="flex min-h-[200px] items-center justify-center">
          <div className="size-8 animate-spin rounded-full border-4 border-muted border-t-primary" />
        </div>
      )}

      {!isLoading && servers.length === 0 && (
        <div className="flex min-h-[200px] items-center justify-center rounded-lg border border-dashed">
          <p className="text-muted-foreground text-sm">{t('capabilities.no_servers')}</p>
        </div>
      )}

      {!isLoading && servers.length > 0 && (
        <div className="overflow-hidden rounded-lg border">
          <table className="w-full text-sm">
            <thead>
              <tr className="border-b bg-muted/50">
                <th className="w-10 px-3 py-2.5">
                  <Checkbox checked={allSelected} onCheckedChange={toggleAll} />
                </th>
                <th className="px-3 py-2.5 text-left font-medium text-muted-foreground">{t('capabilities.server')}</th>
                {CAPABILITIES.map(({ bit, key, risk }) => (
                  <th className="px-3 py-2.5 text-center font-medium text-muted-foreground text-xs" key={bit}>
                    <div>{capLabelMap[key]}</div>
                    <div className={`text-[10px] ${risk === 'high' ? 'text-red-500' : 'text-muted-foreground'}`}>
                      {t(risk === 'high' ? 'cap_high_risk' : 'cap_low_risk', { ns: 'servers' })}
                    </div>
                  </th>
                ))}
              </tr>
            </thead>
            <tbody>
              {filtered.map((server) => {
                const caps = server.capabilities ?? CAP_DEFAULT
                const hasOldAgent = server.protocol_version != null && server.protocol_version < 2
                return (
                  <tr className="border-b transition-colors last:border-b-0 hover:bg-muted/30" key={server.id}>
                    <td className="px-3 py-2">
                      <Checkbox checked={selected.has(server.id)} onCheckedChange={() => toggleOne(server.id)} />
                    </td>
                    <td className="px-3 py-2">
                      <div className="flex items-center gap-2">
                        <span className="font-medium">{server.name}</span>
                        {hasOldAgent && (
                          <span title={t('cap_upgrade_warning', { ns: 'servers' })}>
                            <ShieldAlert className="size-3.5 text-amber-500" />
                          </span>
                        )}
                      </div>
                    </td>
                    {CAPABILITIES.map(({ bit }) => {
                      // biome-ignore lint/suspicious/noBitwiseOperators: intentional capability bitmask check
                      const isEnabled = (caps & bit) !== 0
                      return (
                        <td className="px-3 py-2 text-center" key={bit}>
                          <Switch
                            checked={isEnabled}
                            disabled={isPending}
                            onCheckedChange={() => toggleCap(server, bit)}
                          />
                        </td>
                      )
                    })}
                  </tr>
                )
              })}
            </tbody>
          </table>
        </div>
      )}

      {filtered.length > 0 && (
        <p className="mt-3 text-muted-foreground text-xs">
          Showing {filtered.length} of {servers.length} servers
          {selected.size > 0 && ` · ${selected.size} selected`}
        </p>
      )}
    </div>
  )
}

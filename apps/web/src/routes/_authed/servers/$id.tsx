import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { createFileRoute, Link } from '@tanstack/react-router'
import { ArrowLeft, CreditCard, Pencil, Terminal as TerminalIcon } from 'lucide-react'
import { useMemo, useState } from 'react'
import { MetricsChart } from '@/components/server/metrics-chart'
import { ServerEditDialog } from '@/components/server/server-edit-dialog'
import { StatusBadge } from '@/components/server/status-badge'
import { Button } from '@/components/ui/button'
import { useServer, useServerRecords } from '@/hooks/use-api'
import { useAuth } from '@/hooks/use-auth'
import type { ServerMetrics } from '@/hooks/use-servers-ws'
import { api } from '@/lib/api-client'
import type { ServerResponse } from '@/lib/api-schema'
import { CAPABILITIES } from '@/lib/capabilities'
import { cn, countryCodeToFlag, formatBytes } from '@/lib/utils'

export const Route = createFileRoute('/_authed/servers/$id')({
  component: ServerDetailPage
})

interface TimeRange {
  hours: number
  interval: string
  label: string
}

interface GpuRecordAggregated {
  gpu_usage_avg: number
  mem_total_avg: number
  mem_used_avg: number
  temperature_avg: number
  time: string
}

interface ServerWithCaps {
  capabilities?: number | null
  id: string
  protocol_version?: number | null
}

const TIME_RANGES: TimeRange[] = [
  { label: '1h', hours: 1, interval: 'raw' },
  { label: '6h', hours: 6, interval: 'raw' },
  { label: '24h', hours: 24, interval: 'raw' },
  { label: '7d', hours: 168, interval: 'hourly' },
  { label: '30d', hours: 720, interval: 'hourly' }
]

function formatCurrency(price: number, currency: string): string {
  try {
    return new Intl.NumberFormat(undefined, { style: 'currency', currency }).format(price)
  } catch {
    return `${currency} ${price.toFixed(2)}`
  }
}

function ServerInfoMeta({ server }: { server: ServerResponse }) {
  return (
    <div className="mt-2 flex flex-wrap gap-x-4 gap-y-1 text-muted-foreground text-sm">
      {server.os && <span>OS: {server.os}</span>}
      {server.cpu_name && (
        <span>
          CPU: {server.cpu_name}
          {server.cpu_cores && ` (${server.cpu_cores} cores)`}
          {server.cpu_arch && ` ${server.cpu_arch}`}
        </span>
      )}
      {server.mem_total != null && <span>RAM: {formatBytes(server.mem_total)}</span>}
      {server.ipv4 && <span>IPv4: {server.ipv4}</span>}
      {server.ipv6 && <span>IPv6: {server.ipv6}</span>}
      {server.kernel_version && <span>Kernel: {server.kernel_version}</span>}
      {server.region && <span>Region: {server.region}</span>}
      {server.agent_version && <span>Agent: v{server.agent_version}</span>}
    </div>
  )
}

function CapabilitiesSection({ server }: { server: ServerWithCaps }) {
  const { user } = useAuth()
  const queryClient = useQueryClient()

  const mutation = useMutation({
    mutationFn: (newCaps: number) => api.put(`/api/servers/${server.id}`, { capabilities: newCaps }),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['servers', server.id] })
    }
  })

  if (user?.role !== 'admin') {
    return null
  }

  const caps = server.capabilities ?? 56

  const toggle = (bit: number) => {
    // biome-ignore lint/suspicious/noBitwiseOperators: intentional capability bitmask toggle
    const newCaps = caps & bit ? caps & ~bit : caps | bit
    mutation.mutate(newCaps)
  }

  return (
    <div className="mt-6 rounded-lg border bg-card p-6">
      <div className="mb-4 flex items-center justify-between">
        <h3 className="font-semibold">Capability Toggles</h3>
        {server.protocol_version != null && server.protocol_version < 2 && (
          <span className="rounded bg-amber-100 px-2 py-1 text-amber-600 text-xs dark:bg-amber-900/30 dark:text-amber-400">
            Agent does not support capability enforcement — upgrade recommended
          </span>
        )}
      </div>
      <div className="space-y-3">
        {CAPABILITIES.map(({ bit, label, risk }) => (
          <div className="flex items-center justify-between" key={bit}>
            <div className="flex items-center gap-2">
              <span>{label}</span>
              <span
                className={`rounded px-1.5 py-0.5 text-xs ${
                  risk === 'high'
                    ? 'bg-red-100 text-red-700 dark:bg-red-900/30 dark:text-red-400'
                    : 'bg-green-100 text-green-700 dark:bg-green-900/30 dark:text-green-400'
                }`}
              >
                {risk === 'high' ? 'High Risk' : 'Low Risk'}
              </span>
            </div>
            <button
              aria-checked={
                // biome-ignore lint/suspicious/noBitwiseOperators: intentional capability bitmask check
                !!(caps & bit)
              }
              className={`relative inline-flex h-6 w-11 items-center rounded-full transition-colors ${
                // biome-ignore lint/suspicious/noBitwiseOperators: intentional capability bitmask check
                caps & bit ? 'bg-primary' : 'bg-muted'
              }`}
              disabled={mutation.isPending}
              onClick={() => toggle(bit)}
              role="switch"
              type="button"
            >
              <span
                className={`inline-block size-4 rounded-full bg-white transition-transform ${
                  // biome-ignore lint/suspicious/noBitwiseOperators: intentional capability bitmask check
                  caps & bit ? 'translate-x-6' : 'translate-x-1'
                }`}
              />
            </button>
          </div>
        ))}
      </div>
    </div>
  )
}

function ServerDetailPage() {
  const { id } = Route.useParams()
  const [selectedRange, setSelectedRange] = useState(1)
  const [editOpen, setEditOpen] = useState(false)

  const range = TIME_RANGES[selectedRange]
  // biome-ignore lint/correctness/useExhaustiveDependencies: intentionally re-compute when selectedRange changes
  const now = useMemo(() => new Date(), [selectedRange])
  const from = new Date(now.getTime() - range.hours * 3600 * 1000).toISOString()
  const to = now.toISOString()

  const { data: server, isLoading: serverLoading } = useServer(id)
  const { data: records } = useServerRecords(id, from, to, range.interval)

  const { data: gpuRecords } = useQuery<GpuRecordAggregated[]>({
    queryKey: ['servers', id, 'gpu-records', from, to],
    queryFn: () =>
      api.get<GpuRecordAggregated[]>(
        `/api/servers/${id}/gpu-records?from=${encodeURIComponent(from)}&to=${encodeURIComponent(to)}`
      ),
    enabled: id.length > 0
  })

  const { data: liveServers } = useQuery<ServerMetrics[]>({
    queryKey: ['servers'],
    queryFn: () => [],
    staleTime: Number.POSITIVE_INFINITY,
    refetchOnMount: false,
    refetchOnWindowFocus: false
  })
  const liveData = liveServers?.find((s) => s.id === id)

  const chartData = useMemo(() => {
    if (!records) {
      return []
    }
    return records.map((r) => ({
      timestamp: r.time,
      cpu: r.cpu,
      memory_pct: server?.mem_total ? (r.mem_used / server.mem_total) * 100 : 0,
      disk_pct: server?.disk_total ? (r.disk_used / server.disk_total) * 100 : 0,
      net_in_speed: r.net_in_speed,
      net_out_speed: r.net_out_speed,
      net_in_transfer: r.net_in_transfer,
      net_out_transfer: r.net_out_transfer,
      load1: r.load1,
      load5: r.load5,
      load15: r.load15,
      temperature: r.temperature
    }))
  }, [records, server])

  const gpuChartData = useMemo(() => {
    if (!gpuRecords || gpuRecords.length === 0) {
      return []
    }
    return gpuRecords.map((r) => ({
      timestamp: r.time,
      gpu_usage: r.gpu_usage_avg,
      gpu_temp: r.temperature_avg,
      gpu_mem_pct: r.mem_total_avg > 0 ? (r.mem_used_avg / r.mem_total_avg) * 100 : 0
    }))
  }, [gpuRecords])

  const hasTemperature = chartData.some((d) => d.temperature != null && d.temperature > 0)
  const hasGpu = gpuChartData.length > 0

  if (serverLoading) {
    return (
      <div className="flex min-h-[400px] items-center justify-center">
        <div className="mx-auto size-8 animate-spin rounded-full border-4 border-muted border-t-primary" />
      </div>
    )
  }

  if (!server) {
    return (
      <div className="flex min-h-[400px] items-center justify-center">
        <p className="text-muted-foreground">Server not found</p>
      </div>
    )
  }

  const serverWithCaps = server as ServerResponse & ServerWithCaps
  const isOnline = liveData?.online ?? false
  const hasBilling = server.price != null || server.expired_at != null || server.traffic_limit != null
  const flag = countryCodeToFlag(server.country_code)
  // biome-ignore lint/suspicious/noBitwiseOperators: intentional capability bitmask check
  const terminalEnabled = serverWithCaps.capabilities == null || (serverWithCaps.capabilities & 1) !== 0

  // Network cumulative traffic from live data
  const liveNetIn = liveData?.net_in_transfer ?? 0
  const liveNetOut = liveData?.net_out_transfer ?? 0

  return (
    <div>
      <div className="mb-6">
        <Link
          className="mb-3 inline-flex items-center gap-1 text-muted-foreground text-sm hover:text-foreground"
          to="/"
        >
          <ArrowLeft className="size-4" />
          Back to Dashboard
        </Link>

        <div className="flex items-start justify-between">
          <div>
            <div className="flex items-center gap-3">
              {flag && <span className="text-xl">{flag}</span>}
              <h1 className="font-bold text-2xl">{server.name}</h1>
              <StatusBadge online={isOnline} />
            </div>
            <ServerInfoMeta server={server} />
          </div>
          <div className="flex gap-2">
            <Button onClick={() => setEditOpen(true)} size="sm" variant="outline">
              <Pencil className="mr-1 size-4" />
              Edit
            </Button>
            {isOnline && terminalEnabled && (
              <Link params={{ serverId: id }} to="/terminal/$serverId">
                <Button size="sm" variant="outline">
                  <TerminalIcon className="mr-1 size-4" />
                  Terminal
                </Button>
              </Link>
            )}
          </div>
        </div>
      </div>

      {hasBilling && <BillingInfoBar server={server} />}

      {isOnline && (liveNetIn > 0 || liveNetOut > 0) && (
        <div className="mb-6 flex flex-wrap gap-6 rounded-lg border bg-card p-3 text-sm">
          <span className="text-muted-foreground">
            Network In: <span className="font-medium text-foreground">{formatBytes(liveNetIn)}</span>
          </span>
          <span className="text-muted-foreground">
            Network Out: <span className="font-medium text-foreground">{formatBytes(liveNetOut)}</span>
          </span>
          <span className="text-muted-foreground">
            Total: <span className="font-medium text-foreground">{formatBytes(liveNetIn + liveNetOut)}</span>
          </span>
        </div>
      )}

      <div className="mb-4 flex gap-1">
        {TIME_RANGES.map((tr, i) => (
          <Button
            className={cn(selectedRange === i && 'bg-primary text-primary-foreground')}
            key={tr.label}
            onClick={() => setSelectedRange(i)}
            size="sm"
            variant={selectedRange === i ? 'default' : 'outline'}
          >
            {tr.label}
          </Button>
        ))}
      </div>

      <div className="grid gap-4 lg:grid-cols-2">
        <MetricsChart color="var(--color-chart-1)" data={chartData} dataKey="cpu" title="CPU Usage" unit="%" />
        <MetricsChart
          color="var(--color-chart-2)"
          data={chartData}
          dataKey="memory_pct"
          title="Memory Usage"
          unit="%"
        />
        <MetricsChart color="var(--color-chart-3)" data={chartData} dataKey="disk_pct" title="Disk Usage" unit="%" />
        <MetricsChart
          color="var(--color-chart-4)"
          data={chartData}
          dataKey="net_in_speed"
          formatValue={(v) => formatBytes(v)}
          title="Network In"
        />
        <MetricsChart
          color="var(--color-chart-5)"
          data={chartData}
          dataKey="net_out_speed"
          formatValue={(v) => formatBytes(v)}
          title="Network Out"
        />
        <MetricsChart color="var(--color-chart-1)" data={chartData} dataKey="load1" title="Load Average (1m)" />

        {hasTemperature && (
          <MetricsChart
            color="var(--color-chart-4)"
            data={chartData}
            dataKey="temperature"
            title="Temperature"
            unit="°C"
          />
        )}

        {hasGpu && (
          <>
            <MetricsChart
              color="var(--color-chart-5)"
              data={gpuChartData}
              dataKey="gpu_usage"
              title="GPU Usage"
              unit="%"
            />
            <MetricsChart
              color="var(--color-chart-2)"
              data={gpuChartData}
              dataKey="gpu_temp"
              title="GPU Temperature"
              unit="°C"
            />
          </>
        )}
      </div>

      <CapabilitiesSection server={serverWithCaps} />

      <ServerEditDialog onClose={() => setEditOpen(false)} open={editOpen} server={server} />
    </div>
  )
}

function BillingInfoBar({
  server
}: {
  server: Pick<
    ServerResponse,
    'billing_cycle' | 'currency' | 'expired_at' | 'price' | 'traffic_limit' | 'traffic_limit_type'
  >
}) {
  const isExpired = server.expired_at ? new Date(server.expired_at) < new Date() : false
  const daysUntilExpiry = server.expired_at
    ? Math.ceil((new Date(server.expired_at).getTime() - Date.now()) / 86_400_000)
    : null

  const expiryColor = (() => {
    if (isExpired) {
      return 'text-destructive'
    }
    if (daysUntilExpiry != null && daysUntilExpiry <= 7) {
      return 'text-yellow-600 dark:text-yellow-400'
    }
    return 'text-muted-foreground'
  })()

  return (
    <div className="mb-6 flex flex-wrap items-center gap-4 rounded-lg border bg-card p-3 text-sm">
      <CreditCard className="size-4 text-muted-foreground" />
      {server.price != null && (
        <span>
          {formatCurrency(server.price, server.currency ?? 'USD')}
          {server.billing_cycle && <span className="text-muted-foreground"> / {server.billing_cycle}</span>}
        </span>
      )}
      {server.expired_at && (
        <span className={cn(expiryColor)}>
          {isExpired
            ? `Expired ${new Date(server.expired_at).toLocaleDateString()}`
            : `Expires ${new Date(server.expired_at).toLocaleDateString()}`}
          {daysUntilExpiry != null && !isExpired && ` (${daysUntilExpiry}d)`}
        </span>
      )}
      {server.traffic_limit != null && (
        <span className="text-muted-foreground">
          Traffic: {formatBytes(server.traffic_limit)}
          {server.traffic_limit_type && server.traffic_limit_type !== 'sum' && ` (${server.traffic_limit_type})`}
        </span>
      )}
    </div>
  )
}

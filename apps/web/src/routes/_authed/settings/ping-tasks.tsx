import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { createFileRoute } from '@tanstack/react-router'
import { Activity, BarChart3, Plus, Trash2 } from 'lucide-react'
import { type FormEvent, useMemo, useState } from 'react'
import { Area, AreaChart, CartesianGrid, ResponsiveContainer, Tooltip, XAxis, YAxis } from 'recharts'
import { Button } from '@/components/ui/button'
import { api } from '@/lib/api-client'

export const Route = createFileRoute('/_authed/settings/ping-tasks')({
  component: PingTasksPage
})

interface PingTask {
  created_at: string
  enabled: boolean
  id: string
  interval: number
  name: string
  probe_type: string
  server_ids_json: string
  target: string
}

interface PingRecord {
  id: number
  latency: number
  server_id: string
  success: boolean
  task_id: string
  time: string
}

interface Server {
  id: string
  name: string
}

type ProbeType = 'http' | 'icmp' | 'tcp'

const probeTypeLabels: Record<ProbeType, string> = {
  icmp: 'ICMP Ping',
  tcp: 'TCP Connect',
  http: 'HTTP Request'
}

function PingResultsChart({ taskId }: { taskId: string }) {
  const now = useMemo(() => new Date(), [taskId])
  const from = new Date(now.getTime() - 24 * 3600 * 1000).toISOString()
  const to = now.toISOString()

  const { data: records, isLoading } = useQuery<PingRecord[]>({
    queryKey: ['ping-records', taskId, from, to],
    queryFn: () =>
      api.get<PingRecord[]>(
        `/api/ping-tasks/${taskId}/records?from=${encodeURIComponent(from)}&to=${encodeURIComponent(to)}`
      )
  })

  if (isLoading) {
    return <div className="h-48 animate-pulse rounded bg-muted" />
  }

  if (!records || records.length === 0) {
    return <p className="py-4 text-center text-muted-foreground text-xs">No records in the last 24 hours</p>
  }

  const chartData = records.map((r) => ({
    timestamp: r.time,
    latency: r.success ? r.latency : null
  }))

  const successRate = ((records.filter((r) => r.success).length / records.length) * 100).toFixed(1)
  const avgLatency =
    records.filter((r) => r.success).reduce((sum, r) => sum + r.latency, 0) /
    Math.max(1, records.filter((r) => r.success).length)

  return (
    <div className="space-y-2">
      <div className="flex gap-4 text-muted-foreground text-xs">
        <span>
          Success: <span className="font-medium text-foreground">{successRate}%</span>
        </span>
        <span>
          Avg Latency: <span className="font-medium text-foreground">{avgLatency.toFixed(1)}ms</span>
        </span>
        <span>{records.length} records (24h)</span>
      </div>
      <ResponsiveContainer height={180} width="100%">
        <AreaChart data={chartData}>
          <defs>
            <linearGradient id="gradient-latency" x1="0" x2="0" y1="0" y2="1">
              <stop offset="5%" stopColor="var(--color-chart-4)" stopOpacity={0.3} />
              <stop offset="95%" stopColor="var(--color-chart-4)" stopOpacity={0} />
            </linearGradient>
          </defs>
          <CartesianGrid stroke="var(--color-border)" strokeDasharray="3 3" vertical={false} />
          <XAxis
            axisLine={false}
            dataKey="timestamp"
            stroke="var(--color-muted-foreground)"
            tick={{ fontSize: 10 }}
            tickFormatter={(t: string) => new Date(t).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })}
            tickLine={false}
          />
          <YAxis
            axisLine={false}
            stroke="var(--color-muted-foreground)"
            tick={{ fontSize: 10 }}
            tickLine={false}
            width={40}
          />
          <Tooltip
            contentStyle={{
              backgroundColor: 'var(--color-card)',
              border: '1px solid var(--color-border)',
              borderRadius: '8px',
              fontSize: '12px'
            }}
            formatter={(value) => [`${Number(value).toFixed(1)}ms`, 'Latency']}
            labelFormatter={(label) => new Date(String(label)).toLocaleString()}
          />
          <Area
            connectNulls={false}
            dataKey="latency"
            fill="url(#gradient-latency)"
            stroke="var(--color-chart-4)"
            strokeWidth={2}
            type="monotone"
          />
        </AreaChart>
      </ResponsiveContainer>
    </div>
  )
}

function PingTasksPage() {
  const queryClient = useQueryClient()
  const [showForm, setShowForm] = useState(false)
  const [expandedTaskId, setExpandedTaskId] = useState<string | null>(null)
  const [name, setName] = useState('')
  const [probeType, setProbeType] = useState<ProbeType>('icmp')
  const [target, setTarget] = useState('')
  const [interval, setInterval] = useState(60)
  const [selectedServerIds, setSelectedServerIds] = useState<string[]>([])

  const { data: tasks, isLoading } = useQuery<PingTask[]>({
    queryKey: ['ping-tasks'],
    queryFn: () => api.get<PingTask[]>('/api/ping-tasks')
  })

  const { data: servers } = useQuery<Server[]>({
    queryKey: ['servers-list'],
    queryFn: () => api.get<Server[]>('/api/servers')
  })

  const createMutation = useMutation({
    mutationFn: (input: {
      enabled: boolean
      interval: number
      name: string
      probe_type: string
      server_ids: string[]
      target: string
    }) => api.post<PingTask>('/api/ping-tasks', input),
    onSuccess: () => {
      invalidate()
      resetForm()
    }
  })

  const deleteMutation = useMutation({
    mutationFn: (id: string) => api.delete(`/api/ping-tasks/${id}`),
    onSuccess: () => invalidate()
  })

  const toggleMutation = useMutation({
    mutationFn: ({ enabled, id }: { enabled: boolean; id: string }) =>
      api.put<PingTask>(`/api/ping-tasks/${id}`, { enabled }),
    onSuccess: () => invalidate()
  })

  const invalidate = () => {
    queryClient.invalidateQueries({ queryKey: ['ping-tasks'] }).catch(() => undefined)
  }

  const resetForm = () => {
    setName('')
    setProbeType('icmp')
    setTarget('')
    setInterval(60)
    setSelectedServerIds([])
    setShowForm(false)
  }

  const handleCreate = (e: FormEvent) => {
    e.preventDefault()
    if (name.trim().length === 0 || target.trim().length === 0) {
      return
    }
    createMutation.mutate({
      name: name.trim(),
      probe_type: probeType,
      target: target.trim(),
      interval,
      server_ids: selectedServerIds,
      enabled: true
    })
  }

  const targetPlaceholder: Record<ProbeType, string> = {
    icmp: 'e.g. 8.8.8.8 or google.com',
    tcp: 'e.g. google.com:443',
    http: 'e.g. https://google.com'
  }

  return (
    <div>
      <h1 className="mb-6 font-bold text-2xl">Ping Tasks</h1>

      <div className="max-w-3xl space-y-6">
        <div className="rounded-lg border bg-card p-6">
          <div className="mb-4 flex items-center justify-between">
            <h2 className="font-semibold text-lg">Probe Tasks</h2>
            <Button onClick={() => setShowForm(!showForm)} size="sm" variant="outline">
              <Plus className="size-4" />
              Add
            </Button>
          </div>

          {showForm && (
            <form className="mb-4 space-y-3 rounded-md border bg-muted/30 p-4" onSubmit={handleCreate}>
              <input
                className="flex h-9 w-full rounded-md border border-input bg-transparent px-3 py-1 text-sm shadow-sm placeholder:text-muted-foreground focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring"
                onChange={(e) => setName(e.target.value)}
                placeholder="Task name"
                required
                type="text"
                value={name}
              />

              <div className="flex gap-3">
                <select
                  className="flex h-9 flex-1 rounded-md border border-input bg-transparent px-3 py-1 text-sm"
                  onChange={(e) => setProbeType(e.target.value as ProbeType)}
                  value={probeType}
                >
                  {Object.entries(probeTypeLabels).map(([value, label]) => (
                    <option key={value} value={value}>
                      {label}
                    </option>
                  ))}
                </select>

                <input
                  className="flex h-9 w-24 rounded-md border border-input bg-transparent px-3 py-1 text-sm"
                  min={5}
                  onChange={(e) => setInterval(Number.parseInt(e.target.value, 10) || 60)}
                  placeholder="Interval"
                  type="number"
                  value={interval}
                />
                <span className="flex items-center text-muted-foreground text-sm">sec</span>
              </div>

              <input
                className="flex h-9 w-full rounded-md border border-input bg-transparent px-3 py-1 text-sm shadow-sm placeholder:text-muted-foreground focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring"
                onChange={(e) => setTarget(e.target.value)}
                placeholder={targetPlaceholder[probeType]}
                required
                type="text"
                value={target}
              />

              {servers && servers.length > 0 && (
                <fieldset className="space-y-1">
                  <legend className="text-sm">Run from servers (leave empty for all):</legend>
                  {servers.map((s) => (
                    <label className="flex items-center gap-2 text-sm" key={s.id}>
                      <input
                        checked={selectedServerIds.includes(s.id)}
                        onChange={(e) => {
                          setSelectedServerIds((prev) =>
                            e.target.checked ? [...prev, s.id] : prev.filter((sid) => sid !== s.id)
                          )
                        }}
                        type="checkbox"
                      />
                      {s.name}
                    </label>
                  ))}
                </fieldset>
              )}

              <div className="flex gap-2">
                <Button disabled={createMutation.isPending} size="sm" type="submit">
                  Create
                </Button>
                <Button onClick={resetForm} size="sm" type="button" variant="ghost">
                  Cancel
                </Button>
              </div>
            </form>
          )}

          {isLoading && (
            <div className="space-y-2">
              {Array.from({ length: 2 }, (_, i) => (
                <div className="h-12 animate-pulse rounded bg-muted" key={`skel-${i.toString()}`} />
              ))}
            </div>
          )}
          {!isLoading && (!tasks || tasks.length === 0) && (
            <p className="text-center text-muted-foreground text-sm">No ping tasks configured</p>
          )}
          {tasks && tasks.length > 0 && (
            <div className="divide-y rounded-md border">
              {tasks.map((task) => {
                let serverIds: string[] = []
                try {
                  serverIds = JSON.parse(task.server_ids_json || '[]') as string[]
                } catch {
                  // ignore malformed JSON
                }
                const isExpanded = expandedTaskId === task.id
                return (
                  <div key={task.id}>
                    <div className="flex items-center justify-between px-4 py-3">
                      <div className="flex items-center gap-3">
                        <Activity className={`size-4 ${task.enabled ? 'text-green-500' : 'text-muted-foreground'}`} />
                        <div>
                          <p className="font-medium text-sm">
                            {task.name}
                            {!task.enabled && <span className="ml-2 text-muted-foreground text-xs">(disabled)</span>}
                          </p>
                          <p className="text-muted-foreground text-xs">
                            {probeTypeLabels[task.probe_type as ProbeType] ?? task.probe_type} | {task.target} |{' '}
                            {task.interval}s
                            {serverIds.length > 0 ? ` | ${serverIds.length} server(s)` : ' | all servers'}
                          </p>
                        </div>
                      </div>
                      <div className="flex gap-1">
                        <Button
                          onClick={() => setExpandedTaskId(isExpanded ? null : task.id)}
                          size="sm"
                          variant="ghost"
                        >
                          <BarChart3 className="size-3.5" />
                        </Button>
                        <Button
                          onClick={() => toggleMutation.mutate({ id: task.id, enabled: !task.enabled })}
                          size="sm"
                          variant="outline"
                        >
                          {task.enabled ? 'Disable' : 'Enable'}
                        </Button>
                        <Button
                          aria-label={`Delete task ${task.name}`}
                          disabled={deleteMutation.isPending}
                          onClick={() => deleteMutation.mutate(task.id)}
                          size="sm"
                          variant="destructive"
                        >
                          <Trash2 className="size-3.5" />
                        </Button>
                      </div>
                    </div>
                    {isExpanded && (
                      <div className="border-t bg-muted/20 px-4 py-3">
                        <PingResultsChart taskId={task.id} />
                      </div>
                    )}
                  </div>
                )
              })}
            </div>
          )}
        </div>
      </div>
    </div>
  )
}

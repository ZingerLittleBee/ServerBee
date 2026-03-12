import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { createFileRoute } from '@tanstack/react-router'
import { Activity, Plus, Trash2 } from 'lucide-react'
import { type FormEvent, useState } from 'react'
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

function PingTasksPage() {
  const queryClient = useQueryClient()
  const [showForm, setShowForm] = useState(false)
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

      <div className="max-w-2xl space-y-6">
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
                            e.target.checked ? [...prev, s.id] : prev.filter((id) => id !== s.id)
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
                const serverIds: string[] = JSON.parse(task.server_ids_json || '[]')
                return (
                  <div className="flex items-center justify-between px-4 py-3" key={task.id}>
                    <div className="flex items-center gap-3">
                      <Activity className={`size-4 ${task.enabled ? 'text-green-500' : 'text-muted-foreground'}`} />
                      <div>
                        <p className="font-medium text-sm">
                          {task.name}
                          {!task.enabled && <span className="ml-2 text-muted-foreground text-xs">(disabled)</span>}
                        </p>
                        <p className="text-muted-foreground text-xs">
                          {probeTypeLabels[task.probe_type as ProbeType] ?? task.probe_type} | {task.target} |{' '}
                          {task.interval}s{serverIds.length > 0 ? ` | ${serverIds.length} server(s)` : ' | all servers'}
                        </p>
                      </div>
                    </div>
                    <div className="flex gap-1">
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
                )
              })}
            </div>
          )}
        </div>
      </div>
    </div>
  )
}

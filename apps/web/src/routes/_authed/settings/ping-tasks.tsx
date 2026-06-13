import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { createFileRoute } from '@tanstack/react-router'
import { Activity, BarChart3, Plus, Trash2 } from 'lucide-react'
import { type FormEvent, useMemo, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { Area, AreaChart, CartesianGrid, XAxis, YAxis } from 'recharts'
import { toast } from 'sonner'
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
  AlertDialogTrigger
} from '@/components/ui/alert-dialog'
import { Button } from '@/components/ui/button'
import { type ChartConfig, ChartContainer, ChartTooltip, ChartTooltipContent } from '@/components/ui/chart'
import { Checkbox } from '@/components/ui/checkbox'
import {
  Dialog,
  DialogBody,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  DialogTrigger
} from '@/components/ui/dialog'
import { Input } from '@/components/ui/input'
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select'
import { Skeleton } from '@/components/ui/skeleton'
import { api } from '@/lib/api-client'
import type { PingRecord, PingTask } from '@/lib/api-schema'

export const Route = createFileRoute('/_authed/settings/ping-tasks')({
  component: PingTasksPage
})

interface Server {
  id: string
  name: string
}

type ProbeType = 'http' | 'icmp' | 'tcp'

function PingResultsChart({ taskId }: { taskId: string }) {
  const { t } = useTranslation('settings')
  // biome-ignore lint/correctness/useExhaustiveDependencies: intentionally re-compute when taskId changes
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
    return <Skeleton className="h-48" />
  }

  if (!records || records.length === 0) {
    return <p className="py-4 text-center text-muted-foreground text-xs">{t('ping.no_records')}</p>
  }

  const chartData = records.map((r) => ({
    timestamp: r.time,
    latency: r.success ? r.latency : null
  }))

  const successRate = ((records.filter((r) => r.success).length / records.length) * 100).toFixed(1)
  const avgLatency =
    records.filter((r) => r.success).reduce((sum, r) => sum + r.latency, 0) /
    Math.max(1, records.filter((r) => r.success).length)

  const pingChartConfig = {
    latency: { label: 'Latency', color: 'var(--chart-4)' }
  } satisfies ChartConfig

  return (
    <div className="space-y-2">
      <div className="flex gap-4 text-muted-foreground text-xs">
        <span>{t('ping.success_rate', { rate: successRate })}</span>
        <span>{t('ping.avg_latency', { value: avgLatency.toFixed(1) })}</span>
        <span>{t('ping.record_count', { count: records.length })}</span>
      </div>
      <ChartContainer className="h-[180px] w-full" config={pingChartConfig}>
        <AreaChart accessibilityLayer data={chartData}>
          <CartesianGrid vertical={false} />
          <XAxis
            axisLine={false}
            dataKey="timestamp"
            tickFormatter={(v: string) =>
              new Date(v).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit', hour12: false })
            }
            tickLine={false}
          />
          <YAxis axisLine={false} tickLine={false} width={40} />
          <ChartTooltip
            content={
              <ChartTooltipContent
                labelFormatter={(label) => new Date(String(label)).toLocaleString([], { hour12: false })}
                valueFormatter={(v) => `${v.toFixed(1)}ms`}
              />
            }
          />
          <Area
            connectNulls={false}
            dataKey="latency"
            fill="var(--color-latency)"
            fillOpacity={0.1}
            stroke="var(--color-latency)"
            strokeWidth={2}
            type="monotone"
          />
        </AreaChart>
      </ChartContainer>
    </div>
  )
}

function PingTasksPage() {
  const { t } = useTranslation(['settings', 'common'])
  const queryClient = useQueryClient()
  const [showForm, setShowForm] = useState(false)
  const [expandedTaskId, setExpandedTaskId] = useState<string | null>(null)
  const [deleteId, setDeleteId] = useState<string | null>(null)
  const [name, setName] = useState('')
  const [probeType, setProbeType] = useState<ProbeType>('icmp')
  const [target, setTarget] = useState('')
  const [interval, setInterval] = useState(60)
  const [selectedServerIds, setSelectedServerIds] = useState<string[]>([])

  const probeTypeLabels: Record<ProbeType, string> = {
    icmp: t('ping.type_icmp'),
    tcp: t('ping.type_tcp'),
    http: t('ping.type_http')
  }

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
    if (selectedServerIds.length === 0) {
      toast.error(t('ping.no_servers_selected'))
      return
    }
    createMutation.mutate(
      {
        name: name.trim(),
        probe_type: probeType,
        target: target.trim(),
        interval,
        server_ids: selectedServerIds,
        enabled: true
      },
      {
        onSuccess: () => {
          toast.success(t('ping.task_created', { defaultValue: 'Ping task created' }))
        },
        onError: (err) => {
          toast.error(
            err instanceof Error
              ? err.message
              : t('ping.task_create_failed', { defaultValue: 'Failed to create ping task' })
          )
        }
      }
    )
  }

  const targetPlaceholder: Record<ProbeType, string> = {
    icmp: t('ping.placeholder_icmp'),
    tcp: t('ping.placeholder_tcp'),
    http: t('ping.placeholder_http')
  }

  return (
    <div>
      <div className="max-w-3xl space-y-4">
        <div className="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
          <h2 className="font-semibold text-lg">{t('ping.probe_tasks')}</h2>
          <Dialog
            onOpenChange={(open) => {
              setShowForm(open)
              if (!open) {
                setName('')
                setProbeType('icmp')
                setTarget('')
                setInterval(60)
                setSelectedServerIds([])
              }
            }}
            open={showForm}
          >
            <DialogTrigger render={<Button size="sm" variant="outline" />}>
              <Plus className="size-4" />
              {t('common:add')}
            </DialogTrigger>
            <DialogContent className="sm:max-w-md">
              <DialogHeader>
                <DialogTitle>{t('ping.add_title')}</DialogTitle>
                <DialogDescription>{t('ping.add_description')}</DialogDescription>
              </DialogHeader>
              <DialogBody>
                <form className="space-y-3" id="create-ping-task-form" onSubmit={handleCreate}>
                  <Input
                    onChange={(e) => setName(e.target.value)}
                    placeholder={t('ping.task_name')}
                    required
                    type="text"
                    value={name}
                  />

                  <div className="flex flex-col gap-3 sm:flex-row">
                    <Select
                      items={probeTypeLabels}
                      onValueChange={(value) => setProbeType(value as ProbeType)}
                      value={probeType}
                    >
                      <SelectTrigger className="w-full flex-1">
                        <SelectValue />
                      </SelectTrigger>
                      <SelectContent>
                        {Object.entries(probeTypeLabels).map(([value, label]) => (
                          <SelectItem key={value} value={value}>
                            {label}
                          </SelectItem>
                        ))}
                      </SelectContent>
                    </Select>

                    <Input
                      className="w-24"
                      min={5}
                      onChange={(e) => setInterval(Number.parseInt(e.target.value, 10) || 60)}
                      placeholder={t('ping.interval')}
                      type="number"
                      value={interval}
                    />
                    <span className="flex items-center text-muted-foreground text-sm">sec</span>
                  </div>

                  <Input
                    onChange={(e) => setTarget(e.target.value)}
                    placeholder={targetPlaceholder[probeType]}
                    required
                    type="text"
                    value={target}
                  />

                  {servers && servers.length > 0 && (
                    <fieldset className="space-y-2">
                      <div className="flex items-center justify-between gap-2">
                        <legend className="text-sm">{t('ping.run_from_servers')}</legend>
                        <button
                          className="text-primary text-xs hover:underline focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
                          onClick={() => {
                            setSelectedServerIds(
                              selectedServerIds.length === servers.length ? [] : servers.map((s) => s.id)
                            )
                          }}
                          type="button"
                        >
                          {selectedServerIds.length === servers.length ? t('ping.deselect_all') : t('ping.select_all')}
                        </button>
                      </div>
                      <div className="space-y-1 rounded-md border p-2">
                        {servers.map((s) => (
                          // biome-ignore lint/a11y/noLabelWithoutControl: Checkbox renders as a labelable button element
                          <label className="flex items-center gap-2 text-sm" key={s.id}>
                            <Checkbox
                              checked={selectedServerIds.includes(s.id)}
                              onCheckedChange={(checked) => {
                                setSelectedServerIds((prev) =>
                                  checked ? [...prev, s.id] : prev.filter((sid) => sid !== s.id)
                                )
                              }}
                            />
                            {s.name}
                          </label>
                        ))}
                      </div>
                    </fieldset>
                  )}
                </form>
              </DialogBody>
              <DialogFooter>
                <Button onClick={resetForm} size="sm" type="button" variant="ghost">
                  {t('common:cancel')}
                </Button>
                <Button disabled={createMutation.isPending} form="create-ping-task-form" size="sm" type="submit">
                  {t('common:create')}
                </Button>
              </DialogFooter>
            </DialogContent>
          </Dialog>
        </div>

        {isLoading && (
          <div className="space-y-2">
            {Array.from({ length: 2 }, (_, i) => (
              <Skeleton className="h-12" key={`skel-${i.toString()}`} />
            ))}
          </div>
        )}
        {!isLoading && (!tasks || tasks.length === 0) && (
          <p className="text-center text-muted-foreground text-sm">{t('ping.no_tasks')}</p>
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
                  <div className="flex flex-col gap-3 px-4 py-3 sm:flex-row sm:items-center sm:justify-between">
                    <div className="flex items-center gap-3">
                      <Activity className={`size-4 ${task.enabled ? 'text-green-500' : 'text-muted-foreground'}`} />
                      <div>
                        <p className="font-medium text-sm">
                          {task.name}
                          {!task.enabled && (
                            <span className="ml-2 text-muted-foreground text-xs">{t('ping.disabled')}</span>
                          )}
                        </p>
                        <p className="text-muted-foreground text-xs">
                          {probeTypeLabels[task.probe_type as ProbeType] ?? task.probe_type} | {task.target} |{' '}
                          {task.interval}s
                          {serverIds.length > 0
                            ? ` | ${t('ping.server_count', { count: serverIds.length })}`
                            : ` | ${t('ping.all_servers')}`}
                        </p>
                      </div>
                    </div>
                    <div className="flex gap-1">
                      <Button onClick={() => setExpandedTaskId(isExpanded ? null : task.id)} size="sm" variant="ghost">
                        <BarChart3 className="size-3.5" />
                      </Button>
                      <Button
                        disabled={toggleMutation.isPending}
                        onClick={() =>
                          toggleMutation.mutate(
                            { id: task.id, enabled: !task.enabled },
                            {
                              onSuccess: () => {
                                toast.success(
                                  task.enabled
                                    ? t('ping.task_disabled', { defaultValue: 'Ping task disabled' })
                                    : t('ping.task_enabled', { defaultValue: 'Ping task enabled' })
                                )
                              },
                              onError: (err) => {
                                toast.error(
                                  err instanceof Error
                                    ? err.message
                                    : t('ping.task_toggle_failed', { defaultValue: 'Failed to update ping task' })
                                )
                              }
                            }
                          )
                        }
                        size="sm"
                        variant="outline"
                      >
                        {task.enabled ? t('common:disable') : t('common:enable')}
                      </Button>
                      <AlertDialog
                        onOpenChange={(open) => {
                          if (!open) {
                            setDeleteId(null)
                          }
                        }}
                        open={deleteId === task.id}
                      >
                        <AlertDialogTrigger
                          onClick={() => setDeleteId(task.id)}
                          render={
                            <Button
                              aria-label={`Delete task ${task.name}`}
                              disabled={deleteMutation.isPending}
                              size="sm"
                              variant="destructive"
                            />
                          }
                        >
                          <Trash2 className="size-3.5" />
                        </AlertDialogTrigger>
                        <AlertDialogContent>
                          <AlertDialogHeader>
                            <AlertDialogTitle>{t('common:confirm_title')}</AlertDialogTitle>
                            <AlertDialogDescription>{t('common:confirm_delete_message')}</AlertDialogDescription>
                          </AlertDialogHeader>
                          <AlertDialogFooter>
                            <AlertDialogCancel>{t('common:cancel')}</AlertDialogCancel>
                            <AlertDialogAction
                              onClick={() => {
                                deleteMutation.mutate(task.id, {
                                  onSuccess: () => {
                                    toast.success(t('ping.task_deleted', { defaultValue: 'Ping task deleted' }))
                                  },
                                  onError: (err) => {
                                    toast.error(
                                      err instanceof Error
                                        ? err.message
                                        : t('ping.task_delete_failed', { defaultValue: 'Failed to delete ping task' })
                                    )
                                  }
                                })
                                setDeleteId(null)
                              }}
                              variant="destructive"
                            >
                              {t('common:delete')}
                            </AlertDialogAction>
                          </AlertDialogFooter>
                        </AlertDialogContent>
                      </AlertDialog>
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
  )
}

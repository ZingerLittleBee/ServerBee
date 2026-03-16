import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { createFileRoute } from '@tanstack/react-router'
import { AlertTriangle, Plus, Trash2 } from 'lucide-react'
import { type FormEvent, useState } from 'react'
import { useTranslation } from 'react-i18next'
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
import { Checkbox } from '@/components/ui/checkbox'
import { Input } from '@/components/ui/input'
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select'
import { Skeleton } from '@/components/ui/skeleton'
import { api } from '@/lib/api-client'
import type { AlertRule, AlertRuleItem, AlertStateResponse, NotificationGroup } from '@/lib/api-schema'

export const Route = createFileRoute('/_authed/settings/alerts')({
  component: AlertsPage
})

interface Server {
  id: string
  name: string
}

const THRESHOLD_TYPES = new Set([
  'cpu',
  'memory',
  'swap',
  'disk',
  'load1',
  'load5',
  'load15',
  'tcp_conn',
  'udp_conn',
  'process',
  'net_in_speed',
  'net_out_speed',
  'temperature',
  'gpu',
  'network_latency',
  'network_packet_loss'
])

const CYCLE_TYPES = new Set(['transfer_in_cycle', 'transfer_out_cycle', 'transfer_all_cycle'])

function formatRuleItem(item: AlertRuleItem, t: (key: string, options?: Record<string, unknown>) => string): string {
  if (item.rule_type === 'offline') {
    return `${t('alerts.display_offline')} ${item.duration ?? 60}s`
  }
  if (item.rule_type === 'expiration') {
    return t('alerts.display_expires', { count: item.duration ?? 7 })
  }
  if (item.cycle_limit) {
    return t('alerts.display_transfer', { value: item.cycle_limit, period: item.cycle_interval ?? 'month' })
  }
  if (item.min && item.max) {
    return `${item.rule_type} [${item.min}, ${item.max}]`
  }
  if (item.min) {
    return `${item.rule_type} >= ${item.min}`
  }
  if (item.max) {
    return `${item.rule_type} >= ${item.max}`
  }
  return item.rule_type
}

function AlertsPage() {
  const { t } = useTranslation(['settings', 'common'])
  const queryClient = useQueryClient()
  const [showForm, setShowForm] = useState(false)
  const [name, setName] = useState('')
  const [triggerMode, setTriggerMode] = useState('always')
  const [groupId, setGroupId] = useState('')
  const [ruleItems, setRuleItems] = useState<AlertRuleItem[]>([{ rule_type: 'cpu', min: 90 }])
  const [coverType, setCoverType] = useState<'all' | 'exclude' | 'include'>('all')
  const [serverIds, setServerIds] = useState<string[]>([])
  const [expandedRuleId, setExpandedRuleId] = useState<string | null>(null)
  const [deleteRuleId, setDeleteRuleId] = useState<string | null>(null)

  const ruleTypes = [
    { label: t('alerts.metric_cpu'), value: 'cpu' },
    { label: t('alerts.metric_memory'), value: 'memory' },
    { label: t('alerts.metric_swap'), value: 'swap' },
    { label: t('alerts.metric_disk'), value: 'disk' },
    { label: t('alerts.metric_load1'), value: 'load1' },
    { label: t('alerts.metric_load5'), value: 'load5' },
    { label: t('alerts.metric_load15'), value: 'load15' },
    { label: t('alerts.metric_tcp'), value: 'tcp_conn' },
    { label: t('alerts.metric_udp'), value: 'udp_conn' },
    { label: t('alerts.metric_processes'), value: 'process' },
    { label: t('alerts.metric_net_in'), value: 'net_in_speed' },
    { label: t('alerts.metric_net_out'), value: 'net_out_speed' },
    { label: t('alerts.metric_temperature'), value: 'temperature' },
    { label: t('alerts.metric_gpu'), value: 'gpu' },
    { label: t('alerts.metric_offline'), value: 'offline' },
    { label: t('alerts.metric_transfer_in'), value: 'transfer_in_cycle' },
    { label: t('alerts.metric_transfer_out'), value: 'transfer_out_cycle' },
    { label: t('alerts.metric_transfer_total'), value: 'transfer_all_cycle' },
    { label: t('alerts.metric_expiration'), value: 'expiration' },
    { label: 'Network Latency', value: 'network_latency' },
    { label: 'Network Packet Loss', value: 'network_packet_loss' }
  ]

  const { data: rules, isLoading } = useQuery<AlertRule[]>({
    queryKey: ['alert-rules'],
    queryFn: () => api.get<AlertRule[]>('/api/alert-rules')
  })

  const { data: groups } = useQuery<NotificationGroup[]>({
    queryKey: ['notification-groups'],
    queryFn: () => api.get<NotificationGroup[]>('/api/notification-groups')
  })

  const { data: servers } = useQuery<Server[]>({
    queryKey: ['servers'],
    queryFn: () => api.get<Server[]>('/api/servers'),
    enabled: showForm
  })

  const { data: states } = useQuery<AlertStateResponse[]>({
    queryKey: ['alert-rule-states', expandedRuleId],
    queryFn: () => api.get<AlertStateResponse[]>(`/api/alert-rules/${expandedRuleId}/states`),
    enabled: !!expandedRuleId,
    refetchInterval: 10_000
  })

  const createMutation = useMutation({
    mutationFn: (input: {
      cover_type: string
      name: string
      notification_group_id: string | null
      rules: AlertRuleItem[]
      server_ids: string[]
      trigger_mode: string
    }) => api.post<AlertRule>('/api/alert-rules', input),
    onSuccess: () => {
      invalidate()
      resetForm()
      toast.success('Alert rule created')
    },
    onError: (err) => {
      toast.error(err instanceof Error ? err.message : 'Failed to create alert rule')
    }
  })

  const deleteMutation = useMutation({
    mutationFn: (id: string) => api.delete(`/api/alert-rules/${id}`),
    onSuccess: () => {
      invalidate()
      toast.success('Alert rule deleted')
    },
    onError: (err) => {
      toast.error(err instanceof Error ? err.message : 'Failed to delete alert rule')
    }
  })

  const toggleMutation = useMutation({
    mutationFn: ({ enabled, id }: { enabled: boolean; id: string }) =>
      api.put<AlertRule>(`/api/alert-rules/${id}`, { enabled }),
    onSuccess: () => {
      invalidate()
      toast.success('Alert rule updated')
    },
    onError: (err) => {
      toast.error(err instanceof Error ? err.message : 'Failed to update alert rule')
    }
  })

  const invalidate = () => {
    queryClient.invalidateQueries({ queryKey: ['alert-rules'] }).catch(() => undefined)
  }

  const resetForm = () => {
    setName('')
    setTriggerMode('always')
    setGroupId('')
    setRuleItems([{ rule_type: 'cpu', min: 90 }])
    setCoverType('all')
    setServerIds([])
    setShowForm(false)
  }

  const handleCreate = (e: FormEvent) => {
    e.preventDefault()
    if (name.trim().length === 0 || ruleItems.length === 0) {
      return
    }
    createMutation.mutate({
      name: name.trim(),
      trigger_mode: triggerMode,
      notification_group_id: groupId || null,
      rules: ruleItems,
      cover_type: coverType,
      server_ids: coverType === 'include' || coverType === 'exclude' ? serverIds : []
    })
  }

  const addRuleItem = () => {
    setRuleItems((prev) => [...prev, { rule_type: 'cpu', min: 90 }])
  }

  const removeRuleItem = (index: number) => {
    setRuleItems((prev) => prev.filter((_, i) => i !== index))
  }

  const updateRuleItem = (index: number, field: keyof AlertRuleItem, value: number | string) => {
    setRuleItems((prev) => prev.map((item, i) => (i === index ? { ...item, [field]: value } : item)))
  }

  return (
    <div>
      <h1 className="mb-6 font-bold text-2xl">{t('alerts.title')}</h1>

      <div className="max-w-2xl space-y-6">
        <div className="rounded-lg border bg-card p-6">
          <div className="mb-4 flex items-center justify-between">
            <h2 className="font-semibold text-lg">{t('alerts.rules')}</h2>
            <Button onClick={() => setShowForm(!showForm)} size="sm" variant="outline">
              <Plus className="size-4" />
              {t('common:add')}
            </Button>
          </div>

          {showForm && (
            <form className="mb-4 space-y-3 rounded-md border bg-muted/30 p-4" onSubmit={handleCreate}>
              <Input
                aria-label={t('alerts.rule_name')}
                onChange={(e) => setName(e.target.value)}
                placeholder={t('alerts.rule_name')}
                required
                type="text"
                value={name}
              />

              <div className="flex gap-3">
                <Select onValueChange={(v) => v !== null && setTriggerMode(v)} value={triggerMode}>
                  <SelectTrigger aria-label={t('alerts.trigger_always')} className="h-9 w-full flex-1">
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectItem value="always">{t('alerts.trigger_always')}</SelectItem>
                    <SelectItem value="once">{t('alerts.trigger_once')}</SelectItem>
                  </SelectContent>
                </Select>

                <Select onValueChange={(v) => setGroupId(v ?? '')} value={groupId}>
                  <SelectTrigger aria-label={t('alerts.no_notification')} className="h-9 w-full flex-1">
                    <SelectValue placeholder={t('alerts.no_notification')} />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectItem value="">{t('alerts.no_notification')}</SelectItem>
                    {groups?.map((g) => (
                      <SelectItem key={g.id} value={g.id}>
                        {g.name}
                      </SelectItem>
                    ))}
                  </SelectContent>
                </Select>
              </div>

              <div className="space-y-2">
                <span className="font-medium text-sm">{t('alerts.coverage')}</span>
                <div className="flex gap-3">
                  <Select
                    onValueChange={(val) => {
                      if (val === null) {
                        return
                      }
                      const v = val as 'all' | 'exclude' | 'include'
                      setCoverType(v)
                      if (v === 'all') {
                        setServerIds([])
                      }
                    }}
                    value={coverType}
                  >
                    <SelectTrigger aria-label={t('alerts.coverage')} className="h-9 w-full flex-1">
                      <SelectValue />
                    </SelectTrigger>
                    <SelectContent>
                      <SelectItem value="all">{t('alerts.all_servers')}</SelectItem>
                      <SelectItem value="include">{t('alerts.include_servers')}</SelectItem>
                      <SelectItem value="exclude">{t('alerts.exclude_servers')}</SelectItem>
                    </SelectContent>
                  </Select>
                </div>
                {(coverType === 'include' || coverType === 'exclude') && (
                  <div className="flex flex-wrap gap-2 rounded-md border p-2">
                    {servers && servers.length > 0 ? (
                      servers.map((s) => (
                        // biome-ignore lint/a11y/noLabelWithoutControl: Checkbox renders as a labelable button element
                        <label className="flex items-center gap-1.5 text-sm" key={s.id}>
                          <Checkbox
                            checked={serverIds.includes(s.id)}
                            onCheckedChange={(checked) => {
                              setServerIds((prev) => (checked ? [...prev, s.id] : prev.filter((id) => id !== s.id)))
                            }}
                          />
                          {s.name}
                        </label>
                      ))
                    ) : (
                      <span className="text-muted-foreground text-xs">{t('alerts.no_servers')}</span>
                    )}
                  </div>
                )}
              </div>

              <div className="space-y-2">
                <div className="flex items-center justify-between">
                  <span className="font-medium text-sm">{t('alerts.conditions')}</span>
                  <Button onClick={addRuleItem} size="sm" type="button" variant="ghost">
                    <Plus className="size-3" />
                    {t('alerts.add_condition')}
                  </Button>
                </div>
                {ruleItems.map((item, index) => (
                  <div className="flex gap-2" key={`rule-${index.toString()}`}>
                    <Select
                      onValueChange={(val) => val !== null && updateRuleItem(index, 'rule_type', val)}
                      value={item.rule_type}
                    >
                      <SelectTrigger aria-label={t('alerts.conditions')} className="h-9 w-full flex-1">
                        <SelectValue />
                      </SelectTrigger>
                      <SelectContent>
                        {ruleTypes.map((rt) => (
                          <SelectItem key={rt.value} value={rt.value}>
                            {rt.label}
                          </SelectItem>
                        ))}
                      </SelectContent>
                    </Select>
                    {THRESHOLD_TYPES.has(item.rule_type) && (
                      <>
                        <Input
                          aria-label={t('alerts.threshold_gte')}
                          className="w-28"
                          onChange={(e) => updateRuleItem(index, 'min', Number.parseFloat(e.target.value) || 0)}
                          placeholder={t('alerts.threshold_gte')}
                          type="number"
                          value={item.min ?? ''}
                        />
                        <Input
                          aria-label={t('alerts.threshold_lte')}
                          className="w-28"
                          onChange={(e) => updateRuleItem(index, 'max', Number.parseFloat(e.target.value) || 0)}
                          placeholder={t('alerts.threshold_lte')}
                          type="number"
                          value={item.max ?? ''}
                        />
                      </>
                    )}
                    {item.rule_type === 'offline' && (
                      <Input
                        aria-label={t('alerts.duration')}
                        className="w-28"
                        onChange={(e) => updateRuleItem(index, 'duration', Number.parseInt(e.target.value, 10) || 60)}
                        placeholder={t('alerts.duration')}
                        type="number"
                        value={item.duration ?? 60}
                      />
                    )}
                    {item.rule_type === 'expiration' && (
                      <Input
                        aria-label={t('alerts.days_before')}
                        className="w-28"
                        onChange={(e) => updateRuleItem(index, 'duration', Number.parseInt(e.target.value, 10) || 7)}
                        placeholder={t('alerts.days_before')}
                        type="number"
                        value={item.duration ?? 7}
                      />
                    )}
                    {CYCLE_TYPES.has(item.rule_type) && (
                      <>
                        <Select
                          onValueChange={(val) => val !== null && updateRuleItem(index, 'cycle_interval', val)}
                          value={item.cycle_interval ?? 'month'}
                        >
                          <SelectTrigger aria-label={t('alerts.period_month')} className="h-9 w-28">
                            <SelectValue />
                          </SelectTrigger>
                          <SelectContent>
                            <SelectItem value="hour">{t('alerts.period_hour')}</SelectItem>
                            <SelectItem value="day">{t('alerts.period_day')}</SelectItem>
                            <SelectItem value="week">{t('alerts.period_week')}</SelectItem>
                            <SelectItem value="month">{t('alerts.period_month')}</SelectItem>
                            <SelectItem value="year">{t('alerts.period_year')}</SelectItem>
                          </SelectContent>
                        </Select>
                        <Input
                          aria-label={t('alerts.limit_bytes')}
                          className="w-28"
                          onChange={(e) =>
                            updateRuleItem(index, 'cycle_limit', Number.parseInt(e.target.value, 10) || 0)
                          }
                          placeholder={t('alerts.limit_bytes')}
                          type="number"
                          value={item.cycle_limit ?? ''}
                        />
                      </>
                    )}
                    {ruleItems.length > 1 && (
                      <Button
                        aria-label={t('common:delete')}
                        onClick={() => removeRuleItem(index)}
                        size="sm"
                        type="button"
                        variant="ghost"
                      >
                        <Trash2 aria-hidden="true" className="size-3" />
                      </Button>
                    )}
                  </div>
                ))}
              </div>

              <div className="flex gap-2">
                <Button disabled={createMutation.isPending} size="sm" type="submit">
                  {t('alerts.create_rule')}
                </Button>
                <Button onClick={resetForm} size="sm" type="button" variant="ghost">
                  {t('common:cancel')}
                </Button>
              </div>
            </form>
          )}

          {isLoading && (
            <div className="space-y-2">
              {Array.from({ length: 2 }, (_, i) => (
                <Skeleton className="h-12" key={`skel-${i.toString()}`} />
              ))}
            </div>
          )}
          {!isLoading && (!rules || rules.length === 0) && (
            <p className="text-center text-muted-foreground text-sm">{t('alerts.no_rules')}</p>
          )}
          {rules && rules.length > 0 && (
            <div className="divide-y rounded-md border">
              {rules.map((rule) => {
                const items: AlertRuleItem[] = JSON.parse(rule.rules_json || '[]')
                return (
                  <>
                    <div className="flex items-center justify-between px-4 py-3" key={rule.id}>
                      <div className="flex items-center gap-3">
                        <AlertTriangle
                          className={`size-4 ${rule.enabled ? 'text-amber-500' : 'text-muted-foreground'}`}
                        />
                        <div>
                          <p className="font-medium text-sm">
                            {rule.name}
                            {!rule.enabled && (
                              <span className="ml-2 text-muted-foreground text-xs">{t('notifications.disabled')}</span>
                            )}
                            <button
                              className="ml-2 rounded-full bg-muted px-2 py-0.5 text-muted-foreground text-xs hover:bg-muted/80"
                              onClick={(e) => {
                                e.stopPropagation()
                                setExpandedRuleId(expandedRuleId === rule.id ? null : rule.id)
                              }}
                              type="button"
                            >
                              {t('alerts.states')}
                            </button>
                          </p>
                          <p className="text-muted-foreground text-xs">
                            {items.map((item) => formatRuleItem(item, t)).join(' AND ')} | {rule.trigger_mode}
                          </p>
                        </div>
                      </div>
                      <div className="flex gap-1">
                        <Button
                          onClick={() => toggleMutation.mutate({ id: rule.id, enabled: !rule.enabled })}
                          size="sm"
                          variant="outline"
                        >
                          {rule.enabled ? t('common:disable') : t('common:enable')}
                        </Button>
                        <AlertDialog
                          onOpenChange={(open) => {
                            if (!open) {
                              setDeleteRuleId(null)
                            }
                          }}
                          open={deleteRuleId === rule.id}
                        >
                          <AlertDialogTrigger
                            onClick={() => setDeleteRuleId(rule.id)}
                            render={
                              <Button
                                aria-label={`${t('common:delete')} ${rule.name}`}
                                disabled={deleteMutation.isPending}
                                size="sm"
                                variant="destructive"
                              />
                            }
                          >
                            <Trash2 aria-hidden="true" className="size-3.5" />
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
                                  deleteMutation.mutate(rule.id)
                                  setDeleteRuleId(null)
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
                    {expandedRuleId === rule.id && (
                      <div className="border-t bg-muted/20 px-4 py-2">
                        {states && states.length > 0 ? (
                          <div className="space-y-1">
                            {states.map((s) => (
                              <div className="flex items-center justify-between text-xs" key={s.server_id}>
                                <span className="flex items-center gap-2">
                                  <span
                                    className={`size-2 rounded-full ${s.resolved ? 'bg-green-500' : 'bg-red-500'}`}
                                  />
                                  {s.server_name}
                                </span>
                                <span className="text-muted-foreground">
                                  {s.resolved ? t('alerts.resolved') : `${t('alerts.triggered')} (${s.count}x)`}
                                  {' · '}
                                  {new Date(s.first_triggered_at).toLocaleString()}
                                </span>
                              </div>
                            ))}
                          </div>
                        ) : (
                          <p className="text-muted-foreground text-xs">{t('alerts.no_triggered')}</p>
                        )}
                      </div>
                    )}
                  </>
                )
              })}
            </div>
          )}
        </div>
      </div>
    </div>
  )
}

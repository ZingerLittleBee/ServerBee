import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { createFileRoute } from '@tanstack/react-router'
import { AlertTriangle, Plus, Trash2 } from 'lucide-react'
import { type FormEvent, useState } from 'react'
import { Button } from '@/components/ui/button'
import { api } from '@/lib/api-client'
import type { AlertRule, AlertRuleItem, NotificationGroup } from '@/lib/api-schema'

export const Route = createFileRoute('/_authed/settings/alerts')({
  component: AlertsPage
})

interface Server {
  id: string
  name: string
}

const ruleTypes = [
  { label: 'CPU %', value: 'cpu' },
  { label: 'Load 1m', value: 'load1' },
  { label: 'Load 5m', value: 'load5' },
  { label: 'Load 15m', value: 'load15' },
  { label: 'TCP Connections', value: 'tcp_conn' },
  { label: 'UDP Connections', value: 'udp_conn' },
  { label: 'Processes', value: 'process' },
  { label: 'Network In (B/s)', value: 'net_in_speed' },
  { label: 'Network Out (B/s)', value: 'net_out_speed' },
  { label: 'Temperature', value: 'temperature' },
  { label: 'GPU %', value: 'gpu' },
  { label: 'Offline', value: 'offline' }
]

function AlertsPage() {
  const queryClient = useQueryClient()
  const [showForm, setShowForm] = useState(false)
  const [name, setName] = useState('')
  const [triggerMode, setTriggerMode] = useState('always')
  const [groupId, setGroupId] = useState('')
  const [ruleItems, setRuleItems] = useState<AlertRuleItem[]>([{ rule_type: 'cpu', max: 90 }])
  const [coverType, setCoverType] = useState<'all' | 'exclude' | 'include'>('all')
  const [serverIds, setServerIds] = useState<string[]>([])

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
    }
  })

  const deleteMutation = useMutation({
    mutationFn: (id: string) => api.delete(`/api/alert-rules/${id}`),
    onSuccess: () => invalidate()
  })

  const toggleMutation = useMutation({
    mutationFn: ({ enabled, id }: { enabled: boolean; id: string }) =>
      api.put<AlertRule>(`/api/alert-rules/${id}`, { enabled }),
    onSuccess: () => invalidate()
  })

  const invalidate = () => {
    queryClient.invalidateQueries({ queryKey: ['alert-rules'] }).catch(() => undefined)
  }

  const resetForm = () => {
    setName('')
    setTriggerMode('always')
    setGroupId('')
    setRuleItems([{ rule_type: 'cpu', max: 90 }])
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
    setRuleItems((prev) => [...prev, { rule_type: 'cpu', max: 90 }])
  }

  const removeRuleItem = (index: number) => {
    setRuleItems((prev) => prev.filter((_, i) => i !== index))
  }

  const updateRuleItem = (index: number, field: keyof AlertRuleItem, value: number | string) => {
    setRuleItems((prev) => prev.map((item, i) => (i === index ? { ...item, [field]: value } : item)))
  }

  return (
    <div>
      <h1 className="mb-6 font-bold text-2xl">Alert Rules</h1>

      <div className="max-w-2xl space-y-6">
        <div className="rounded-lg border bg-card p-6">
          <div className="mb-4 flex items-center justify-between">
            <h2 className="font-semibold text-lg">Rules</h2>
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
                placeholder="Rule name"
                required
                type="text"
                value={name}
              />

              <div className="flex gap-3">
                <select
                  className="flex h-9 flex-1 rounded-md border border-input bg-transparent px-3 py-1 text-sm"
                  onChange={(e) => setTriggerMode(e.target.value)}
                  value={triggerMode}
                >
                  <option value="always">Always (5min debounce)</option>
                  <option value="once">Once (until resolved)</option>
                </select>

                <select
                  className="flex h-9 flex-1 rounded-md border border-input bg-transparent px-3 py-1 text-sm"
                  onChange={(e) => setGroupId(e.target.value)}
                  value={groupId}
                >
                  <option value="">No notification</option>
                  {groups?.map((g) => (
                    <option key={g.id} value={g.id}>
                      {g.name}
                    </option>
                  ))}
                </select>
              </div>

              <div className="space-y-2">
                <span className="font-medium text-sm">Coverage</span>
                <div className="flex gap-3">
                  <select
                    className="flex h-9 flex-1 rounded-md border border-input bg-transparent px-3 py-1 text-sm"
                    onChange={(e) => {
                      const val = e.target.value as 'all' | 'exclude' | 'include'
                      setCoverType(val)
                      if (val === 'all') {
                        setServerIds([])
                      }
                    }}
                    value={coverType}
                  >
                    <option value="all">All servers</option>
                    <option value="include">Include servers</option>
                    <option value="exclude">Exclude servers</option>
                  </select>
                </div>
                {(coverType === 'include' || coverType === 'exclude') && (
                  <div className="flex flex-wrap gap-2 rounded-md border p-2">
                    {servers && servers.length > 0 ? (
                      servers.map((s) => (
                        <label className="flex items-center gap-1.5 text-sm" key={s.id}>
                          <input
                            checked={serverIds.includes(s.id)}
                            onChange={(e) => {
                              setServerIds((prev) =>
                                e.target.checked ? [...prev, s.id] : prev.filter((id) => id !== s.id)
                              )
                            }}
                            type="checkbox"
                          />
                          {s.name}
                        </label>
                      ))
                    ) : (
                      <span className="text-muted-foreground text-xs">No servers found</span>
                    )}
                  </div>
                )}
              </div>

              <div className="space-y-2">
                <div className="flex items-center justify-between">
                  <span className="font-medium text-sm">Conditions (AND)</span>
                  <Button onClick={addRuleItem} size="sm" type="button" variant="ghost">
                    <Plus className="size-3" />
                    Add condition
                  </Button>
                </div>
                {ruleItems.map((item, index) => (
                  <div className="flex gap-2" key={`rule-${index.toString()}`}>
                    <select
                      className="flex h-9 flex-1 rounded-md border border-input bg-transparent px-3 py-1 text-sm"
                      onChange={(e) => updateRuleItem(index, 'rule_type', e.target.value)}
                      value={item.rule_type}
                    >
                      {ruleTypes.map((rt) => (
                        <option key={rt.value} value={rt.value}>
                          {rt.label}
                        </option>
                      ))}
                    </select>
                    {item.rule_type !== 'offline' && (
                      <input
                        className="flex h-9 w-24 rounded-md border border-input bg-transparent px-3 py-1 text-sm"
                        onChange={(e) => updateRuleItem(index, 'max', Number.parseFloat(e.target.value) || 0)}
                        placeholder="Threshold"
                        type="number"
                        value={item.max ?? ''}
                      />
                    )}
                    {ruleItems.length > 1 && (
                      <Button onClick={() => removeRuleItem(index)} size="sm" type="button" variant="ghost">
                        <Trash2 className="size-3" />
                      </Button>
                    )}
                  </div>
                ))}
              </div>

              <div className="flex gap-2">
                <Button disabled={createMutation.isPending} size="sm" type="submit">
                  Create Rule
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
          {!isLoading && (!rules || rules.length === 0) && (
            <p className="text-center text-muted-foreground text-sm">No alert rules configured</p>
          )}
          {rules && rules.length > 0 && (
            <div className="divide-y rounded-md border">
              {rules.map((rule) => {
                const items: AlertRuleItem[] = JSON.parse(rule.rules_json || '[]')
                return (
                  <div className="flex items-center justify-between px-4 py-3" key={rule.id}>
                    <div className="flex items-center gap-3">
                      <AlertTriangle
                        className={`size-4 ${rule.enabled ? 'text-amber-500' : 'text-muted-foreground'}`}
                      />
                      <div>
                        <p className="font-medium text-sm">
                          {rule.name}
                          {!rule.enabled && <span className="ml-2 text-muted-foreground text-xs">(disabled)</span>}
                        </p>
                        <p className="text-muted-foreground text-xs">
                          {items.map((item) => `${item.rule_type}${item.max ? ` >= ${item.max}` : ''}`).join(' AND ')} |{' '}
                          {rule.trigger_mode}
                        </p>
                      </div>
                    </div>
                    <div className="flex gap-1">
                      <Button
                        onClick={() => toggleMutation.mutate({ id: rule.id, enabled: !rule.enabled })}
                        size="sm"
                        variant="outline"
                      >
                        {rule.enabled ? 'Disable' : 'Enable'}
                      </Button>
                      <Button
                        aria-label={`Delete rule ${rule.name}`}
                        disabled={deleteMutation.isPending}
                        onClick={() => deleteMutation.mutate(rule.id)}
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

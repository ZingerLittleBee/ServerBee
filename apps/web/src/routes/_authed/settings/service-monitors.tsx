import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { createFileRoute, Link } from '@tanstack/react-router'
import { Eye, Pencil, Play, Plus, Trash2 } from 'lucide-react'
import { type FormEvent, useState } from 'react'
import { toast } from 'sonner'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle
} from '@/components/ui/dialog'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select'
import { Skeleton } from '@/components/ui/skeleton'
import { Switch } from '@/components/ui/switch'
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from '@/components/ui/table'
import { api } from '@/lib/api-client'

export const Route = createFileRoute('/_authed/settings/service-monitors')({
  component: ServiceMonitorsPage
})

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

type MonitorType = 'ssl' | 'dns' | 'http_keyword' | 'tcp' | 'whois'

interface ServiceMonitor {
  config_json: string
  consecutive_failures: number
  created_at: string
  enabled: boolean
  id: string
  interval: number
  last_checked_at: string | null
  last_status: boolean | null
  monitor_type: string
  name: string
  notification_group_id: string | null
  retry_count: number
  server_ids_json: string | null
  target: string
  updated_at: string
}

interface CreateInput {
  config_json: Record<string, unknown>
  enabled: boolean
  interval: number
  monitor_type: string
  name: string
  target: string
}

interface UpdateInput {
  config_json?: Record<string, unknown>
  enabled?: boolean
  interval?: number
  name?: string
  target?: string
}

const MONITOR_TYPES: { label: string; value: MonitorType }[] = [
  { value: 'ssl', label: 'SSL' },
  { value: 'dns', label: 'DNS' },
  { value: 'http_keyword', label: 'HTTP Keyword' },
  { value: 'tcp', label: 'TCP' },
  { value: 'whois', label: 'WHOIS' }
]

const TYPE_LABELS: Record<string, string> = {
  ssl: 'SSL',
  dns: 'DNS',
  http_keyword: 'HTTP Keyword',
  tcp: 'TCP',
  whois: 'WHOIS'
}

// ---------------------------------------------------------------------------
// Type-specific config fields
// ---------------------------------------------------------------------------

function SslConfigFields({
  config,
  onChange
}: {
  config: Record<string, unknown>
  onChange: (c: Record<string, unknown>) => void
}) {
  return (
    <div className="grid gap-3 sm:grid-cols-2">
      <div className="space-y-1">
        <Label htmlFor="ssl-warning-days">Warning Days</Label>
        <Input
          id="ssl-warning-days"
          min={1}
          onChange={(e) => onChange({ ...config, warning_days: Number(e.target.value) || 14 })}
          type="number"
          value={(config.warning_days as number) ?? 14}
        />
      </div>
      <div className="space-y-1">
        <Label htmlFor="ssl-critical-days">Critical Days</Label>
        <Input
          id="ssl-critical-days"
          min={1}
          onChange={(e) => onChange({ ...config, critical_days: Number(e.target.value) || 7 })}
          type="number"
          value={(config.critical_days as number) ?? 7}
        />
      </div>
    </div>
  )
}

function DnsConfigFields({
  config,
  onChange
}: {
  config: Record<string, unknown>
  onChange: (c: Record<string, unknown>) => void
}) {
  const expectedStr = Array.isArray(config.expected_values) ? (config.expected_values as string[]).join('\n') : ''

  return (
    <div className="space-y-3">
      <div className="space-y-1">
        <Label htmlFor="dns-record-type">Record Type</Label>
        <Select
          onValueChange={(v) => onChange({ ...config, record_type: v })}
          value={(config.record_type as string) ?? 'A'}
        >
          <SelectTrigger id="dns-record-type">
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            {['A', 'AAAA', 'CNAME', 'MX', 'TXT'].map((rt) => (
              <SelectItem key={rt} value={rt}>
                {rt}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
      </div>
      <div className="space-y-1">
        <Label htmlFor="dns-expected">Expected Values (one per line)</Label>
        <textarea
          className="flex min-h-[60px] w-full rounded-md border border-input bg-transparent px-3 py-2 text-sm shadow-xs placeholder:text-muted-foreground focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring"
          id="dns-expected"
          onChange={(e) => {
            const values = e.target.value
              .split('\n')
              .map((v) => v.trim())
              .filter(Boolean)
            onChange({ ...config, expected_values: values.length > 0 ? values : undefined })
          }}
          placeholder="e.g. 93.184.216.34"
          rows={3}
          value={expectedStr}
        />
      </div>
      <div className="space-y-1">
        <Label htmlFor="dns-nameserver">Nameserver (optional)</Label>
        <Input
          id="dns-nameserver"
          onChange={(e) => onChange({ ...config, nameserver: e.target.value || undefined })}
          placeholder="e.g. 8.8.8.8"
          value={(config.nameserver as string) ?? ''}
        />
      </div>
    </div>
  )
}

function HttpKeywordConfigFields({
  config,
  onChange
}: {
  config: Record<string, unknown>
  onChange: (c: Record<string, unknown>) => void
}) {
  const expectedStatusStr = Array.isArray(config.expected_status)
    ? (config.expected_status as number[]).join(', ')
    : '200'

  return (
    <div className="space-y-3">
      <div className="grid gap-3 sm:grid-cols-2">
        <div className="space-y-1">
          <Label htmlFor="http-method">Method</Label>
          <Select onValueChange={(v) => onChange({ ...config, method: v })} value={(config.method as string) ?? 'GET'}>
            <SelectTrigger id="http-method">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="GET">GET</SelectItem>
              <SelectItem value="POST">POST</SelectItem>
            </SelectContent>
          </Select>
        </div>
        <div className="space-y-1">
          <Label htmlFor="http-timeout">Timeout (s)</Label>
          <Input
            id="http-timeout"
            min={1}
            onChange={(e) => onChange({ ...config, timeout: Number(e.target.value) || 10 })}
            type="number"
            value={(config.timeout as number) ?? 10}
          />
        </div>
      </div>
      <div className="space-y-1">
        <Label htmlFor="http-keyword">Keyword</Label>
        <Input
          id="http-keyword"
          onChange={(e) => onChange({ ...config, keyword: e.target.value || undefined })}
          placeholder="String to search in response body"
          value={(config.keyword as string) ?? ''}
        />
      </div>
      {/* biome-ignore lint/a11y/noLabelWithoutControl: Switch renders as a labelable button element */}
      <label className="flex items-center gap-2 text-sm">
        <Switch
          checked={(config.keyword_exists as boolean) ?? true}
          onCheckedChange={(checked) => onChange({ ...config, keyword_exists: checked })}
        />
        Keyword should exist in response
      </label>
      <div className="space-y-1">
        <Label htmlFor="http-expected-status">Expected Status Codes (comma-separated)</Label>
        <Input
          id="http-expected-status"
          onChange={(e) => {
            const codes = e.target.value
              .split(',')
              .map((s) => Number(s.trim()))
              .filter((n) => n > 0)
            onChange({ ...config, expected_status: codes.length > 0 ? codes : [200] })
          }}
          placeholder="200, 201"
          value={expectedStatusStr}
        />
      </div>
    </div>
  )
}

function TcpConfigFields({
  config,
  onChange
}: {
  config: Record<string, unknown>
  onChange: (c: Record<string, unknown>) => void
}) {
  return (
    <div className="space-y-1">
      <Label htmlFor="tcp-timeout">Timeout (s)</Label>
      <Input
        id="tcp-timeout"
        min={1}
        onChange={(e) => onChange({ ...config, timeout: Number(e.target.value) || 10 })}
        type="number"
        value={(config.timeout as number) ?? 10}
      />
    </div>
  )
}

function WhoisConfigFields({
  config,
  onChange
}: {
  config: Record<string, unknown>
  onChange: (c: Record<string, unknown>) => void
}) {
  return (
    <div className="grid gap-3 sm:grid-cols-2">
      <div className="space-y-1">
        <Label htmlFor="whois-warning-days">Warning Days</Label>
        <Input
          id="whois-warning-days"
          min={1}
          onChange={(e) => onChange({ ...config, warning_days: Number(e.target.value) || 30 })}
          type="number"
          value={(config.warning_days as number) ?? 30}
        />
      </div>
      <div className="space-y-1">
        <Label htmlFor="whois-critical-days">Critical Days</Label>
        <Input
          id="whois-critical-days"
          min={1}
          onChange={(e) => onChange({ ...config, critical_days: Number(e.target.value) || 7 })}
          type="number"
          value={(config.critical_days as number) ?? 7}
        />
      </div>
    </div>
  )
}

function TypeConfigFields({
  config,
  onChange,
  type
}: {
  config: Record<string, unknown>
  onChange: (c: Record<string, unknown>) => void
  type: MonitorType
}) {
  switch (type) {
    case 'ssl':
      return <SslConfigFields config={config} onChange={onChange} />
    case 'dns':
      return <DnsConfigFields config={config} onChange={onChange} />
    case 'http_keyword':
      return <HttpKeywordConfigFields config={config} onChange={onChange} />
    case 'tcp':
      return <TcpConfigFields config={config} onChange={onChange} />
    case 'whois':
      return <WhoisConfigFields config={config} onChange={onChange} />
    default:
      return null
  }
}

// ---------------------------------------------------------------------------
// Monitor Form Dialog
// ---------------------------------------------------------------------------

function MonitorFormDialog({
  editing,
  onClose,
  onSubmit,
  open,
  pending
}: {
  editing: ServiceMonitor | null
  onClose: () => void
  onSubmit: (data: CreateInput | UpdateInput, id?: string) => void
  open: boolean
  pending: boolean
}) {
  const [name, setName] = useState('')
  const [monitorType, setMonitorType] = useState<MonitorType>('ssl')
  const [target, setTarget] = useState('')
  const [interval, setIntervalVal] = useState(300)
  const [enabled, setEnabled] = useState(true)
  const [config, setConfig] = useState<Record<string, unknown>>({})

  // Reset form when dialog opens
  const handleOpenChange = (isOpen: boolean) => {
    if (isOpen && editing) {
      setName(editing.name)
      setMonitorType(editing.monitor_type as MonitorType)
      setTarget(editing.target)
      setIntervalVal(editing.interval)
      setEnabled(editing.enabled)
      try {
        setConfig(JSON.parse(editing.config_json) as Record<string, unknown>)
      } catch {
        setConfig({})
      }
    } else if (isOpen) {
      setName('')
      setMonitorType('ssl')
      setTarget('')
      setIntervalVal(300)
      setEnabled(true)
      setConfig({})
    }
    if (!isOpen) {
      onClose()
    }
  }

  const handleSubmit = (e: FormEvent) => {
    e.preventDefault()
    if (name.trim().length === 0 || target.trim().length === 0) {
      return
    }
    if (editing) {
      onSubmit(
        {
          name: name.trim(),
          target: target.trim(),
          interval,
          enabled,
          config_json: config
        },
        editing.id
      )
    } else {
      onSubmit({
        name: name.trim(),
        monitor_type: monitorType,
        target: target.trim(),
        interval,
        enabled,
        config_json: config
      })
    }
  }

  return (
    <Dialog onOpenChange={handleOpenChange} open={open}>
      <DialogContent className="sm:max-w-lg">
        <DialogHeader>
          <DialogTitle>{editing ? 'Edit Monitor' : 'Add Monitor'}</DialogTitle>
          <DialogDescription>
            {editing ? 'Update service monitor configuration.' : 'Create a new service monitor.'}
          </DialogDescription>
        </DialogHeader>
        <form className="space-y-4" id="monitor-form" onSubmit={handleSubmit}>
          <div className="space-y-1">
            <Label htmlFor="monitor-name">Name</Label>
            <Input
              id="monitor-name"
              onChange={(e) => setName(e.target.value)}
              placeholder="My SSL Check"
              required
              value={name}
            />
          </div>

          {!editing && (
            <div className="space-y-1">
              <Label htmlFor="monitor-type">Type</Label>
              <Select
                onValueChange={(v) => {
                  setMonitorType(v as MonitorType)
                  setConfig({})
                }}
                value={monitorType}
              >
                <SelectTrigger id="monitor-type">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  {MONITOR_TYPES.map((t) => (
                    <SelectItem key={t.value} value={t.value}>
                      {t.label}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </div>
          )}

          <div className="space-y-1">
            <Label htmlFor="monitor-target">Target</Label>
            <Input
              id="monitor-target"
              onChange={(e) => setTarget(e.target.value)}
              placeholder={getTargetPlaceholder((editing?.monitor_type as MonitorType) ?? monitorType)}
              required
              value={target}
            />
          </div>

          <div className="grid gap-3 sm:grid-cols-2">
            <div className="space-y-1">
              <Label htmlFor="monitor-interval">Interval (seconds)</Label>
              <Input
                id="monitor-interval"
                min={10}
                onChange={(e) => setIntervalVal(Number(e.target.value) || 300)}
                type="number"
                value={interval}
              />
            </div>
            <div className="flex items-end gap-2 pb-1">
              {/* biome-ignore lint/a11y/noLabelWithoutControl: Switch renders as a labelable button element */}
              <label className="flex items-center gap-2 text-sm">
                <Switch checked={enabled} onCheckedChange={setEnabled} />
                Enabled
              </label>
            </div>
          </div>

          <TypeConfigFields
            config={config}
            onChange={setConfig}
            type={(editing?.monitor_type as MonitorType) ?? monitorType}
          />
        </form>
        <DialogFooter>
          <Button disabled={pending} form="monitor-form" type="submit">
            {editing ? 'Save Changes' : 'Create'}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}

function getTargetPlaceholder(type: MonitorType): string {
  switch (type) {
    case 'ssl':
      return 'example.com or example.com:8443'
    case 'dns':
      return 'example.com'
    case 'http_keyword':
      return 'https://example.com/health'
    case 'tcp':
      return 'example.com:3306'
    case 'whois':
      return 'example.com'
    default:
      return 'target'
  }
}

// ---------------------------------------------------------------------------
// Status dot
// ---------------------------------------------------------------------------

function StatusDot({ status }: { status: boolean | null }) {
  if (status === null) {
    return <span className="inline-block size-2.5 rounded-full bg-muted-foreground/40" title="Not checked yet" />
  }
  return status ? (
    <span className="inline-block size-2.5 rounded-full bg-emerald-500" title="Up" />
  ) : (
    <span className="inline-block size-2.5 rounded-full bg-red-500" title="Down" />
  )
}

// ---------------------------------------------------------------------------
// Main Page
// ---------------------------------------------------------------------------

function ServiceMonitorsPage() {
  const queryClient = useQueryClient()
  const [dialogOpen, setDialogOpen] = useState(false)
  const [editing, setEditing] = useState<ServiceMonitor | null>(null)

  const { data: monitors, isLoading } = useQuery<ServiceMonitor[]>({
    queryKey: ['service-monitors'],
    queryFn: () => api.get<ServiceMonitor[]>('/api/service-monitors')
  })

  const invalidate = () => {
    queryClient.invalidateQueries({ queryKey: ['service-monitors'] }).catch(() => undefined)
  }

  const createMutation = useMutation({
    mutationFn: (input: CreateInput) => api.post<ServiceMonitor>('/api/service-monitors', input),
    onSuccess: () => {
      invalidate()
      setDialogOpen(false)
      toast.success('Monitor created')
    },
    onError: (err) => {
      toast.error(err instanceof Error ? err.message : 'Failed to create monitor')
    }
  })

  const updateMutation = useMutation({
    mutationFn: ({ id, input }: { id: string; input: UpdateInput }) =>
      api.put<ServiceMonitor>(`/api/service-monitors/${id}`, input),
    onSuccess: () => {
      invalidate()
      setDialogOpen(false)
      setEditing(null)
      toast.success('Monitor updated')
    },
    onError: (err) => {
      toast.error(err instanceof Error ? err.message : 'Failed to update monitor')
    }
  })

  const deleteMutation = useMutation({
    mutationFn: (id: string) => api.delete(`/api/service-monitors/${id}`),
    onSuccess: () => {
      invalidate()
      toast.success('Monitor deleted')
    },
    onError: (err) => {
      toast.error(err instanceof Error ? err.message : 'Failed to delete monitor')
    }
  })

  const triggerMutation = useMutation({
    mutationFn: (id: string) => api.post(`/api/service-monitors/${id}/check`),
    onSuccess: () => {
      invalidate()
      toast.success('Check triggered')
    },
    onError: (err) => {
      toast.error(err instanceof Error ? err.message : 'Failed to trigger check')
    }
  })

  const toggleMutation = useMutation({
    mutationFn: ({ enabled, id }: { enabled: boolean; id: string }) =>
      api.put<ServiceMonitor>(`/api/service-monitors/${id}`, { enabled }),
    onSuccess: () => {
      invalidate()
    },
    onError: (err) => {
      toast.error(err instanceof Error ? err.message : 'Failed to toggle monitor')
    }
  })

  const handleFormSubmit = (data: CreateInput | UpdateInput, id?: string) => {
    if (id) {
      updateMutation.mutate({ id, input: data as UpdateInput })
    } else {
      createMutation.mutate(data as CreateInput)
    }
  }

  const openCreate = () => {
    setEditing(null)
    setDialogOpen(true)
  }

  const openEdit = (monitor: ServiceMonitor) => {
    setEditing(monitor)
    setDialogOpen(true)
  }

  return (
    <div>
      <div className="mb-6 flex items-center justify-between">
        <h1 className="font-bold text-2xl">Service Monitors</h1>
        <Button onClick={openCreate} size="sm">
          <Plus className="size-4" />
          Add Monitor
        </Button>
      </div>

      <div className="max-w-5xl">
        {isLoading && (
          <div className="space-y-2">
            {Array.from({ length: 3 }, (_, i) => (
              <Skeleton className="h-12" key={`skel-${i.toString()}`} />
            ))}
          </div>
        )}

        {!isLoading && (!monitors || monitors.length === 0) && (
          <div className="rounded-lg border bg-card p-12 text-center">
            <p className="text-muted-foreground">No service monitors configured yet.</p>
            <Button className="mt-4" onClick={openCreate} size="sm" variant="outline">
              <Plus className="size-4" />
              Create your first monitor
            </Button>
          </div>
        )}

        {monitors && monitors.length > 0 && (
          <div className="rounded-lg border">
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>Status</TableHead>
                  <TableHead>Name</TableHead>
                  <TableHead>Type</TableHead>
                  <TableHead>Target</TableHead>
                  <TableHead>Interval</TableHead>
                  <TableHead>Enabled</TableHead>
                  <TableHead>Last Checked</TableHead>
                  <TableHead className="text-right">Actions</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {monitors.map((monitor) => (
                  <TableRow key={monitor.id}>
                    <TableCell>
                      <StatusDot status={monitor.last_status} />
                    </TableCell>
                    <TableCell className="font-medium">{monitor.name}</TableCell>
                    <TableCell>
                      <Badge variant="secondary">{TYPE_LABELS[monitor.monitor_type] ?? monitor.monitor_type}</Badge>
                    </TableCell>
                    <TableCell className="max-w-[200px] truncate font-mono text-xs">{monitor.target}</TableCell>
                    <TableCell>{monitor.interval}s</TableCell>
                    <TableCell>
                      <Switch
                        checked={monitor.enabled}
                        onCheckedChange={(checked) => toggleMutation.mutate({ id: monitor.id, enabled: checked })}
                        size="sm"
                      />
                    </TableCell>
                    <TableCell className="text-muted-foreground text-xs">
                      {monitor.last_checked_at ? new Date(monitor.last_checked_at).toLocaleString() : 'Never'}
                    </TableCell>
                    <TableCell className="text-right">
                      <div className="flex justify-end gap-1">
                        <Link params={{ id: monitor.id }} to="/service-monitors/$id">
                          <Button aria-label="View details" size="sm" variant="ghost">
                            <Eye className="size-3.5" />
                          </Button>
                        </Link>
                        <Button
                          aria-label="Trigger check"
                          disabled={triggerMutation.isPending}
                          onClick={() => triggerMutation.mutate(monitor.id)}
                          size="sm"
                          variant="ghost"
                        >
                          <Play className="size-3.5" />
                        </Button>
                        <Button aria-label="Edit monitor" onClick={() => openEdit(monitor)} size="sm" variant="ghost">
                          <Pencil className="size-3.5" />
                        </Button>
                        <Button
                          aria-label={`Delete monitor ${monitor.name}`}
                          disabled={deleteMutation.isPending}
                          onClick={() => deleteMutation.mutate(monitor.id)}
                          size="sm"
                          variant="ghost"
                        >
                          <Trash2 className="size-3.5 text-destructive" />
                        </Button>
                      </div>
                    </TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
          </div>
        )}
      </div>

      <MonitorFormDialog
        editing={editing}
        onClose={() => {
          setDialogOpen(false)
          setEditing(null)
        }}
        onSubmit={handleFormSubmit}
        open={dialogOpen}
        pending={createMutation.isPending || updateMutation.isPending}
      />
    </div>
  )
}

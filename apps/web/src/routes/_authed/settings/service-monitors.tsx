import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { createFileRoute, Link } from '@tanstack/react-router'
import { Eye, Pencil, Play, Plus, Trash2 } from 'lucide-react'
import { type FormEvent, useEffect, useState } from 'react'
import { useTranslation } from 'react-i18next'
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

const WHOIS_UNSUPPORTED_TLDS = new Set(['app', 'dev', 'page'])
const TRAILING_DOTS_RE = /\.+$/

function useMonitorTypes(t: (key: string) => string): { label: string; value: MonitorType }[] {
  return [
    { value: 'ssl', label: t('monitorTypes.ssl') },
    { value: 'dns', label: t('monitorTypes.dns') },
    { value: 'http_keyword', label: t('monitorTypes.http_keyword') },
    { value: 'tcp', label: t('monitorTypes.tcp') },
    { value: 'whois', label: t('monitorTypes.whois') }
  ]
}

function useTypeLabels(t: (key: string) => string): Record<string, string> {
  return {
    ssl: t('monitorTypes.ssl'),
    dns: t('monitorTypes.dns'),
    http_keyword: t('monitorTypes.http_keyword'),
    tcp: t('monitorTypes.tcp'),
    whois: t('monitorTypes.whois')
  }
}

function parseConfigJson(configJson: string): Record<string, unknown> {
  try {
    const parsed: unknown = JSON.parse(configJson)
    if (parsed && typeof parsed === 'object' && !Array.isArray(parsed)) {
      return parsed as Record<string, unknown>
    }
  } catch {
    // Ignore malformed config and fall back to defaults.
  }

  return {}
}

function normalizeWhoisTarget(target: string): string | null {
  const trimmed = target.trim()
  if (!trimmed) {
    return null
  }

  try {
    const candidate = trimmed.includes('://') ? trimmed : `https://${trimmed}`
    const url = new URL(candidate)
    return url.hostname.trim().replace(TRAILING_DOTS_RE, '').toLowerCase() || null
  } catch {
    return null
  }
}

function isUnsupportedWhoisTld(target: string): boolean {
  const parts = target.split('.')
  const tld = parts.at(-1) ?? ''
  return WHOIS_UNSUPPORTED_TLDS.has(tld)
}

// ---------------------------------------------------------------------------
// Type-specific config fields
// ---------------------------------------------------------------------------

function SslConfigFields({
  config,
  onChange,
  t
}: {
  config: Record<string, unknown>
  onChange: (c: Record<string, unknown>) => void
  t: (key: string) => string
}) {
  return (
    <div className="grid gap-3 sm:grid-cols-2">
      <div className="space-y-1">
        <Label htmlFor="ssl-warning-days">{t('sslConfig.warningDays')}</Label>
        <Input
          id="ssl-warning-days"
          min={1}
          onChange={(e) => onChange({ ...config, warning_days: Number(e.target.value) || 14 })}
          type="number"
          value={(config.warning_days as number) ?? 14}
        />
      </div>
      <div className="space-y-1">
        <Label htmlFor="ssl-critical-days">{t('sslConfig.criticalDays')}</Label>
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
  onChange,
  t
}: {
  config: Record<string, unknown>
  onChange: (c: Record<string, unknown>) => void
  t: (key: string) => string
}) {
  const expectedStr = Array.isArray(config.expected_values) ? (config.expected_values as string[]).join('\n') : ''

  return (
    <div className="space-y-3">
      <div className="space-y-1">
        <Label htmlFor="dns-record-type">{t('dnsConfig.recordType')}</Label>
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
        <Label htmlFor="dns-expected">{t('dnsConfig.expectedValues')}</Label>
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
          placeholder={t('dnsExpectedPlaceholder')}
          rows={3}
          value={expectedStr}
        />
      </div>
      <div className="space-y-1">
        <Label htmlFor="dns-nameserver">{t('dnsConfig.nameserver')}</Label>
        <Input
          id="dns-nameserver"
          onChange={(e) => onChange({ ...config, nameserver: e.target.value || undefined })}
          placeholder={t('dnsNameserverPlaceholder')}
          value={(config.nameserver as string) ?? ''}
        />
      </div>
    </div>
  )
}

function HttpKeywordConfigFields({
  config,
  onChange,
  t
}: {
  config: Record<string, unknown>
  onChange: (c: Record<string, unknown>) => void
  t: (key: string) => string
}) {
  const expectedStatusStr = Array.isArray(config.expected_status)
    ? (config.expected_status as number[]).join(', ')
    : '200'

  return (
    <div className="space-y-3">
      <div className="grid gap-3 sm:grid-cols-2">
        <div className="space-y-1">
          <Label htmlFor="http-method">{t('httpConfig.method')}</Label>
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
          <Label htmlFor="http-timeout">{t('httpConfig.timeout')}</Label>
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
        <Label htmlFor="http-keyword">{t('httpConfig.keyword')}</Label>
        <Input
          id="http-keyword"
          onChange={(e) => onChange({ ...config, keyword: e.target.value || undefined })}
          placeholder={t('httpKeywordPlaceholder')}
          value={(config.keyword as string) ?? ''}
        />
      </div>
      {/* biome-ignore lint/a11y/noLabelWithoutControl: Switch renders as a labelable button element */}
      <label className="flex items-center gap-2 text-sm">
        <Switch
          checked={(config.keyword_exists as boolean) ?? true}
          onCheckedChange={(checked) => onChange({ ...config, keyword_exists: checked })}
        />
        {t('httpConfig.keywordExists')}
      </label>
      <div className="space-y-1">
        <Label htmlFor="http-expected-status">{t('httpConfig.expectedStatus')}</Label>
        <Input
          id="http-expected-status"
          onChange={(e) => {
            const codes = e.target.value
              .split(',')
              .map((s) => Number(s.trim()))
              .filter((n) => n > 0)
            onChange({ ...config, expected_status: codes.length > 0 ? codes : [200] })
          }}
          placeholder={t('httpStatusPlaceholder')}
          value={expectedStatusStr}
        />
      </div>
    </div>
  )
}

function TcpConfigFields({
  config,
  onChange,
  t
}: {
  config: Record<string, unknown>
  onChange: (c: Record<string, unknown>) => void
  t: (key: string) => string
}) {
  return (
    <div className="space-y-1">
      <Label htmlFor="tcp-timeout">{t('tcpConfig.timeout')}</Label>
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
  onChange,
  t
}: {
  config: Record<string, unknown>
  onChange: (c: Record<string, unknown>) => void
  t: (key: string) => string
}) {
  return (
    <div className="grid gap-3 sm:grid-cols-2">
      <div className="space-y-1">
        <Label htmlFor="whois-warning-days">{t('whoisConfig.warningDays')}</Label>
        <Input
          id="whois-warning-days"
          min={1}
          onChange={(e) => onChange({ ...config, warning_days: Number(e.target.value) || 30 })}
          type="number"
          value={(config.warning_days as number) ?? 30}
        />
      </div>
      <div className="space-y-1">
        <Label htmlFor="whois-critical-days">{t('whoisConfig.criticalDays')}</Label>
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
  type,
  t
}: {
  config: Record<string, unknown>
  onChange: (c: Record<string, unknown>) => void
  type: MonitorType
  t: (key: string) => string
}) {
  switch (type) {
    case 'ssl':
      return <SslConfigFields config={config} onChange={onChange} t={t} />
    case 'dns':
      return <DnsConfigFields config={config} onChange={onChange} t={t} />
    case 'http_keyword':
      return <HttpKeywordConfigFields config={config} onChange={onChange} t={t} />
    case 'tcp':
      return <TcpConfigFields config={config} onChange={onChange} t={t} />
    case 'whois':
      return <WhoisConfigFields config={config} onChange={onChange} t={t} />
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
  const { t } = useTranslation('service-monitors')
  const { t: tCommon } = useTranslation('common')
  const MONITOR_TYPES = useMonitorTypes(t)

  const [name, setName] = useState('')
  const [monitorType, setMonitorType] = useState<MonitorType>('ssl')
  const [target, setTarget] = useState('')
  const [interval, setIntervalVal] = useState(300)
  const [enabled, setEnabled] = useState(true)
  const [config, setConfig] = useState<Record<string, unknown>>({})
  const [targetError, setTargetError] = useState<string | null>(null)

  const activeMonitorType = (editing?.monitor_type as MonitorType) ?? monitorType
  const normalizedWhoisTarget = activeMonitorType === 'whois' ? normalizeWhoisTarget(target) : null
  const showUnsupportedWhoisHint =
    activeMonitorType === 'whois' && normalizedWhoisTarget ? isUnsupportedWhoisTld(normalizedWhoisTarget) : false

  useEffect(() => {
    if (!open) {
      return
    }

    setTargetError(null)

    if (editing) {
      setName(editing.name)
      setMonitorType(editing.monitor_type as MonitorType)
      setTarget(editing.target)
      setIntervalVal(editing.interval)
      setEnabled(editing.enabled)
      setConfig(parseConfigJson(editing.config_json))
      return
    }

    setName('')
    setMonitorType('ssl')
    setTarget('')
    setIntervalVal(300)
    setEnabled(true)
    setConfig({})
  }, [editing, open])

  const handleOpenChange = (isOpen: boolean) => {
    if (!isOpen) {
      setName('')
      setTarget('')
      setConfig({})
      setTargetError(null)
      onClose()
    }
  }

  const handleSubmit = (e: FormEvent) => {
    e.preventDefault()
    const trimmedName = name.trim()
    const trimmedTarget = target.trim()

    if (trimmedName.length === 0 || trimmedTarget.length === 0) {
      return
    }

    const normalizedTarget = activeMonitorType === 'whois' ? normalizeWhoisTarget(trimmedTarget) : trimmedTarget

    if (activeMonitorType === 'whois' && !normalizedTarget) {
      setTargetError(t('whoisConfig.targetInvalid'))
      return
    }

    setTargetError(null)

    if (editing) {
      onSubmit(
        {
          name: trimmedName,
          target: normalizedTarget ?? trimmedTarget,
          interval,
          enabled,
          config_json: config
        },
        editing.id
      )
    } else {
      onSubmit({
        name: trimmedName,
        monitor_type: monitorType,
        target: normalizedTarget ?? trimmedTarget,
        interval,
        enabled,
        config_json: config
      })
    }
  }

  function getTargetPlaceholder(type: MonitorType): string {
    switch (type) {
      case 'ssl':
        return t('targetPlaceholder.ssl')
      case 'dns':
        return t('targetPlaceholder.dns')
      case 'http_keyword':
        return t('targetPlaceholder.http_keyword')
      case 'tcp':
        return t('targetPlaceholder.tcp')
      case 'whois':
        return t('targetPlaceholder.whois')
      default:
        return 'target'
    }
  }

  return (
    <Dialog onOpenChange={handleOpenChange} open={open}>
      <DialogContent className="sm:max-w-lg">
        <DialogHeader>
          <DialogTitle>{editing ? t('dialog.editTitle') : t('dialog.addTitle')}</DialogTitle>
          <DialogDescription>{editing ? t('dialog.editDescription') : t('dialog.addDescription')}</DialogDescription>
        </DialogHeader>
        <form className="space-y-4" id="monitor-form" onSubmit={handleSubmit}>
          <div className="space-y-1">
            <Label htmlFor="monitor-name">{t('form.name')}</Label>
            <Input
              id="monitor-name"
              onChange={(e) => setName(e.target.value)}
              placeholder={t('namePlaceholder')}
              required
              value={name}
            />
          </div>

          {!editing && (
            <div className="space-y-1">
              <Label htmlFor="monitor-type">{t('form.type')}</Label>
              <Select
                items={MONITOR_TYPES}
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
            <Label htmlFor="monitor-target">{t('form.target')}</Label>
            <Input
              aria-invalid={targetError ? true : undefined}
              id="monitor-target"
              onChange={(e) => {
                setTarget(e.target.value)
                if (targetError) {
                  setTargetError(null)
                }
              }}
              placeholder={getTargetPlaceholder(activeMonitorType)}
              required
              value={target}
            />
            {activeMonitorType === 'whois' && (
              <div className="space-y-1">
                <p className="text-muted-foreground text-xs">{t('whoisConfig.targetHint')}</p>
                {normalizedWhoisTarget && (
                  <p className="text-muted-foreground text-xs">
                    {t('whoisConfig.targetPreview', { target: normalizedWhoisTarget })}
                  </p>
                )}
                {showUnsupportedWhoisHint && (
                  <p className="text-amber-600 text-xs dark:text-amber-400">{t('whoisConfig.unsupportedTldHint')}</p>
                )}
                {targetError && <p className="text-destructive text-xs">{targetError}</p>}
              </div>
            )}
          </div>

          <div className="grid gap-3 sm:grid-cols-2">
            <div className="space-y-1">
              <Label htmlFor="monitor-interval">{t('form.interval')}</Label>
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
                {t('form.enabled')}
              </label>
            </div>
          </div>

          <TypeConfigFields config={config} onChange={setConfig} t={t} type={activeMonitorType} />
        </form>
        <DialogFooter>
          <Button disabled={pending} form="monitor-form" type="submit">
            {editing ? tCommon('actions.save') : tCommon('actions.create')}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}

// ---------------------------------------------------------------------------
// Status dot
// ---------------------------------------------------------------------------

function StatusDot({ status, t }: { status: boolean | null; t: (key: string) => string }) {
  if (status === null) {
    return <span className="inline-block size-2.5 rounded-full bg-muted-foreground/40" title={t('status.notChecked')} />
  }
  return status ? (
    <span className="inline-block size-2.5 rounded-full bg-emerald-500" title={t('status.up')} />
  ) : (
    <span className="inline-block size-2.5 rounded-full bg-red-500" title={t('status.down')} />
  )
}

// ---------------------------------------------------------------------------
// Main Page
// ---------------------------------------------------------------------------

export function ServiceMonitorsPage() {
  const { t } = useTranslation('service-monitors')
  const { t: tCommon } = useTranslation('common')
  const TYPE_LABELS = useTypeLabels(t)
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
      toast.success(t('toast.created'))
    },
    onError: (err) => {
      toast.error(err instanceof Error ? err.message : t('toast.createFailed'))
    }
  })

  const updateMutation = useMutation({
    mutationFn: ({ id, input }: { id: string; input: UpdateInput }) =>
      api.put<ServiceMonitor>(`/api/service-monitors/${id}`, input),
    onSuccess: () => {
      invalidate()
      setDialogOpen(false)
      setEditing(null)
      toast.success(t('toast.updated'))
    },
    onError: (err) => {
      toast.error(err instanceof Error ? err.message : t('toast.updateFailed'))
    }
  })

  const deleteMutation = useMutation({
    mutationFn: (id: string) => api.delete(`/api/service-monitors/${id}`),
    onSuccess: () => {
      invalidate()
      toast.success(t('toast.deleted'))
    },
    onError: (err) => {
      toast.error(err instanceof Error ? err.message : t('toast.deleteFailed'))
    }
  })

  const triggerMutation = useMutation({
    mutationFn: (id: string) => api.post(`/api/service-monitors/${id}/check`),
    onSuccess: () => {
      invalidate()
      toast.success(t('toast.checkTriggered'))
    },
    onError: (err) => {
      toast.error(err instanceof Error ? err.message : t('toast.triggerFailed'))
    }
  })

  const toggleMutation = useMutation({
    mutationFn: ({ enabled, id }: { enabled: boolean; id: string }) =>
      api.put<ServiceMonitor>(`/api/service-monitors/${id}`, { enabled }),
    onSuccess: () => {
      invalidate()
    },
    onError: (err) => {
      toast.error(err instanceof Error ? err.message : t('toast.toggleFailed'))
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
        <h1 className="font-bold text-2xl">{t('page.title')}</h1>
        <Button onClick={openCreate} size="sm">
          <Plus className="size-4" />
          {tCommon('actions.addMonitor')}
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
            <p className="text-muted-foreground">{t('empty.noMonitors')}</p>
            <Button className="mt-4" onClick={openCreate} size="sm" variant="outline">
              <Plus className="size-4" />
              {t('empty.createFirst')}
            </Button>
          </div>
        )}

        {monitors && monitors.length > 0 && (
          <div className="rounded-lg border">
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>{t('table.status')}</TableHead>
                  <TableHead>{t('table.name')}</TableHead>
                  <TableHead>{t('table.type')}</TableHead>
                  <TableHead>{t('table.target')}</TableHead>
                  <TableHead>{t('table.interval')}</TableHead>
                  <TableHead>{t('table.enabled')}</TableHead>
                  <TableHead>{t('table.lastChecked')}</TableHead>
                  <TableHead className="text-right">{t('table.actions')}</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {monitors.map((monitor) => (
                  <TableRow key={monitor.id}>
                    <TableCell>
                      <StatusDot status={monitor.last_status} t={t} />
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
                      {monitor.last_checked_at
                        ? new Date(monitor.last_checked_at).toLocaleString()
                        : tCommon('status.never')}
                    </TableCell>
                    <TableCell className="text-right">
                      <div className="flex justify-end gap-1">
                        <Link params={{ id: monitor.id }} to="/service-monitors/$id">
                          <Button aria-label={t('aria.viewDetails')} size="sm" variant="ghost">
                            <Eye className="size-3.5" />
                          </Button>
                        </Link>
                        <Button
                          aria-label={t('aria.triggerCheck')}
                          disabled={triggerMutation.isPending}
                          onClick={() => triggerMutation.mutate(monitor.id)}
                          size="sm"
                          variant="ghost"
                        >
                          <Play className="size-3.5" />
                        </Button>
                        <Button aria-label={t('aria.edit')} onClick={() => openEdit(monitor)} size="sm" variant="ghost">
                          <Pencil className="size-3.5" />
                        </Button>
                        <Button
                          aria-label={`${t('aria.deleteMonitor')} ${monitor.name}`}
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

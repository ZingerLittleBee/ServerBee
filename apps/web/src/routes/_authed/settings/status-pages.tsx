import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { createFileRoute } from '@tanstack/react-router'
import { AlertTriangle, ExternalLink, Pencil, Plus, Trash2 } from 'lucide-react'
import { type FormEvent, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { toast } from 'sonner'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card'
import { Checkbox } from '@/components/ui/checkbox'
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
import { ScrollArea } from '@/components/ui/scroll-area'
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select'
import { Skeleton } from '@/components/ui/skeleton'
import { Switch } from '@/components/ui/switch'
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from '@/components/ui/table'
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs'
import { Textarea } from '@/components/ui/textarea'
import { api } from '@/lib/api-client'
import type {
  CreateIncidentRequest,
  CreateMaintenanceRequest,
  IncidentItem,
  MaintenanceItem,
  ServerResponse,
  StatusPageItem,
  UpdateIncidentRequest,
  UpdateMaintenanceRequest,
  UpdateStatusPageRequest
} from '@/lib/api-schema'

export const Route = createFileRoute('/_authed/settings/status-pages')({
  component: StatusPagesManagement
})

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/** Parse the entity's `server_ids_json` storage column into a `string[]`. */
export function parseServerIds(raw: string | null | undefined): string[] {
  if (!raw) {
    return []
  }
  try {
    const parsed = JSON.parse(raw) as unknown
    if (Array.isArray(parsed)) {
      return parsed.filter((v): v is string => typeof v === 'string')
    }
  } catch {
    // ignore malformed JSON; fall through to []
  }
  return []
}

function ServerCheckboxItem({ checked, name, onToggle }: { checked: boolean; name: string; onToggle: () => void }) {
  return (
    // biome-ignore lint/a11y/noLabelWithoutControl: Checkbox renders as a labelable button element
    <label className="flex items-center gap-2 text-sm">
      <Checkbox checked={checked} onCheckedChange={onToggle} />
      {name}
    </label>
  )
}

// ---------------------------------------------------------------------------
// Singleton Status-Page Config
// ---------------------------------------------------------------------------

interface ConfigFormState {
  defaultLayout: 'grid' | 'list'
  description: string
  enabled: boolean
  redThreshold: number
  selectedServers: string[]
  showIncidents: boolean
  showIpQuality: boolean
  showMaintenance: boolean
  showNetwork: boolean
  showServerDetail: boolean
  title: string
  yellowThreshold: number
}

function configFromItem(item: StatusPageItem): ConfigFormState {
  return {
    defaultLayout: item.default_layout === 'grid' ? 'grid' : 'list',
    description: item.description ?? '',
    enabled: item.enabled,
    redThreshold: item.uptime_red_threshold,
    selectedServers: parseServerIds(item.server_ids_json),
    showIncidents: item.show_incidents,
    showIpQuality: item.show_ip_quality,
    showMaintenance: item.show_maintenance,
    showNetwork: item.show_network,
    showServerDetail: item.show_server_detail,
    title: item.title,
    yellowThreshold: item.uptime_yellow_threshold
  }
}

/** Build the PUT body. Sends every field so the admin can save a fully-edited
 * form in one round-trip; matches the prevailing settings UX in this app. */
export function buildStatusPageUpdatePayload(state: ConfigFormState): UpdateStatusPageRequest {
  return {
    default_layout: state.defaultLayout,
    description: state.description.trim() ? state.description.trim() : null,
    enabled: state.enabled,
    server_ids: state.selectedServers,
    show_incidents: state.showIncidents,
    show_ip_quality: state.showIpQuality,
    show_maintenance: state.showMaintenance,
    show_network: state.showNetwork,
    show_server_detail: state.showServerDetail,
    title: state.title.trim(),
    uptime_red_threshold: state.redThreshold,
    uptime_yellow_threshold: state.yellowThreshold
  }
}

function StatusPageConfigForm({ servers }: { servers: ServerResponse[] }) {
  const { t } = useTranslation(['settings', 'common'])
  const queryClient = useQueryClient()

  const { data: config, isLoading } = useQuery<StatusPageItem>({
    queryKey: ['status-page-config'],
    queryFn: () => api.get<StatusPageItem>('/api/status-page')
  })

  const [state, setState] = useState<ConfigFormState | null>(null)

  // Lazy initialise local form state from server data on first load.
  if (config && state === null) {
    setState(configFromItem(config))
  }

  const mutation = useMutation({
    mutationFn: (input: UpdateStatusPageRequest) => api.put<StatusPageItem>('/api/status-page', input),
    onSuccess: (next) => {
      queryClient.setQueryData(['status-page-config'], next)
      queryClient.invalidateQueries({ queryKey: ['status-page-config'] }).catch(() => undefined)
      toast.success(t('status_pages.config_saved'))
    },
    onError: (err) => toast.error(err instanceof Error ? err.message : t('common:errors.failed'))
  })

  if (isLoading || !state) {
    return (
      <div className="space-y-2">
        {Array.from({ length: 4 }, (_, i) => (
          <Skeleton className="h-12" key={`skel-${i.toString()}`} />
        ))}
      </div>
    )
  }

  const update = <K extends keyof ConfigFormState>(key: K, value: ConfigFormState[K]) => {
    setState((prev) => (prev ? { ...prev, [key]: value } : prev))
  }

  const toggleServer = (id: string) => {
    setState((prev) => {
      if (!prev) {
        return prev
      }
      const next = prev.selectedServers.includes(id)
        ? prev.selectedServers.filter((s) => s !== id)
        : [...prev.selectedServers, id]
      return { ...prev, selectedServers: next }
    })
  }

  const handleSubmit = (e: FormEvent<HTMLFormElement>) => {
    e.preventDefault()
    if (!state.title.trim()) {
      toast.error(t('status_pages.title_required'))
      return
    }
    mutation.mutate(buildStatusPageUpdatePayload(state))
  }

  return (
    <form className="space-y-6" onSubmit={handleSubmit}>
      <Card>
        <CardHeader>
          <CardTitle>{t('status_pages.section_general')}</CardTitle>
          <CardDescription>
            <a
              className="inline-flex items-center gap-1 font-mono text-primary text-xs hover:underline"
              href="/status"
              rel="noopener noreferrer"
              target="_blank"
            >
              /status
              <ExternalLink className="size-3" />
            </a>
          </CardDescription>
        </CardHeader>
        <CardContent className="space-y-4">
          {!state.enabled && (
            <div className="flex items-start gap-2 rounded-md border border-amber-500/40 bg-amber-500/10 p-3 text-amber-700 text-sm dark:text-amber-300">
              <AlertTriangle className="mt-0.5 size-4 shrink-0" />
              <p>{t('status_pages.site_disabled_notice_admin')}</p>
            </div>
          )}

          <div className="flex items-center justify-between gap-4">
            <div className="space-y-0.5">
              <Label htmlFor="sp-enabled">{t('status_pages.field_enabled')}</Label>
              <p className="text-muted-foreground text-xs">{t('status_pages.field_enabled_hint')}</p>
            </div>
            <Switch checked={state.enabled} id="sp-enabled" onCheckedChange={(value) => update('enabled', value)} />
          </div>

          <div className="space-y-1">
            <Label htmlFor="sp-title">{t('status_pages.field_title')}</Label>
            <Input
              id="sp-title"
              onChange={(e) => update('title', e.target.value)}
              placeholder={t('status_pages.placeholder_title')}
              required
              value={state.title}
            />
          </div>

          <div className="space-y-1">
            <Label htmlFor="sp-desc">{t('status_pages.field_description')}</Label>
            <Textarea
              id="sp-desc"
              onChange={(e) => update('description', e.target.value)}
              placeholder={t('status_pages.placeholder_description')}
              rows={2}
              value={state.description}
            />
          </div>
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle>{t('status_pages.section_servers')}</CardTitle>
          <CardDescription>{t('status_pages.section_servers_description')}</CardDescription>
        </CardHeader>
        <CardContent className="space-y-4">
          <div className="space-y-1">
            <Label htmlFor="sp-layout">{t('status_pages.field_default_layout')}</Label>
            <Select
              items={{
                list: t('status_pages.layout_list'),
                grid: t('status_pages.layout_grid')
              }}
              onValueChange={(value) => {
                if (value === 'list' || value === 'grid') {
                  update('defaultLayout', value)
                }
              }}
              value={state.defaultLayout}
            >
              <SelectTrigger id="sp-layout">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="list">{t('status_pages.layout_list')}</SelectItem>
                <SelectItem value="grid">{t('status_pages.layout_grid')}</SelectItem>
              </SelectContent>
            </Select>
          </div>

          <div className="space-y-2">
            <Label>{t('status_pages.field_servers')}</Label>
            <ScrollArea className="h-40 rounded-md border">
              <div className="space-y-1 p-2">
                {servers.map((s) => (
                  <ServerCheckboxItem
                    checked={state.selectedServers.includes(s.id)}
                    key={s.id}
                    name={s.name}
                    onToggle={() => toggleServer(s.id)}
                  />
                ))}
                {servers.length === 0 && (
                  <p className="text-muted-foreground text-xs">{t('status_pages.no_servers')}</p>
                )}
              </div>
            </ScrollArea>
            <p className="text-muted-foreground text-xs">{t('status_pages.field_servers_hint')}</p>
          </div>
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle>{t('status_pages.section_panels')}</CardTitle>
          <CardDescription>{t('status_pages.section_panels_description')}</CardDescription>
        </CardHeader>
        <CardContent className="space-y-3">
          <PanelToggle
            checked={state.showServerDetail}
            description={t('status_pages.field_show_server_detail_hint')}
            id="sp-show-server-detail"
            label={t('status_pages.field_show_server_detail')}
            onChange={(v) => update('showServerDetail', v)}
          />
          <PanelToggle
            checked={state.showNetwork}
            description={t('status_pages.field_show_network_hint')}
            id="sp-show-network"
            label={t('status_pages.field_show_network')}
            onChange={(v) => update('showNetwork', v)}
          />
          <PanelToggle
            checked={state.showIpQuality}
            description={t('status_pages.field_show_ip_quality_hint')}
            id="sp-show-ip-quality"
            label={t('status_pages.field_show_ip_quality')}
            onChange={(v) => update('showIpQuality', v)}
          />
          <PanelToggle
            checked={state.showIncidents}
            description={t('status_pages.field_show_incidents_hint')}
            id="sp-show-incidents"
            label={t('status_pages.field_show_incidents')}
            onChange={(v) => update('showIncidents', v)}
          />
          <PanelToggle
            checked={state.showMaintenance}
            description={t('status_pages.field_show_maintenance_hint')}
            id="sp-show-maintenance"
            label={t('status_pages.field_show_maintenance')}
            onChange={(v) => update('showMaintenance', v)}
          />
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle>{t('status_pages.section_thresholds')}</CardTitle>
          <CardDescription>{t('status_pages.section_thresholds_description')}</CardDescription>
        </CardHeader>
        <CardContent>
          <div className="grid gap-3 sm:grid-cols-2">
            <div className="space-y-1">
              <Label htmlFor="sp-yellow">{t('status_pages.uptime_yellow_label')}</Label>
              <Input
                id="sp-yellow"
                max={100}
                min={0}
                onChange={(e) => update('yellowThreshold', Number(e.target.value) || 100)}
                step={0.1}
                type="number"
                value={state.yellowThreshold}
              />
              <p className="text-muted-foreground text-xs">{t('status_pages.uptime_yellow_hint')}</p>
            </div>
            <div className="space-y-1">
              <Label htmlFor="sp-red">{t('status_pages.uptime_red_label')}</Label>
              <Input
                id="sp-red"
                max={100}
                min={0}
                onChange={(e) => update('redThreshold', Number(e.target.value) || 95)}
                step={0.1}
                type="number"
                value={state.redThreshold}
              />
              <p className="text-muted-foreground text-xs">{t('status_pages.uptime_red_hint')}</p>
            </div>
          </div>
        </CardContent>
      </Card>

      <div className="flex justify-end">
        <Button disabled={mutation.isPending} type="submit">
          {t('common:save')}
        </Button>
      </div>
    </form>
  )
}

function PanelToggle({
  checked,
  description,
  id,
  label,
  onChange
}: {
  checked: boolean
  description: string
  id: string
  label: string
  onChange: (next: boolean) => void
}) {
  return (
    <div className="flex items-center justify-between gap-4">
      <div className="space-y-0.5">
        <Label htmlFor={id}>{label}</Label>
        <p className="text-muted-foreground text-xs">{description}</p>
      </div>
      <Switch checked={checked} id={id} onCheckedChange={onChange} />
    </div>
  )
}

// ---------------------------------------------------------------------------
// Incidents Tab
// ---------------------------------------------------------------------------

const INCIDENT_STATUSES = ['investigating', 'identified', 'monitoring', 'resolved'] as const
const INCIDENT_SEVERITIES = ['minor', 'major', 'critical'] as const

function IncidentFormDialog({
  editing,
  onClose,
  onSubmit,
  open,
  pending,
  servers
}: {
  editing: IncidentItem | null
  onClose: () => void
  onSubmit: (data: CreateIncidentRequest | UpdateIncidentRequest, id?: string) => void
  open: boolean
  pending: boolean
  servers: ServerResponse[]
}) {
  const { t } = useTranslation('settings')
  const [title, setTitle] = useState('')
  const [severity, setSeverity] = useState<string>('minor')
  const [status, setStatus] = useState<string>('investigating')
  const [selectedServers, setSelectedServers] = useState<string[]>([])
  const [isPublic, setIsPublic] = useState(false)

  const handleOpenChange = (isOpen: boolean) => {
    if (isOpen && editing) {
      setTitle(editing.title)
      setSeverity(editing.severity)
      setStatus(editing.status)
      setSelectedServers(parseServerIds(editing.server_ids_json))
      setIsPublic(editing.is_public)
    } else if (isOpen) {
      setTitle('')
      setSeverity('minor')
      setStatus('investigating')
      setSelectedServers([])
      setIsPublic(false)
    }
    if (!isOpen) {
      onClose()
    }
  }

  const handleSubmit = (e: FormEvent<HTMLFormElement>) => {
    e.preventDefault()
    if (!title.trim()) {
      return
    }
    const payload: CreateIncidentRequest | UpdateIncidentRequest = {
      title: title.trim(),
      severity,
      status,
      server_ids_json: selectedServers,
      is_public: isPublic
    }
    onSubmit(payload, editing?.id)
  }

  const toggleServer = (id: string) => {
    setSelectedServers((prev) => (prev.includes(id) ? prev.filter((s) => s !== id) : [...prev, id]))
  }

  return (
    <Dialog onOpenChange={handleOpenChange} open={open}>
      <DialogContent className="sm:max-w-lg">
        <DialogHeader>
          <DialogTitle>{editing ? t('incidents.edit') : t('incidents.create')}</DialogTitle>
          <DialogDescription>
            {editing ? t('incidents.edit_description') : t('incidents.create_description')}
          </DialogDescription>
        </DialogHeader>
        <form className="space-y-4" id="incident-form" onSubmit={handleSubmit}>
          <div className="space-y-1">
            <Label htmlFor="inc-title">{t('incidents.field_title')}</Label>
            <Input
              id="inc-title"
              onChange={(e) => setTitle(e.target.value)}
              placeholder={t('incidents.placeholder_title')}
              required
              value={title}
            />
          </div>
          <div className="grid gap-3 sm:grid-cols-2">
            <div className="space-y-1">
              <Label htmlFor="inc-severity">{t('incidents.field_severity')}</Label>
              <Select onValueChange={(v) => v && setSeverity(v)} value={severity}>
                <SelectTrigger id="inc-severity">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  {INCIDENT_SEVERITIES.map((s) => (
                    <SelectItem key={s} value={s}>
                      {t(`incidents.severity_${s}`)}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </div>
            <div className="space-y-1">
              <Label htmlFor="inc-status">{t('incidents.field_status')}</Label>
              <Select onValueChange={(v) => v && setStatus(v)} value={status}>
                <SelectTrigger id="inc-status">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  {INCIDENT_STATUSES.map((s) => (
                    <SelectItem key={s} value={s}>
                      {t(`incidents.status_${s}`)}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </div>
          </div>
          <div className="flex items-center justify-between gap-4 rounded-md border p-3">
            <div className="space-y-0.5">
              <Label htmlFor="inc-public">{t('incidents.field_is_public')}</Label>
              <p className="text-muted-foreground text-xs">{t('incidents.field_is_public_hint')}</p>
            </div>
            <Switch checked={isPublic} id="inc-public" onCheckedChange={setIsPublic} />
          </div>
          <div className="space-y-2">
            <Label>{t('incidents.field_servers')}</Label>
            <ScrollArea className="h-32 rounded-md border">
              <div className="space-y-1 p-2">
                {servers.map((s) => (
                  <ServerCheckboxItem
                    checked={selectedServers.includes(s.id)}
                    key={s.id}
                    name={s.name}
                    onToggle={() => toggleServer(s.id)}
                  />
                ))}
              </div>
            </ScrollArea>
          </div>
        </form>
        <DialogFooter>
          <Button disabled={pending} form="incident-form" type="submit">
            {editing ? t('common:save') : t('common:create')}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}

function IncidentUpdateDialog({
  incidentId,
  onClose,
  open
}: {
  incidentId: string
  onClose: () => void
  open: boolean
}) {
  const { t } = useTranslation('settings')
  const queryClient = useQueryClient()
  const [message, setMessage] = useState('')
  const [status, setStatus] = useState<string>('investigating')

  const addUpdateMutation = useMutation({
    mutationFn: (input: { message: string; status: string }) => api.post(`/api/incidents/${incidentId}/updates`, input),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['incidents'] }).catch(() => undefined)
      onClose()
      setMessage('')
      toast.success(t('incidents.update_added'))
    },
    onError: (err) => toast.error(err instanceof Error ? err.message : t('common:errors.failed'))
  })

  const handleSubmit = (e: FormEvent<HTMLFormElement>) => {
    e.preventDefault()
    if (!message.trim()) {
      return
    }
    addUpdateMutation.mutate({ message: message.trim(), status })
  }

  return (
    <Dialog
      onOpenChange={(isOpen) => {
        if (!isOpen) {
          onClose()
        }
      }}
      open={open}
    >
      <DialogContent className="sm:max-w-md">
        <DialogHeader>
          <DialogTitle>{t('incidents.add_update')}</DialogTitle>
          <DialogDescription>{t('incidents.add_update_description')}</DialogDescription>
        </DialogHeader>
        <form className="space-y-4" id="incident-update-form" onSubmit={handleSubmit}>
          <div className="space-y-1">
            <Label htmlFor="upd-status">{t('incidents.field_status')}</Label>
            <Select onValueChange={(v) => v && setStatus(v)} value={status}>
              <SelectTrigger id="upd-status">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                {INCIDENT_STATUSES.map((s) => (
                  <SelectItem key={s} value={s}>
                    {t(`incidents.status_${s}`)}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </div>
          <div className="space-y-1">
            <Label htmlFor="upd-message">{t('incidents.field_message')}</Label>
            <Textarea
              id="upd-message"
              onChange={(e) => setMessage(e.target.value)}
              placeholder={t('incidents.placeholder_message')}
              required
              rows={3}
              value={message}
            />
          </div>
        </form>
        <DialogFooter>
          <Button disabled={addUpdateMutation.isPending} form="incident-update-form" type="submit">
            {t('incidents.post_update')}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}

function IncidentsTab({ servers }: { servers: ServerResponse[] }) {
  const { t } = useTranslation('settings')
  const queryClient = useQueryClient()
  const [dialogOpen, setDialogOpen] = useState(false)
  const [editing, setEditing] = useState<IncidentItem | null>(null)
  const [updateDialogIncidentId, setUpdateDialogIncidentId] = useState<string | null>(null)

  const { data: incidents, isLoading } = useQuery<IncidentItem[]>({
    queryKey: ['incidents'],
    queryFn: () => api.get<IncidentItem[]>('/api/incidents')
  })

  const invalidate = () => {
    queryClient.invalidateQueries({ queryKey: ['incidents'] }).catch(() => undefined)
  }

  const createMutation = useMutation({
    mutationFn: (input: CreateIncidentRequest) => api.post('/api/incidents', input),
    onSuccess: () => {
      invalidate()
      setDialogOpen(false)
      toast.success(t('incidents.created'))
    },
    onError: (err) => toast.error(err instanceof Error ? err.message : t('common:errors.failed'))
  })

  const updateMutation = useMutation({
    mutationFn: ({ id, input }: { id: string; input: UpdateIncidentRequest }) => api.put(`/api/incidents/${id}`, input),
    onSuccess: () => {
      invalidate()
      setDialogOpen(false)
      setEditing(null)
      toast.success(t('incidents.updated'))
    },
    onError: (err) => toast.error(err instanceof Error ? err.message : t('common:errors.failed'))
  })

  const deleteMutation = useMutation({
    mutationFn: (id: string) => api.delete(`/api/incidents/${id}`),
    onSuccess: () => {
      invalidate()
      toast.success(t('incidents.deleted'))
    },
    onError: (err) => toast.error(err instanceof Error ? err.message : t('common:errors.failed'))
  })

  const handleSubmit = (data: CreateIncidentRequest | UpdateIncidentRequest, id?: string) => {
    if (id) {
      updateMutation.mutate({ id, input: data as UpdateIncidentRequest })
    } else {
      createMutation.mutate(data as CreateIncidentRequest)
    }
  }

  return (
    <div>
      <div className="mb-4 flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
        <p className="text-muted-foreground text-sm">{t('incidents.tab_description')}</p>
        <Button
          onClick={() => {
            setEditing(null)
            setDialogOpen(true)
          }}
          size="sm"
        >
          <Plus className="size-4" />
          {t('incidents.create')}
        </Button>
      </div>

      {isLoading && (
        <div className="space-y-2">
          {Array.from({ length: 2 }, (_, i) => (
            <Skeleton className="h-12" key={`skel-${i.toString()}`} />
          ))}
        </div>
      )}

      {!isLoading && (!incidents || incidents.length === 0) && (
        <div className="rounded-lg border bg-card p-12 text-center">
          <p className="text-muted-foreground">{t('incidents.empty')}</p>
        </div>
      )}

      {incidents && incidents.length > 0 && (
        <div className="rounded-lg border">
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>{t('incidents.field_title')}</TableHead>
                <TableHead>{t('incidents.field_severity')}</TableHead>
                <TableHead>{t('incidents.field_status')}</TableHead>
                <TableHead>{t('incidents.field_is_public')}</TableHead>
                <TableHead>{t('incidents.col_created')}</TableHead>
                <TableHead className="text-right">{t('status_pages.col_actions')}</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {incidents.map((incident) => (
                <TableRow key={incident.id}>
                  <TableCell className="font-medium">{incident.title}</TableCell>
                  <TableCell>
                    <Badge variant={incident.severity === 'critical' ? 'destructive' : 'secondary'}>
                      {incident.severity}
                    </Badge>
                  </TableCell>
                  <TableCell>
                    <Badge variant={incident.status === 'resolved' ? 'default' : 'outline'}>{incident.status}</Badge>
                  </TableCell>
                  <TableCell>
                    <Badge variant={incident.is_public ? 'default' : 'secondary'}>
                      {incident.is_public ? t('incidents.is_public_yes') : t('incidents.is_public_no')}
                    </Badge>
                  </TableCell>
                  <TableCell className="text-muted-foreground text-xs">
                    {new Date(incident.created_at).toLocaleDateString()}
                  </TableCell>
                  <TableCell className="text-right">
                    <div className="flex justify-end gap-1">
                      <Button
                        onClick={() => setUpdateDialogIncidentId(incident.id)}
                        size="sm"
                        title={t('incidents.add_update')}
                        variant="ghost"
                      >
                        <Plus className="size-3.5" />
                      </Button>
                      <Button
                        onClick={() => {
                          setEditing(incident)
                          setDialogOpen(true)
                        }}
                        size="sm"
                        variant="ghost"
                      >
                        <Pencil className="size-3.5" />
                      </Button>
                      <Button
                        disabled={deleteMutation.isPending}
                        onClick={() => deleteMutation.mutate(incident.id)}
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

      <IncidentFormDialog
        editing={editing}
        onClose={() => {
          setDialogOpen(false)
          setEditing(null)
        }}
        onSubmit={handleSubmit}
        open={dialogOpen}
        pending={createMutation.isPending || updateMutation.isPending}
        servers={servers}
      />

      {updateDialogIncidentId && (
        <IncidentUpdateDialog
          incidentId={updateDialogIncidentId}
          onClose={() => setUpdateDialogIncidentId(null)}
          open={!!updateDialogIncidentId}
        />
      )}
    </div>
  )
}

// ---------------------------------------------------------------------------
// Maintenance Tab
// ---------------------------------------------------------------------------

function MaintenanceFormDialog({
  editing,
  onClose,
  onSubmit,
  open,
  pending,
  servers
}: {
  editing: MaintenanceItem | null
  onClose: () => void
  onSubmit: (data: CreateMaintenanceRequest | UpdateMaintenanceRequest, id?: string) => void
  open: boolean
  pending: boolean
  servers: ServerResponse[]
}) {
  const { t } = useTranslation('settings')
  const [title, setTitle] = useState('')
  const [description, setDescription] = useState('')
  const [startAt, setStartAt] = useState('')
  const [endAt, setEndAt] = useState('')
  const [active, setActive] = useState(true)
  const [selectedServers, setSelectedServers] = useState<string[]>([])
  const [isPublic, setIsPublic] = useState(false)

  const handleOpenChange = (isOpen: boolean) => {
    if (isOpen && editing) {
      setTitle(editing.title)
      setDescription(editing.description ?? '')
      setStartAt(editing.start_at.slice(0, 16))
      setEndAt(editing.end_at.slice(0, 16))
      setActive(editing.active)
      setSelectedServers(parseServerIds(editing.server_ids_json))
      setIsPublic(editing.is_public)
    } else if (isOpen) {
      setTitle('')
      setDescription('')
      setStartAt('')
      setEndAt('')
      setActive(true)
      setSelectedServers([])
      setIsPublic(false)
    }
    if (!isOpen) {
      onClose()
    }
  }

  const handleSubmit = (e: FormEvent<HTMLFormElement>) => {
    e.preventDefault()
    if (!(title.trim() && startAt && endAt)) {
      return
    }
    const payload: CreateMaintenanceRequest | UpdateMaintenanceRequest = {
      title: title.trim(),
      description: description.trim() || null,
      start_at: new Date(startAt).toISOString(),
      end_at: new Date(endAt).toISOString(),
      active,
      server_ids_json: selectedServers,
      is_public: isPublic
    }
    onSubmit(payload, editing?.id)
  }

  const toggleServer = (id: string) => {
    setSelectedServers((prev) => (prev.includes(id) ? prev.filter((s) => s !== id) : [...prev, id]))
  }

  return (
    <Dialog onOpenChange={handleOpenChange} open={open}>
      <DialogContent className="sm:max-w-lg">
        <DialogHeader>
          <DialogTitle>{editing ? t('maintenance.edit') : t('maintenance.create')}</DialogTitle>
          <DialogDescription>
            {editing ? t('maintenance.edit_description') : t('maintenance.create_description')}
          </DialogDescription>
        </DialogHeader>
        <form className="space-y-4" id="maintenance-form" onSubmit={handleSubmit}>
          <div className="space-y-1">
            <Label htmlFor="mnt-title">{t('maintenance.field_title')}</Label>
            <Input
              id="mnt-title"
              onChange={(e) => setTitle(e.target.value)}
              placeholder={t('maintenance.placeholder_title')}
              required
              value={title}
            />
          </div>
          <div className="space-y-1">
            <Label htmlFor="mnt-desc">{t('maintenance.field_description')}</Label>
            <Textarea
              id="mnt-desc"
              onChange={(e) => setDescription(e.target.value)}
              placeholder={t('maintenance.placeholder_description')}
              rows={2}
              value={description}
            />
          </div>
          <div className="grid gap-3 sm:grid-cols-2">
            <div className="space-y-1">
              <Label htmlFor="mnt-start">{t('maintenance.field_start')}</Label>
              <Input
                id="mnt-start"
                onChange={(e) => setStartAt(e.target.value)}
                required
                type="datetime-local"
                value={startAt}
              />
            </div>
            <div className="space-y-1">
              <Label htmlFor="mnt-end">{t('maintenance.field_end')}</Label>
              <Input
                id="mnt-end"
                onChange={(e) => setEndAt(e.target.value)}
                required
                type="datetime-local"
                value={endAt}
              />
            </div>
          </div>
          <div className="flex items-center gap-2">
            {/* biome-ignore lint/a11y/noLabelWithoutControl: Switch renders as a labelable button element */}
            <label className="flex items-center gap-2 text-sm">
              <Switch checked={active} onCheckedChange={setActive} />
              {t('maintenance.field_active')}
            </label>
          </div>
          <div className="flex items-center justify-between gap-4 rounded-md border p-3">
            <div className="space-y-0.5">
              <Label htmlFor="mnt-public">{t('maintenance.field_is_public')}</Label>
              <p className="text-muted-foreground text-xs">{t('maintenance.field_is_public_hint')}</p>
            </div>
            <Switch checked={isPublic} id="mnt-public" onCheckedChange={setIsPublic} />
          </div>
          <div className="space-y-2">
            <Label>{t('maintenance.field_servers')}</Label>
            <ScrollArea className="h-32 rounded-md border">
              <div className="space-y-1 p-2">
                {servers.map((s) => (
                  <ServerCheckboxItem
                    checked={selectedServers.includes(s.id)}
                    key={s.id}
                    name={s.name}
                    onToggle={() => toggleServer(s.id)}
                  />
                ))}
              </div>
            </ScrollArea>
          </div>
        </form>
        <DialogFooter>
          <Button disabled={pending} form="maintenance-form" type="submit">
            {editing ? t('common:save') : t('common:create')}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}

function MaintenanceTab({ servers }: { servers: ServerResponse[] }) {
  const { t } = useTranslation('settings')
  const queryClient = useQueryClient()
  const [dialogOpen, setDialogOpen] = useState(false)
  const [editing, setEditing] = useState<MaintenanceItem | null>(null)

  const { data: maintenances, isLoading } = useQuery<MaintenanceItem[]>({
    queryKey: ['maintenances'],
    queryFn: () => api.get<MaintenanceItem[]>('/api/maintenances')
  })

  const invalidate = () => {
    queryClient.invalidateQueries({ queryKey: ['maintenances'] }).catch(() => undefined)
  }

  const createMutation = useMutation({
    mutationFn: (input: CreateMaintenanceRequest) => api.post('/api/maintenances', input),
    onSuccess: () => {
      invalidate()
      setDialogOpen(false)
      toast.success(t('maintenance.created'))
    },
    onError: (err) => toast.error(err instanceof Error ? err.message : t('common:errors.failed'))
  })

  const updateMutation = useMutation({
    mutationFn: ({ id, input }: { id: string; input: UpdateMaintenanceRequest }) =>
      api.put(`/api/maintenances/${id}`, input),
    onSuccess: () => {
      invalidate()
      setDialogOpen(false)
      setEditing(null)
      toast.success(t('maintenance.updated'))
    },
    onError: (err) => toast.error(err instanceof Error ? err.message : t('common:errors.failed'))
  })

  const deleteMutation = useMutation({
    mutationFn: (id: string) => api.delete(`/api/maintenances/${id}`),
    onSuccess: () => {
      invalidate()
      toast.success(t('maintenance.deleted'))
    },
    onError: (err) => toast.error(err instanceof Error ? err.message : t('common:errors.failed'))
  })

  const handleSubmit = (data: CreateMaintenanceRequest | UpdateMaintenanceRequest, id?: string) => {
    if (id) {
      updateMutation.mutate({ id, input: data as UpdateMaintenanceRequest })
    } else {
      createMutation.mutate(data as CreateMaintenanceRequest)
    }
  }

  return (
    <div>
      <div className="mb-4 flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
        <p className="text-muted-foreground text-sm">{t('maintenance.tab_description')}</p>
        <Button
          onClick={() => {
            setEditing(null)
            setDialogOpen(true)
          }}
          size="sm"
        >
          <Plus className="size-4" />
          {t('maintenance.create')}
        </Button>
      </div>

      {isLoading && (
        <div className="space-y-2">
          {Array.from({ length: 2 }, (_, i) => (
            <Skeleton className="h-12" key={`skel-${i.toString()}`} />
          ))}
        </div>
      )}

      {!isLoading && (!maintenances || maintenances.length === 0) && (
        <div className="rounded-lg border bg-card p-12 text-center">
          <p className="text-muted-foreground">{t('maintenance.empty')}</p>
        </div>
      )}

      {maintenances && maintenances.length > 0 && (
        <div className="rounded-lg border">
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>{t('maintenance.field_title')}</TableHead>
                <TableHead>{t('maintenance.field_start')}</TableHead>
                <TableHead>{t('maintenance.field_end')}</TableHead>
                <TableHead>{t('maintenance.field_active')}</TableHead>
                <TableHead>{t('maintenance.field_is_public')}</TableHead>
                <TableHead className="text-right">{t('status_pages.col_actions')}</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {maintenances.map((m) => (
                <TableRow key={m.id}>
                  <TableCell className="font-medium">{m.title}</TableCell>
                  <TableCell className="text-muted-foreground text-xs">
                    {new Date(m.start_at).toLocaleString()}
                  </TableCell>
                  <TableCell className="text-muted-foreground text-xs">{new Date(m.end_at).toLocaleString()}</TableCell>
                  <TableCell>
                    <Badge variant={m.active ? 'default' : 'secondary'}>
                      {m.active ? t('common:enable') : t('common:disable')}
                    </Badge>
                  </TableCell>
                  <TableCell>
                    <Badge variant={m.is_public ? 'default' : 'secondary'}>
                      {m.is_public ? t('maintenance.is_public_yes') : t('maintenance.is_public_no')}
                    </Badge>
                  </TableCell>
                  <TableCell className="text-right">
                    <div className="flex justify-end gap-1">
                      <Button
                        onClick={() => {
                          setEditing(m)
                          setDialogOpen(true)
                        }}
                        size="sm"
                        variant="ghost"
                      >
                        <Pencil className="size-3.5" />
                      </Button>
                      <Button
                        disabled={deleteMutation.isPending}
                        onClick={() => deleteMutation.mutate(m.id)}
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

      <MaintenanceFormDialog
        editing={editing}
        onClose={() => {
          setDialogOpen(false)
          setEditing(null)
        }}
        onSubmit={handleSubmit}
        open={dialogOpen}
        pending={createMutation.isPending || updateMutation.isPending}
        servers={servers}
      />
    </div>
  )
}

// ---------------------------------------------------------------------------
// Main Page
// ---------------------------------------------------------------------------

function StatusPagesManagement() {
  const { t } = useTranslation('settings')

  const { data: servers } = useQuery<ServerResponse[]>({
    queryKey: ['servers-list'],
    queryFn: () => api.get<ServerResponse[]>('/api/servers')
  })

  return (
    <div>
      <Tabs className="max-w-5xl" defaultValue="config">
        <TabsList>
          <TabsTrigger value="config">{t('status_pages.tab_config')}</TabsTrigger>
          <TabsTrigger value="incidents">{t('status_pages.tab_incidents')}</TabsTrigger>
          <TabsTrigger value="maintenance">{t('status_pages.tab_maintenance')}</TabsTrigger>
        </TabsList>

        <TabsContent value="config">
          <StatusPageConfigForm servers={servers ?? []} />
        </TabsContent>

        <TabsContent value="incidents">
          <IncidentsTab servers={servers ?? []} />
        </TabsContent>

        <TabsContent value="maintenance">
          <MaintenanceTab servers={servers ?? []} />
        </TabsContent>
      </Tabs>
    </div>
  )
}

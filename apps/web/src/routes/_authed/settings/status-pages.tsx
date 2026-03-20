import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { createFileRoute } from '@tanstack/react-router'
import { ExternalLink, Pencil, Plus, Trash2 } from 'lucide-react'
import { type FormEvent, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { toast } from 'sonner'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
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
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select'
import { Skeleton } from '@/components/ui/skeleton'
import { Switch } from '@/components/ui/switch'
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from '@/components/ui/table'
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs'
import { api } from '@/lib/api-client'
import type { IncidentItem, MaintenanceItem, ServerResponse, StatusPageItem } from '@/lib/api-schema'

export const Route = createFileRoute('/_authed/settings/status-pages')({
  component: StatusPagesManagement
})

// ---------------------------------------------------------------------------
// Shared server checkbox list
// ---------------------------------------------------------------------------

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
// Status Pages Tab
// ---------------------------------------------------------------------------

function StatusPageFormDialog({
  editing,
  onClose,
  onSubmit,
  open,
  pending,
  servers
}: {
  editing: StatusPageItem | null
  onClose: () => void
  onSubmit: (data: Record<string, unknown>, id?: string) => void
  open: boolean
  pending: boolean
  servers: ServerResponse[]
}) {
  const { t } = useTranslation('settings')
  const [title, setTitle] = useState('')
  const [slug, setSlug] = useState('')
  const [description, setDescription] = useState('')
  const [enabled, setEnabled] = useState(true)
  const [selectedServers, setSelectedServers] = useState<string[]>([])

  const handleOpenChange = (isOpen: boolean) => {
    if (isOpen && editing) {
      setTitle(editing.title)
      setSlug(editing.slug)
      setDescription(editing.description ?? '')
      setEnabled(editing.enabled)
      setSelectedServers(editing.server_ids ?? [])
    } else if (isOpen) {
      setTitle('')
      setSlug('')
      setDescription('')
      setEnabled(true)
      setSelectedServers([])
    }
    if (!isOpen) {
      onClose()
    }
  }

  const handleSubmit = (e: FormEvent) => {
    e.preventDefault()
    if (!(title.trim() && slug.trim())) {
      return
    }
    const payload: Record<string, unknown> = {
      title: title.trim(),
      slug: slug.trim(),
      description: description.trim() || null,
      enabled,
      server_ids: selectedServers
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
          <DialogTitle>{editing ? t('status_pages.edit_page') : t('status_pages.create_page')}</DialogTitle>
          <DialogDescription>
            {editing ? t('status_pages.edit_description') : t('status_pages.create_description')}
          </DialogDescription>
        </DialogHeader>
        <form className="space-y-4" id="status-page-form" onSubmit={handleSubmit}>
          <div className="space-y-1">
            <Label htmlFor="sp-title">{t('status_pages.field_title')}</Label>
            <Input
              id="sp-title"
              onChange={(e) => setTitle(e.target.value)}
              placeholder="My Status Page"
              required
              value={title}
            />
          </div>
          <div className="space-y-1">
            <Label htmlFor="sp-slug">{t('status_pages.field_slug')}</Label>
            <Input
              id="sp-slug"
              onChange={(e) => setSlug(e.target.value.toLowerCase().replace(/[^a-z0-9-]/g, '-'))}
              placeholder="my-services"
              required
              value={slug}
            />
            <p className="text-muted-foreground text-xs">{t('status_pages.slug_hint')}</p>
          </div>
          <div className="space-y-1">
            <Label htmlFor="sp-desc">{t('status_pages.field_description')}</Label>
            <textarea
              className="flex min-h-[60px] w-full rounded-md border border-input bg-transparent px-3 py-2 text-sm shadow-xs placeholder:text-muted-foreground focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring"
              id="sp-desc"
              onChange={(e) => setDescription(e.target.value)}
              placeholder="Optional description"
              rows={2}
              value={description}
            />
          </div>
          <div className="flex items-center gap-2">
            {/* biome-ignore lint/a11y/noLabelWithoutControl: Switch renders as a labelable button element */}
            <label className="flex items-center gap-2 text-sm">
              <Switch checked={enabled} onCheckedChange={setEnabled} />
              {t('status_pages.field_enabled')}
            </label>
          </div>
          <div className="space-y-2">
            <Label>{t('status_pages.field_servers')}</Label>
            <div className="max-h-40 space-y-1 overflow-y-auto rounded-md border p-2">
              {servers.map((s) => (
                <ServerCheckboxItem
                  checked={selectedServers.includes(s.id)}
                  key={s.id}
                  name={s.name}
                  onToggle={() => toggleServer(s.id)}
                />
              ))}
              {servers.length === 0 && <p className="text-muted-foreground text-xs">{t('status_pages.no_servers')}</p>}
            </div>
          </div>
        </form>
        <DialogFooter>
          <Button disabled={pending} form="status-page-form" type="submit">
            {editing ? t('common:save') : t('common:create')}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}

function StatusPagesTab({ servers }: { servers: ServerResponse[] }) {
  const { t } = useTranslation('settings')
  const queryClient = useQueryClient()
  const [dialogOpen, setDialogOpen] = useState(false)
  const [editing, setEditing] = useState<StatusPageItem | null>(null)

  const { data: pages, isLoading } = useQuery<StatusPageItem[]>({
    queryKey: ['status-pages'],
    queryFn: () => api.get<StatusPageItem[]>('/api/status-pages')
  })

  const invalidate = () => {
    queryClient.invalidateQueries({ queryKey: ['status-pages'] }).catch(() => undefined)
  }

  const createMutation = useMutation({
    mutationFn: (input: Record<string, unknown>) => api.post('/api/status-pages', input),
    onSuccess: () => {
      invalidate()
      setDialogOpen(false)
      toast.success(t('status_pages.created'))
    },
    onError: (err) => toast.error(err instanceof Error ? err.message : 'Failed')
  })

  const updateMutation = useMutation({
    mutationFn: ({ id, input }: { id: string; input: Record<string, unknown> }) =>
      api.put(`/api/status-pages/${id}`, input),
    onSuccess: () => {
      invalidate()
      setDialogOpen(false)
      setEditing(null)
      toast.success(t('status_pages.updated'))
    },
    onError: (err) => toast.error(err instanceof Error ? err.message : 'Failed')
  })

  const deleteMutation = useMutation({
    mutationFn: (id: string) => api.delete(`/api/status-pages/${id}`),
    onSuccess: () => {
      invalidate()
      toast.success(t('status_pages.deleted'))
    },
    onError: (err) => toast.error(err instanceof Error ? err.message : 'Failed')
  })

  const handleSubmit = (data: Record<string, unknown>, id?: string) => {
    if (id) {
      updateMutation.mutate({ id, input: data })
    } else {
      createMutation.mutate(data)
    }
  }

  return (
    <div>
      <div className="mb-4 flex items-center justify-between">
        <p className="text-muted-foreground text-sm">{t('status_pages.tab_description')}</p>
        <Button
          onClick={() => {
            setEditing(null)
            setDialogOpen(true)
          }}
          size="sm"
        >
          <Plus className="size-4" />
          {t('status_pages.create_page')}
        </Button>
      </div>

      {isLoading && (
        <div className="space-y-2">
          {Array.from({ length: 2 }, (_, i) => (
            <Skeleton className="h-12" key={`skel-${i.toString()}`} />
          ))}
        </div>
      )}

      {!isLoading && (!pages || pages.length === 0) && (
        <div className="rounded-lg border bg-card p-12 text-center">
          <p className="text-muted-foreground">{t('status_pages.empty')}</p>
        </div>
      )}

      {pages && pages.length > 0 && (
        <div className="rounded-lg border">
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>{t('status_pages.field_title')}</TableHead>
                <TableHead>{t('status_pages.field_slug')}</TableHead>
                <TableHead>{t('status_pages.field_enabled')}</TableHead>
                <TableHead>{t('status_pages.col_servers')}</TableHead>
                <TableHead className="text-right">{t('status_pages.col_actions')}</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {pages.map((page) => (
                <TableRow key={page.id}>
                  <TableCell className="font-medium">{page.title}</TableCell>
                  <TableCell>
                    <a
                      className="inline-flex items-center gap-1 font-mono text-primary text-xs hover:underline"
                      href={`/status/${page.slug}`}
                      rel="noopener noreferrer"
                      target="_blank"
                    >
                      /status/{page.slug}
                      <ExternalLink className="size-3" />
                    </a>
                  </TableCell>
                  <TableCell>
                    <Badge variant={page.enabled ? 'default' : 'secondary'}>
                      {page.enabled ? t('common:enable') : t('common:disable')}
                    </Badge>
                  </TableCell>
                  <TableCell className="text-muted-foreground text-xs">
                    {page.server_ids?.length ?? 0} {t('status_pages.col_servers').toLowerCase()}
                  </TableCell>
                  <TableCell className="text-right">
                    <div className="flex justify-end gap-1">
                      <Button
                        onClick={() => {
                          setEditing(page)
                          setDialogOpen(true)
                        }}
                        size="sm"
                        variant="ghost"
                      >
                        <Pencil className="size-3.5" />
                      </Button>
                      <Button
                        disabled={deleteMutation.isPending}
                        onClick={() => deleteMutation.mutate(page.id)}
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

      <StatusPageFormDialog
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
  onSubmit: (data: Record<string, unknown>, id?: string) => void
  open: boolean
  pending: boolean
  servers: ServerResponse[]
}) {
  const { t } = useTranslation('settings')
  const [title, setTitle] = useState('')
  const [severity, setSeverity] = useState<string>('minor')
  const [status, setStatus] = useState<string>('investigating')
  const [selectedServers, setSelectedServers] = useState<string[]>([])

  const handleOpenChange = (isOpen: boolean) => {
    if (isOpen && editing) {
      setTitle(editing.title)
      setSeverity(editing.severity)
      setStatus(editing.status)
      setSelectedServers(editing.server_ids ?? [])
    } else if (isOpen) {
      setTitle('')
      setSeverity('minor')
      setStatus('investigating')
      setSelectedServers([])
    }
    if (!isOpen) {
      onClose()
    }
  }

  const handleSubmit = (e: FormEvent) => {
    e.preventDefault()
    if (!title.trim()) {
      return
    }
    const payload: Record<string, unknown> = {
      title: title.trim(),
      severity,
      status,
      server_ids: selectedServers
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
              placeholder="Service disruption"
              required
              value={title}
            />
          </div>
          <div className="grid gap-3 sm:grid-cols-2">
            <div className="space-y-1">
              <Label htmlFor="inc-severity">{t('incidents.field_severity')}</Label>
              <Select onValueChange={setSeverity} value={severity}>
                <SelectTrigger id="inc-severity">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  {INCIDENT_SEVERITIES.map((s) => (
                    <SelectItem key={s} value={s}>
                      {s}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </div>
            <div className="space-y-1">
              <Label htmlFor="inc-status">{t('incidents.field_status')}</Label>
              <Select onValueChange={setStatus} value={status}>
                <SelectTrigger id="inc-status">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  {INCIDENT_STATUSES.map((s) => (
                    <SelectItem key={s} value={s}>
                      {s}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </div>
          </div>
          <div className="space-y-2">
            <Label>{t('incidents.field_servers')}</Label>
            <div className="max-h-32 space-y-1 overflow-y-auto rounded-md border p-2">
              {servers.map((s) => (
                <ServerCheckboxItem
                  checked={selectedServers.includes(s.id)}
                  key={s.id}
                  name={s.name}
                  onToggle={() => toggleServer(s.id)}
                />
              ))}
            </div>
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
    onError: (err) => toast.error(err instanceof Error ? err.message : 'Failed')
  })

  const handleSubmit = (e: FormEvent) => {
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
            <Select onValueChange={setStatus} value={status}>
              <SelectTrigger id="upd-status">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                {INCIDENT_STATUSES.map((s) => (
                  <SelectItem key={s} value={s}>
                    {s}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </div>
          <div className="space-y-1">
            <Label htmlFor="upd-message">{t('incidents.field_message')}</Label>
            <textarea
              className="flex min-h-[80px] w-full rounded-md border border-input bg-transparent px-3 py-2 text-sm shadow-xs placeholder:text-muted-foreground focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring"
              id="upd-message"
              onChange={(e) => setMessage(e.target.value)}
              placeholder="Describe the update..."
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
    mutationFn: (input: Record<string, unknown>) => api.post('/api/incidents', input),
    onSuccess: () => {
      invalidate()
      setDialogOpen(false)
      toast.success(t('incidents.created'))
    },
    onError: (err) => toast.error(err instanceof Error ? err.message : 'Failed')
  })

  const updateMutation = useMutation({
    mutationFn: ({ id, input }: { id: string; input: Record<string, unknown> }) =>
      api.put(`/api/incidents/${id}`, input),
    onSuccess: () => {
      invalidate()
      setDialogOpen(false)
      setEditing(null)
      toast.success(t('incidents.updated'))
    },
    onError: (err) => toast.error(err instanceof Error ? err.message : 'Failed')
  })

  const deleteMutation = useMutation({
    mutationFn: (id: string) => api.delete(`/api/incidents/${id}`),
    onSuccess: () => {
      invalidate()
      toast.success(t('incidents.deleted'))
    },
    onError: (err) => toast.error(err instanceof Error ? err.message : 'Failed')
  })

  const handleSubmit = (data: Record<string, unknown>, id?: string) => {
    if (id) {
      updateMutation.mutate({ id, input: data })
    } else {
      createMutation.mutate(data)
    }
  }

  return (
    <div>
      <div className="mb-4 flex items-center justify-between">
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
  onSubmit: (data: Record<string, unknown>, id?: string) => void
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

  const handleOpenChange = (isOpen: boolean) => {
    if (isOpen && editing) {
      setTitle(editing.title)
      setDescription(editing.description ?? '')
      setStartAt(editing.start_at.slice(0, 16))
      setEndAt(editing.end_at.slice(0, 16))
      setActive(editing.active)
      setSelectedServers(editing.server_ids ?? [])
    } else if (isOpen) {
      setTitle('')
      setDescription('')
      setStartAt('')
      setEndAt('')
      setActive(true)
      setSelectedServers([])
    }
    if (!isOpen) {
      onClose()
    }
  }

  const handleSubmit = (e: FormEvent) => {
    e.preventDefault()
    if (!(title.trim() && startAt && endAt)) {
      return
    }
    const payload: Record<string, unknown> = {
      title: title.trim(),
      description: description.trim() || null,
      start_at: new Date(startAt).toISOString(),
      end_at: new Date(endAt).toISOString(),
      active,
      server_ids: selectedServers
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
              placeholder="Scheduled server maintenance"
              required
              value={title}
            />
          </div>
          <div className="space-y-1">
            <Label htmlFor="mnt-desc">{t('maintenance.field_description')}</Label>
            <textarea
              className="flex min-h-[60px] w-full rounded-md border border-input bg-transparent px-3 py-2 text-sm shadow-xs placeholder:text-muted-foreground focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring"
              id="mnt-desc"
              onChange={(e) => setDescription(e.target.value)}
              placeholder="Optional details"
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
          <div className="space-y-2">
            <Label>{t('maintenance.field_servers')}</Label>
            <div className="max-h-32 space-y-1 overflow-y-auto rounded-md border p-2">
              {servers.map((s) => (
                <ServerCheckboxItem
                  checked={selectedServers.includes(s.id)}
                  key={s.id}
                  name={s.name}
                  onToggle={() => toggleServer(s.id)}
                />
              ))}
            </div>
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
    mutationFn: (input: Record<string, unknown>) => api.post('/api/maintenances', input),
    onSuccess: () => {
      invalidate()
      setDialogOpen(false)
      toast.success(t('maintenance.created'))
    },
    onError: (err) => toast.error(err instanceof Error ? err.message : 'Failed')
  })

  const updateMutation = useMutation({
    mutationFn: ({ id, input }: { id: string; input: Record<string, unknown> }) =>
      api.put(`/api/maintenances/${id}`, input),
    onSuccess: () => {
      invalidate()
      setDialogOpen(false)
      setEditing(null)
      toast.success(t('maintenance.updated'))
    },
    onError: (err) => toast.error(err instanceof Error ? err.message : 'Failed')
  })

  const deleteMutation = useMutation({
    mutationFn: (id: string) => api.delete(`/api/maintenances/${id}`),
    onSuccess: () => {
      invalidate()
      toast.success(t('maintenance.deleted'))
    },
    onError: (err) => toast.error(err instanceof Error ? err.message : 'Failed')
  })

  const handleSubmit = (data: Record<string, unknown>, id?: string) => {
    if (id) {
      updateMutation.mutate({ id, input: data })
    } else {
      createMutation.mutate(data)
    }
  }

  return (
    <div>
      <div className="mb-4 flex items-center justify-between">
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
      <h1 className="mb-6 font-bold text-2xl">{t('status_pages.title')}</h1>

      <Tabs className="max-w-5xl" defaultValue="pages">
        <TabsList>
          <TabsTrigger value="pages">{t('status_pages.tab_pages')}</TabsTrigger>
          <TabsTrigger value="incidents">{t('status_pages.tab_incidents')}</TabsTrigger>
          <TabsTrigger value="maintenance">{t('status_pages.tab_maintenance')}</TabsTrigger>
        </TabsList>

        <TabsContent value="pages">
          <StatusPagesTab servers={servers ?? []} />
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

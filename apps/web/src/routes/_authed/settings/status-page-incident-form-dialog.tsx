import { type FormEvent, useState } from 'react'
import { useTranslation } from 'react-i18next'
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
import { ScrollArea } from '@/components/ui/scroll-area'
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select'
import { Switch } from '@/components/ui/switch'
import type { CreateIncidentRequest, IncidentItem, ServerResponse, UpdateIncidentRequest } from '@/lib/api-schema'
import { parseServerIds } from './status-page-config-utils'
import { INCIDENT_SEVERITIES, INCIDENT_STATUSES } from './status-page-incident-options'
import { StatusPageServerCheckboxItem } from './status-page-server-checkbox-item'

export function StatusPageIncidentFormDialog({
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
    setSelectedServers((prev) => (prev.includes(id) ? prev.filter((serverId) => serverId !== id) : [...prev, id]))
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
              <Select onValueChange={(value) => value && setSeverity(value)} value={severity}>
                <SelectTrigger id="inc-severity">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  {INCIDENT_SEVERITIES.map((severityValue) => (
                    <SelectItem key={severityValue} value={severityValue}>
                      {t(`incidents.severity_${severityValue}`)}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </div>
            <div className="space-y-1">
              <Label htmlFor="inc-status">{t('incidents.field_status')}</Label>
              <Select onValueChange={(value) => value && setStatus(value)} value={status}>
                <SelectTrigger id="inc-status">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  {INCIDENT_STATUSES.map((statusValue) => (
                    <SelectItem key={statusValue} value={statusValue}>
                      {t(`incidents.status_${statusValue}`)}
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
                {servers.map((server) => (
                  <StatusPageServerCheckboxItem
                    checked={selectedServers.includes(server.id)}
                    key={server.id}
                    name={server.name}
                    onToggle={() => toggleServer(server.id)}
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

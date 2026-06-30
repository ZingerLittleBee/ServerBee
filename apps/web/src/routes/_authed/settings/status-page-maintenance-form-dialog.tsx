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
import { Switch } from '@/components/ui/switch'
import { Textarea } from '@/components/ui/textarea'
import type {
  CreateMaintenanceRequest,
  MaintenanceItem,
  ServerResponse,
  UpdateMaintenanceRequest
} from '@/lib/api-schema'
import { parseServerIds } from './status-page-config-utils'
import { StatusPageServerCheckboxItem } from './status-page-server-checkbox-item'

export function StatusPageMaintenanceFormDialog({
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
    setSelectedServers((prev) => (prev.includes(id) ? prev.filter((serverId) => serverId !== id) : [...prev, id]))
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
          <Button disabled={pending} form="maintenance-form" type="submit">
            {editing ? t('common:save') : t('common:create')}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}

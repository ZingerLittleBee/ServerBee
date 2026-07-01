import { type FormEvent, useReducer } from 'react'
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

interface MaintenanceFormState {
  active: boolean
  description: string
  endAt: string
  isPublic: boolean
  selectedServers: string[]
  startAt: string
  title: string
}

type MaintenanceFormAction =
  | { type: 'reset'; value: MaintenanceFormState }
  | { type: 'setActive'; value: boolean }
  | { type: 'setDescription'; value: string }
  | { type: 'setEndAt'; value: string }
  | { type: 'setIsPublic'; value: boolean }
  | { type: 'setStartAt'; value: string }
  | { type: 'setTitle'; value: string }
  | { type: 'toggleServer'; id: string }

const EMPTY_MAINTENANCE_FORM: MaintenanceFormState = {
  active: true,
  description: '',
  endAt: '',
  isPublic: false,
  selectedServers: [],
  startAt: '',
  title: ''
}

function maintenanceFormFromItem(item: MaintenanceItem | null): MaintenanceFormState {
  if (!item) {
    return EMPTY_MAINTENANCE_FORM
  }
  return {
    active: item.active,
    description: item.description ?? '',
    endAt: item.end_at.slice(0, 16),
    isPublic: item.is_public,
    selectedServers: parseServerIds(item.server_ids_json),
    startAt: item.start_at.slice(0, 16),
    title: item.title
  }
}

function maintenanceFormReducer(state: MaintenanceFormState, action: MaintenanceFormAction): MaintenanceFormState {
  switch (action.type) {
    case 'reset':
      return action.value
    case 'setActive':
      return { ...state, active: action.value }
    case 'setDescription':
      return { ...state, description: action.value }
    case 'setEndAt':
      return { ...state, endAt: action.value }
    case 'setIsPublic':
      return { ...state, isPublic: action.value }
    case 'setStartAt':
      return { ...state, startAt: action.value }
    case 'setTitle':
      return { ...state, title: action.value }
    case 'toggleServer':
      return {
        ...state,
        selectedServers: state.selectedServers.includes(action.id)
          ? state.selectedServers.filter((serverId) => serverId !== action.id)
          : [...state.selectedServers, action.id]
      }
    default:
      return state
  }
}

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
  const [state, dispatch] = useReducer(maintenanceFormReducer, EMPTY_MAINTENANCE_FORM)

  const handleOpenChange = (isOpen: boolean) => {
    if (isOpen) {
      dispatch({ type: 'reset', value: maintenanceFormFromItem(editing) })
    }
    if (!isOpen) {
      onClose()
    }
  }

  const handleSubmit = (e: FormEvent<HTMLFormElement>) => {
    e.preventDefault()
    if (!(state.title.trim() && state.startAt && state.endAt)) {
      return
    }
    const payload: CreateMaintenanceRequest | UpdateMaintenanceRequest = {
      title: state.title.trim(),
      description: state.description.trim() || null,
      start_at: new Date(state.startAt).toISOString(),
      end_at: new Date(state.endAt).toISOString(),
      active: state.active,
      server_ids_json: state.selectedServers,
      is_public: state.isPublic
    }
    onSubmit(payload, editing?.id)
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
              onChange={(e) => dispatch({ type: 'setTitle', value: e.target.value })}
              placeholder={t('maintenance.placeholder_title')}
              required
              value={state.title}
            />
          </div>
          <div className="space-y-1">
            <Label htmlFor="mnt-desc">{t('maintenance.field_description')}</Label>
            <Textarea
              id="mnt-desc"
              onChange={(e) => dispatch({ type: 'setDescription', value: e.target.value })}
              placeholder={t('maintenance.placeholder_description')}
              rows={2}
              value={state.description}
            />
          </div>
          <div className="grid gap-3 sm:grid-cols-2">
            <div className="space-y-1">
              <Label htmlFor="mnt-start">{t('maintenance.field_start')}</Label>
              <Input
                id="mnt-start"
                onChange={(e) => dispatch({ type: 'setStartAt', value: e.target.value })}
                required
                type="datetime-local"
                value={state.startAt}
              />
            </div>
            <div className="space-y-1">
              <Label htmlFor="mnt-end">{t('maintenance.field_end')}</Label>
              <Input
                id="mnt-end"
                onChange={(e) => dispatch({ type: 'setEndAt', value: e.target.value })}
                required
                type="datetime-local"
                value={state.endAt}
              />
            </div>
          </div>
          <div className="flex items-center gap-2">
            {/* biome-ignore lint/a11y/noLabelWithoutControl: Switch renders as a labelable button element */}
            <label className="flex items-center gap-2 text-sm">
              <Switch checked={state.active} onCheckedChange={(value) => dispatch({ type: 'setActive', value })} />
              {t('maintenance.field_active')}
            </label>
          </div>
          <div className="flex items-center justify-between gap-4 rounded-md border p-3">
            <div className="space-y-0.5">
              <Label htmlFor="mnt-public">{t('maintenance.field_is_public')}</Label>
              <p className="text-muted-foreground text-xs">{t('maintenance.field_is_public_hint')}</p>
            </div>
            <Switch
              checked={state.isPublic}
              id="mnt-public"
              onCheckedChange={(value) => dispatch({ type: 'setIsPublic', value })}
            />
          </div>
          <div className="space-y-2">
            <Label>{t('maintenance.field_servers')}</Label>
            <ScrollArea className="h-32 rounded-md border">
              <div className="space-y-1 p-2">
                {servers.map((server) => (
                  <StatusPageServerCheckboxItem
                    checked={state.selectedServers.includes(server.id)}
                    key={server.id}
                    name={server.name}
                    onToggle={() => dispatch({ type: 'toggleServer', id: server.id })}
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

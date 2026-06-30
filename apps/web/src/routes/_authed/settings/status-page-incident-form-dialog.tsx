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
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select'
import { Switch } from '@/components/ui/switch'
import type { CreateIncidentRequest, IncidentItem, ServerResponse, UpdateIncidentRequest } from '@/lib/api-schema'
import { parseServerIds } from './status-page-config-utils'
import { INCIDENT_SEVERITIES, INCIDENT_STATUSES } from './status-page-incident-options'
import { StatusPageServerCheckboxItem } from './status-page-server-checkbox-item'

interface IncidentFormState {
  isPublic: boolean
  selectedServers: string[]
  severity: string
  status: string
  title: string
}

type IncidentFormAction =
  | { type: 'reset'; value: IncidentFormState }
  | { type: 'setIsPublic'; value: boolean }
  | { type: 'setSeverity'; value: string }
  | { type: 'setStatus'; value: string }
  | { type: 'setTitle'; value: string }
  | { type: 'toggleServer'; id: string }

const EMPTY_INCIDENT_FORM: IncidentFormState = {
  isPublic: false,
  selectedServers: [],
  severity: 'minor',
  status: 'investigating',
  title: ''
}

function incidentFormFromItem(item: IncidentItem | null): IncidentFormState {
  if (!item) {
    return EMPTY_INCIDENT_FORM
  }
  return {
    isPublic: item.is_public,
    selectedServers: parseServerIds(item.server_ids_json),
    severity: item.severity,
    status: item.status,
    title: item.title
  }
}

function incidentFormReducer(state: IncidentFormState, action: IncidentFormAction): IncidentFormState {
  switch (action.type) {
    case 'reset':
      return action.value
    case 'setIsPublic':
      return { ...state, isPublic: action.value }
    case 'setSeverity':
      return { ...state, severity: action.value }
    case 'setStatus':
      return { ...state, status: action.value }
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
  const [state, dispatch] = useReducer(incidentFormReducer, EMPTY_INCIDENT_FORM)

  const handleOpenChange = (isOpen: boolean) => {
    if (isOpen) {
      dispatch({ type: 'reset', value: incidentFormFromItem(editing) })
    }
    if (!isOpen) {
      onClose()
    }
  }

  const handleSubmit = (e: FormEvent<HTMLFormElement>) => {
    e.preventDefault()
    if (!state.title.trim()) {
      return
    }
    const payload: CreateIncidentRequest | UpdateIncidentRequest = {
      title: state.title.trim(),
      severity: state.severity,
      status: state.status,
      server_ids_json: state.selectedServers,
      is_public: state.isPublic
    }
    onSubmit(payload, editing?.id)
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
              onChange={(e) => dispatch({ type: 'setTitle', value: e.target.value })}
              placeholder={t('incidents.placeholder_title')}
              required
              value={state.title}
            />
          </div>
          <div className="grid gap-3 sm:grid-cols-2">
            <div className="space-y-1">
              <Label htmlFor="inc-severity">{t('incidents.field_severity')}</Label>
              <Select
                onValueChange={(value) => value && dispatch({ type: 'setSeverity', value })}
                value={state.severity}
              >
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
              <Select onValueChange={(value) => value && dispatch({ type: 'setStatus', value })} value={state.status}>
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
            <Switch
              checked={state.isPublic}
              id="inc-public"
              onCheckedChange={(value) => dispatch({ type: 'setIsPublic', value })}
            />
          </div>
          <div className="space-y-2">
            <Label>{t('incidents.field_servers')}</Label>
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
          <Button disabled={pending} form="incident-form" type="submit">
            {editing ? t('common:save') : t('common:create')}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}

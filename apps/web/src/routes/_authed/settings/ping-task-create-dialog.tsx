import { useMutation, useQueryClient } from '@tanstack/react-query'
import { Plus } from 'lucide-react'
import { type FormEvent, useReducer } from 'react'
import { useTranslation } from 'react-i18next'
import { toast } from 'sonner'
import { Button } from '@/components/ui/button'
import { Checkbox } from '@/components/ui/checkbox'
import {
  Dialog,
  DialogBody,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  DialogTrigger
} from '@/components/ui/dialog'
import { Input } from '@/components/ui/input'
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select'
import { api } from '@/lib/api-client'
import type { PingTask } from '@/lib/api-schema'
import type { ProbeType, Server } from './ping-task-types'

interface PingTaskFormState {
  interval: number
  name: string
  probeType: ProbeType
  selectedServerIds: string[]
  showForm: boolean
  target: string
}

type PingTaskFormAction =
  | { type: 'patch'; value: Partial<PingTaskFormState> }
  | { type: 'reset' }
  | { type: 'setAllServers'; serverIds: string[] }
  | { type: 'setShowForm'; value: boolean }
  | { type: 'toggleServer'; checked: boolean; id: string }

const INITIAL_PING_TASK_FORM: PingTaskFormState = {
  interval: 60,
  name: '',
  probeType: 'icmp',
  selectedServerIds: [],
  showForm: false,
  target: ''
}

function pingTaskFormReducer(state: PingTaskFormState, action: PingTaskFormAction): PingTaskFormState {
  switch (action.type) {
    case 'patch':
      return { ...state, ...action.value }
    case 'reset':
      return INITIAL_PING_TASK_FORM
    case 'setAllServers':
      return {
        ...state,
        selectedServerIds: state.selectedServerIds.length === action.serverIds.length ? [] : action.serverIds
      }
    case 'setShowForm':
      return action.value ? { ...state, showForm: true } : INITIAL_PING_TASK_FORM
    case 'toggleServer':
      return {
        ...state,
        selectedServerIds: action.checked
          ? [...state.selectedServerIds, action.id]
          : state.selectedServerIds.filter((serverId) => serverId !== action.id)
      }
    default:
      return state
  }
}

export function PingTaskCreateDialog({ servers }: { servers: Server[] }) {
  const { t } = useTranslation(['settings', 'common'])
  const queryClient = useQueryClient()
  const [state, dispatch] = useReducer(pingTaskFormReducer, INITIAL_PING_TASK_FORM)

  const probeTypeLabels: Record<ProbeType, string> = {
    icmp: t('ping.type_icmp'),
    tcp: t('ping.type_tcp'),
    http: t('ping.type_http')
  }
  const targetPlaceholder: Record<ProbeType, string> = {
    icmp: t('ping.placeholder_icmp'),
    tcp: t('ping.placeholder_tcp'),
    http: t('ping.placeholder_http')
  }

  const createMutation = useMutation({
    mutationFn: (input: {
      enabled: boolean
      interval: number
      name: string
      probe_type: string
      server_ids: string[]
      target: string
    }) => api.post<PingTask>('/api/ping-tasks', input),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['ping-tasks'] }).catch(() => undefined)
      dispatch({ type: 'reset' })
    }
  })

  const handleCreate = (e: FormEvent) => {
    e.preventDefault()
    if (state.name.trim().length === 0 || state.target.trim().length === 0) {
      return
    }
    if (state.selectedServerIds.length === 0) {
      toast.error(t('ping.no_servers_selected'))
      return
    }
    createMutation.mutate(
      {
        name: state.name.trim(),
        probe_type: state.probeType,
        target: state.target.trim(),
        interval: state.interval,
        server_ids: state.selectedServerIds,
        enabled: true
      },
      {
        onSuccess: () => {
          toast.success(t('ping.task_created', { defaultValue: 'Ping task created' }))
        },
        onError: (err) => {
          toast.error(
            err instanceof Error
              ? err.message
              : t('ping.task_create_failed', { defaultValue: 'Failed to create ping task' })
          )
        }
      }
    )
  }

  return (
    <Dialog onOpenChange={(open) => dispatch({ type: 'setShowForm', value: open })} open={state.showForm}>
      <DialogTrigger render={<Button size="sm" variant="outline" />}>
        <Plus className="size-4" />
        {t('common:add')}
      </DialogTrigger>
      <DialogContent className="sm:max-w-md">
        <DialogHeader>
          <DialogTitle>{t('ping.add_title')}</DialogTitle>
          <DialogDescription>{t('ping.add_description')}</DialogDescription>
        </DialogHeader>
        <DialogBody>
          <form className="space-y-3" id="create-ping-task-form" onSubmit={handleCreate}>
            <Input
              onChange={(e) => dispatch({ type: 'patch', value: { name: e.target.value } })}
              placeholder={t('ping.task_name')}
              required
              type="text"
              value={state.name}
            />

            <div className="flex flex-col gap-3 sm:flex-row">
              <Select
                items={probeTypeLabels}
                onValueChange={(value) => dispatch({ type: 'patch', value: { probeType: value as ProbeType } })}
                value={state.probeType}
              >
                <SelectTrigger className="w-full flex-1">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  {Object.entries(probeTypeLabels).map(([value, label]) => (
                    <SelectItem key={value} value={value}>
                      {label}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>

              <Input
                className="w-24"
                min={5}
                onChange={(e) =>
                  dispatch({ type: 'patch', value: { interval: Number.parseInt(e.target.value, 10) || 60 } })
                }
                placeholder={t('ping.interval')}
                type="number"
                value={state.interval}
              />
              <span className="flex items-center text-muted-foreground text-sm">sec</span>
            </div>

            <Input
              onChange={(e) => dispatch({ type: 'patch', value: { target: e.target.value } })}
              placeholder={targetPlaceholder[state.probeType]}
              required
              type="text"
              value={state.target}
            />

            {servers.length > 0 && (
              <fieldset className="space-y-2">
                <div className="flex items-center justify-between gap-2">
                  <legend className="text-sm">{t('ping.run_from_servers')}</legend>
                  <button
                    className="text-primary text-xs hover:underline focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
                    onClick={() => dispatch({ type: 'setAllServers', serverIds: servers.map((server) => server.id) })}
                    type="button"
                  >
                    {state.selectedServerIds.length === servers.length ? t('ping.deselect_all') : t('ping.select_all')}
                  </button>
                </div>
                <div className="space-y-1 rounded-md border p-2">
                  {servers.map((server) => (
                    // biome-ignore lint/a11y/noLabelWithoutControl: Checkbox renders as a labelable button element
                    <label className="flex items-center gap-2 text-sm" key={server.id}>
                      <Checkbox
                        checked={state.selectedServerIds.includes(server.id)}
                        onCheckedChange={(checked) =>
                          dispatch({ type: 'toggleServer', checked: checked === true, id: server.id })
                        }
                      />
                      {server.name}
                    </label>
                  ))}
                </div>
              </fieldset>
            )}
          </form>
        </DialogBody>
        <DialogFooter>
          <Button onClick={() => dispatch({ type: 'reset' })} size="sm" type="button" variant="ghost">
            {t('common:cancel')}
          </Button>
          <Button disabled={createMutation.isPending} form="create-ping-task-form" size="sm" type="submit">
            {t('common:create')}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}

import { useQuery } from '@tanstack/react-query'
import { useReducer } from 'react'
import { useTranslation } from 'react-i18next'
import { toast } from 'sonner'
import { Button } from '@/components/ui/button'
import { Checkbox } from '@/components/ui/checkbox'
import { Input } from '@/components/ui/input'
import { ScrollArea } from '@/components/ui/scroll-area'
import {
  type CreateScheduledTaskInput,
  type ScheduledTask,
  type UpdateScheduledTaskInput,
  useCreateScheduledTask,
  useUpdateScheduledTask
} from '@/hooks/use-scheduled-tasks'
import { api } from '@/lib/api-client'
import { CAP_EXEC, getEffectiveCapabilityEnabled } from '@/lib/capabilities'

const CRON_SPLIT_RE = /\s+/

interface Props {
  onClose: () => void
  task?: ScheduledTask | null
}

interface ServerInfo {
  capabilities?: number
  effective_capabilities?: number | null
  id: string
  name: string
}

interface ScheduledTaskFormState {
  command: string
  cronError: string
  cronExpression: string
  name: string
  retryCount: number
  retryInterval: number
  selectedServerIds: string[]
  timeout: number
}

type ScheduledTaskFormAction =
  | { type: 'setCommand'; value: string }
  | { type: 'setCronExpression'; error: string; value: string }
  | { type: 'setName'; value: string }
  | { type: 'setRetryCount'; value: number }
  | { type: 'setRetryInterval'; value: number }
  | { type: 'setSelectedServerIds'; value: string[] }
  | { type: 'setTimeout'; value: number }
  | { type: 'toggleServer'; id: string }

function scheduledTaskFormFromTask(task?: ScheduledTask | null): ScheduledTaskFormState {
  return {
    command: task?.command ?? '',
    cronError: '',
    cronExpression: task?.cron_expression ?? '',
    name: task?.name ?? '',
    retryCount: task?.retry_count ?? 0,
    retryInterval: task?.retry_interval ?? 60,
    selectedServerIds: task?.server_ids ?? [],
    timeout: task?.timeout ?? 300
  }
}

function scheduledTaskFormReducer(
  state: ScheduledTaskFormState,
  action: ScheduledTaskFormAction
): ScheduledTaskFormState {
  switch (action.type) {
    case 'setCommand':
      return { ...state, command: action.value }
    case 'setCronExpression':
      return { ...state, cronError: action.error, cronExpression: action.value }
    case 'setName':
      return { ...state, name: action.value }
    case 'setRetryCount':
      return { ...state, retryCount: action.value }
    case 'setRetryInterval':
      return { ...state, retryInterval: action.value }
    case 'setSelectedServerIds':
      return { ...state, selectedServerIds: action.value }
    case 'setTimeout':
      return { ...state, timeout: action.value }
    case 'toggleServer':
      return {
        ...state,
        selectedServerIds: state.selectedServerIds.includes(action.id)
          ? state.selectedServerIds.filter((serverId) => serverId !== action.id)
          : [...state.selectedServerIds, action.id]
      }
    default:
      return state
  }
}

export function ScheduledTaskDialog({ onClose, task }: Props) {
  const { t } = useTranslation(['settings', 'common'])
  const [state, dispatch] = useReducer(scheduledTaskFormReducer, task, scheduledTaskFormFromTask)

  const { data: servers } = useQuery<ServerInfo[]>({
    queryKey: ['servers-list'],
    queryFn: () => api.get<ServerInfo[]>('/api/servers')
  })

  const createMutation = useCreateScheduledTask()
  const updateMutation = useUpdateScheduledTask()

  const isEdit = !!task

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault()

    if (
      !(state.name.trim() && state.cronExpression.trim() && state.command.trim()) ||
      state.selectedServerIds.length === 0
    ) {
      toast.error(t('tasks.fill_required'))
      return
    }

    if (isEdit) {
      const input: UpdateScheduledTaskInput & { id: string } = {
        id: task.id,
        name: state.name.trim(),
        cron_expression: state.cronExpression.trim(),
        command: state.command.trim(),
        server_ids: state.selectedServerIds,
        timeout: state.timeout,
        retry_count: state.retryCount,
        retry_interval: state.retryInterval
      }
      updateMutation.mutate(input, { onSuccess: onClose })
    } else {
      const input: CreateScheduledTaskInput = {
        name: state.name.trim(),
        cron_expression: state.cronExpression.trim(),
        command: state.command.trim(),
        server_ids: state.selectedServerIds,
        timeout: state.timeout,
        retry_count: state.retryCount,
        retry_interval: state.retryInterval
      }
      createMutation.mutate(input, { onSuccess: onClose })
    }
  }

  const toggleServer = (id: string) => {
    dispatch({ type: 'toggleServer', id })
  }

  const selectAll = () => {
    if (!servers) {
      return
    }
    const execEnabled = servers.filter((server) =>
      getEffectiveCapabilityEnabled(server.effective_capabilities, server.capabilities, CAP_EXEC)
    )
    dispatch({
      type: 'setSelectedServerIds',
      value: state.selectedServerIds.length === execEnabled.length ? [] : execEnabled.map((server) => server.id)
    })
  }

  const handleCronChange = (value: string) => {
    let error = ''
    if (value.trim()) {
      // Basic validation: must have 5-7 space-separated parts
      const parts = value.trim().split(CRON_SPLIT_RE)
      if (parts.length < 5 || parts.length > 7) {
        error = t('tasks.scheduled.invalid_cron', { defaultValue: 'Invalid cron expression' })
      }
    }
    dispatch({ type: 'setCronExpression', value, error })
  }

  const isPending = createMutation.isPending || updateMutation.isPending

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50">
      <div className="w-full max-w-lg rounded-lg border bg-background p-6 shadow-lg">
        <h2 className="mb-4 font-semibold text-lg">
          {isEdit
            ? t('tasks.scheduled.edit', { defaultValue: 'Edit Scheduled Task' })
            : t('tasks.scheduled.create', { defaultValue: 'Create Scheduled Task' })}
        </h2>

        <form className="space-y-4" onSubmit={handleSubmit}>
          <div>
            <label className="mb-1 block font-medium text-sm" htmlFor="task-name">
              {t('tasks.scheduled.name', { defaultValue: 'Name' })}
            </label>
            <Input
              id="task-name"
              onChange={(e) => dispatch({ type: 'setName', value: e.target.value })}
              placeholder={t('tasks.scheduled.name_placeholder', { defaultValue: 'e.g. Daily Backup' })}
              required
              value={state.name}
            />
          </div>

          <div>
            <label className="mb-1 block font-medium text-sm" htmlFor="task-cron">
              {t('tasks.scheduled.cron', { defaultValue: 'Cron Expression' })}
            </label>
            <Input
              className="font-mono"
              id="task-cron"
              onChange={(e) => handleCronChange(e.target.value)}
              placeholder="0 0 * * * *"
              required
              value={state.cronExpression}
            />
            {state.cronError && <p className="mt-1 text-red-500 text-xs">{state.cronError}</p>}
            <p className="mt-1 text-muted-foreground text-xs">
              {t('tasks.scheduled.cron_help', {
                defaultValue: 'sec min hour day month weekday (e.g. 0 0 2 * * * = daily at 2:00 AM)'
              })}
            </p>
          </div>

          <div>
            <label className="mb-1 block font-medium text-sm" htmlFor="task-command">
              {t('tasks.command')}
            </label>
            <textarea
              className="w-full rounded-md border bg-background px-3 py-2 font-mono text-sm"
              id="task-command"
              onChange={(e) => dispatch({ type: 'setCommand', value: e.target.value })}
              placeholder={t('tasks.command_placeholder')}
              required
              rows={3}
              value={state.command}
            />
          </div>

          <div className="grid gap-3 sm:grid-cols-3">
            <div>
              <label className="mb-1 block font-medium text-sm" htmlFor="task-timeout">
                {t('tasks.timeout')}
              </label>
              <Input
                id="task-timeout"
                min={1}
                onChange={(e) => dispatch({ type: 'setTimeout', value: Number.parseInt(e.target.value, 10) || 300 })}
                type="number"
                value={state.timeout}
              />
            </div>
            <div>
              <label className="mb-1 block font-medium text-sm" htmlFor="task-retry">
                {t('tasks.scheduled.retry_count', { defaultValue: 'Retries' })}
              </label>
              <Input
                id="task-retry"
                max={10}
                min={0}
                onChange={(e) => dispatch({ type: 'setRetryCount', value: Number.parseInt(e.target.value, 10) || 0 })}
                type="number"
                value={state.retryCount}
              />
            </div>
            {state.retryCount > 0 && (
              <div>
                <label className="mb-1 block font-medium text-sm" htmlFor="task-retry-interval">
                  {t('tasks.scheduled.retry_interval', { defaultValue: 'Retry Interval (s)' })}
                </label>
                <Input
                  id="task-retry-interval"
                  min={1}
                  onChange={(e) =>
                    dispatch({ type: 'setRetryInterval', value: Number.parseInt(e.target.value, 10) || 60 })
                  }
                  type="number"
                  value={state.retryInterval}
                />
              </div>
            )}
          </div>

          <div>
            <div className="mb-2 flex items-center justify-between">
              <span className="font-medium text-sm">{t('tasks.target_servers')}</span>
              <Button onClick={selectAll} size="sm" type="button" variant="ghost">
                {t('tasks.select_all')}
              </Button>
            </div>
            {!servers || servers.length === 0 ? (
              <p className="text-muted-foreground text-sm">{t('tasks.no_servers')}</p>
            ) : (
              <ScrollArea className="max-h-40">
                <div className="grid grid-cols-2 gap-1">
                  {servers.map((srv) => {
                    const execEnabled = getEffectiveCapabilityEnabled(
                      srv.effective_capabilities,
                      srv.capabilities,
                      CAP_EXEC
                    )
                    return (
                      // biome-ignore lint/a11y/noLabelWithoutControl: Checkbox is labelable
                      <label
                        className={`flex cursor-pointer items-center gap-2 rounded-md border px-3 py-2 text-sm transition-colors has-[:checked]:border-primary has-[:checked]:bg-primary/5 ${
                          execEnabled ? '' : 'cursor-not-allowed opacity-50'
                        }`}
                        key={srv.id}
                      >
                        <Checkbox
                          checked={state.selectedServerIds.includes(srv.id)}
                          disabled={!execEnabled}
                          onCheckedChange={() => toggleServer(srv.id)}
                        />
                        {srv.name}
                      </label>
                    )
                  })}
                </div>
              </ScrollArea>
            )}
          </div>

          <div className="flex justify-end gap-2 pt-2">
            <Button onClick={onClose} type="button" variant="outline">
              {t('common:cancel', { defaultValue: 'Cancel' })}
            </Button>
            <Button
              disabled={
                isPending ||
                !state.name.trim() ||
                !state.cronExpression.trim() ||
                !state.command.trim() ||
                state.selectedServerIds.length === 0
              }
              type="submit"
            >
              {isEdit
                ? t('common:save', { defaultValue: 'Save' })
                : t('tasks.scheduled.create', { defaultValue: 'Create' })}
            </Button>
          </div>
        </form>
      </div>
    </div>
  )
}

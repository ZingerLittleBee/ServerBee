import { useQuery } from '@tanstack/react-query'
import { useState } from 'react'
import { useTranslation } from 'react-i18next'
import { Button } from '@/components/ui/button'
import { Checkbox } from '@/components/ui/checkbox'
import { Input } from '@/components/ui/input'
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

export function ScheduledTaskDialog({ onClose, task }: Props) {
  const { t } = useTranslation(['settings', 'common'])
  const [name, setName] = useState(task?.name ?? '')
  const [cronExpression, setCronExpression] = useState(task?.cron_expression ?? '')
  const [command, setCommand] = useState(task?.command ?? '')
  const [selectedServerIds, setSelectedServerIds] = useState<string[]>(task?.server_ids ?? [])
  const [timeout, setTimeout] = useState(task?.timeout ?? 300)
  const [retryCount, setRetryCount] = useState(task?.retry_count ?? 0)
  const [retryInterval, setRetryInterval] = useState(task?.retry_interval ?? 60)
  const [cronError, setCronError] = useState('')

  const { data: servers } = useQuery<ServerInfo[]>({
    queryKey: ['servers-list'],
    queryFn: () => api.get<ServerInfo[]>('/api/servers')
  })

  const createMutation = useCreateScheduledTask()
  const updateMutation = useUpdateScheduledTask()

  const isEdit = !!task

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault()

    if (!name.trim()) {
      return
    }
    if (!cronExpression.trim()) {
      return
    }
    if (!command.trim()) {
      return
    }
    if (selectedServerIds.length === 0) {
      return
    }

    if (isEdit) {
      const input: UpdateScheduledTaskInput & { id: string } = {
        id: task.id,
        name: name.trim(),
        cron_expression: cronExpression.trim(),
        command: command.trim(),
        server_ids: selectedServerIds,
        timeout,
        retry_count: retryCount,
        retry_interval: retryInterval
      }
      updateMutation.mutate(input, { onSuccess: onClose })
    } else {
      const input: CreateScheduledTaskInput = {
        name: name.trim(),
        cron_expression: cronExpression.trim(),
        command: command.trim(),
        server_ids: selectedServerIds,
        timeout,
        retry_count: retryCount,
        retry_interval: retryInterval
      }
      createMutation.mutate(input, { onSuccess: onClose })
    }
  }

  const toggleServer = (id: string) => {
    setSelectedServerIds((prev) => (prev.includes(id) ? prev.filter((s) => s !== id) : [...prev, id]))
  }

  const selectAll = () => {
    if (!servers) {
      return
    }
    const execEnabled = servers.filter((server) =>
      getEffectiveCapabilityEnabled(server.effective_capabilities, server.capabilities, CAP_EXEC)
    )
    setSelectedServerIds(selectedServerIds.length === execEnabled.length ? [] : execEnabled.map((s) => s.id))
  }

  const handleCronChange = (value: string) => {
    setCronExpression(value)
    setCronError('')
    if (value.trim()) {
      // Basic validation: must have 5-7 space-separated parts
      const parts = value.trim().split(CRON_SPLIT_RE)
      if (parts.length < 5 || parts.length > 7) {
        setCronError(t('tasks.scheduled.invalid_cron', { defaultValue: 'Invalid cron expression' }))
      }
    }
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
              onChange={(e) => setName(e.target.value)}
              placeholder={t('tasks.scheduled.name_placeholder', { defaultValue: 'e.g. Daily Backup' })}
              required
              value={name}
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
              value={cronExpression}
            />
            {cronError && <p className="mt-1 text-red-500 text-xs">{cronError}</p>}
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
              onChange={(e) => setCommand(e.target.value)}
              placeholder={t('tasks.command_placeholder')}
              required
              rows={3}
              value={command}
            />
          </div>

          <div className="grid grid-cols-3 gap-3">
            <div>
              <label className="mb-1 block font-medium text-sm" htmlFor="task-timeout">
                {t('tasks.timeout')}
              </label>
              <Input
                id="task-timeout"
                min={1}
                onChange={(e) => setTimeout(Number.parseInt(e.target.value, 10) || 300)}
                type="number"
                value={timeout}
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
                onChange={(e) => setRetryCount(Number.parseInt(e.target.value, 10) || 0)}
                type="number"
                value={retryCount}
              />
            </div>
            {retryCount > 0 && (
              <div>
                <label className="mb-1 block font-medium text-sm" htmlFor="task-retry-interval">
                  {t('tasks.scheduled.retry_interval', { defaultValue: 'Retry Interval (s)' })}
                </label>
                <Input
                  id="task-retry-interval"
                  min={1}
                  onChange={(e) => setRetryInterval(Number.parseInt(e.target.value, 10) || 60)}
                  type="number"
                  value={retryInterval}
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
              <div className="max-h-40 overflow-auto">
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
                          checked={selectedServerIds.includes(srv.id)}
                          disabled={!execEnabled}
                          onCheckedChange={() => toggleServer(srv.id)}
                        />
                        {srv.name}
                      </label>
                    )
                  })}
                </div>
              </div>
            )}
          </div>

          <div className="flex justify-end gap-2 pt-2">
            <Button onClick={onClose} type="button" variant="outline">
              {t('common:cancel', { defaultValue: 'Cancel' })}
            </Button>
            <Button
              disabled={
                isPending || !name.trim() || !cronExpression.trim() || !command.trim() || selectedServerIds.length === 0
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

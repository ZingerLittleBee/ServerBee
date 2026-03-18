import { useQuery } from '@tanstack/react-query'
import { Calendar, ChevronDown, ChevronRight, Edit, Pause, Play, Plus, Trash2 } from 'lucide-react'
import { useState } from 'react'
import { useTranslation } from 'react-i18next'
import { Button } from '@/components/ui/button'
import {
  type ScheduledTask,
  type ScheduledTaskResult,
  useDeleteScheduledTask,
  useRunScheduledTask,
  useScheduledTaskResults,
  useScheduledTasks,
  useUpdateScheduledTask
} from '@/hooks/use-scheduled-tasks'
import { api } from '@/lib/api-client'
import { ScheduledTaskDialog } from './scheduled-task-dialog'

interface ServerInfo {
  id: string
  name: string
}

export function ScheduledTaskList() {
  const { t } = useTranslation(['settings', 'common'])
  const [dialogOpen, setDialogOpen] = useState(false)
  const [editingTask, setEditingTask] = useState<ScheduledTask | null>(null)
  const [expandedTaskId, setExpandedTaskId] = useState<string | null>(null)

  const { data: tasks, isLoading } = useScheduledTasks()
  const { data: results } = useScheduledTaskResults(expandedTaskId)
  const deleteMutation = useDeleteScheduledTask()
  const runMutation = useRunScheduledTask()
  const updateMutation = useUpdateScheduledTask()

  const { data: servers } = useQuery<ServerInfo[]>({
    queryKey: ['servers-list'],
    queryFn: () => api.get<ServerInfo[]>('/api/servers')
  })

  const serverNameMap = new Map(servers?.map((s) => [s.id, s.name]) ?? [])

  const handleToggleEnabled = (task: ScheduledTask) => {
    updateMutation.mutate({ id: task.id, enabled: !task.enabled })
  }

  const [deleteConfirmId, setDeleteConfirmId] = useState<string | null>(null)

  const handleDelete = (id: string) => {
    setDeleteConfirmId(id)
  }

  const confirmDelete = () => {
    if (deleteConfirmId) {
      deleteMutation.mutate(deleteConfirmId)
      setDeleteConfirmId(null)
    }
  }

  // Group results by run_id
  const groupedResults = results
    ? Object.entries(
        results.reduce<Record<string, ScheduledTaskResult[]>>((acc, r) => {
          const key = r.run_id ?? 'oneshot'
          if (!acc[key]) {
            acc[key] = []
          }
          acc[key].push(r)
          return acc
        }, {})
      ).sort(([, a], [, b]) => {
        const aTime = a[0]?.finished_at ?? ''
        const bTime = b[0]?.finished_at ?? ''
        return bTime.localeCompare(aTime)
      })
    : []

  const exitCodeColor = (code: number) => {
    if (code === 0) {
      return 'bg-emerald-500/10 text-emerald-600 dark:text-emerald-400'
    }
    if (code === -2) {
      return 'bg-yellow-500/10 text-yellow-600 dark:text-yellow-400'
    }
    if (code === -3 || code === -4) {
      return 'bg-orange-500/10 text-orange-600 dark:text-orange-400'
    }
    return 'bg-red-500/10 text-red-600 dark:text-red-400'
  }

  return (
    <div className="space-y-4">
      <div className="flex items-center justify-between">
        <p className="text-muted-foreground text-sm">
          {t('tasks.scheduled.description', {
            defaultValue: 'Create cron-based recurring tasks with retry and cancellation support.'
          })}
        </p>
        <Button
          onClick={() => {
            setEditingTask(null)
            setDialogOpen(true)
          }}
          size="sm"
        >
          <Plus className="mr-1 size-4" />
          {t('tasks.scheduled.create', { defaultValue: 'Create' })}
        </Button>
      </div>

      {isLoading && (
        <div className="py-8 text-center text-muted-foreground text-sm">
          {t('common:loading', { defaultValue: 'Loading...' })}
        </div>
      )}
      {!isLoading && (!tasks || tasks.length === 0) && (
        <div className="rounded-lg border bg-card py-12 text-center">
          <Calendar className="mx-auto mb-3 size-8 text-muted-foreground" />
          <p className="text-muted-foreground text-sm">
            {t('tasks.scheduled.empty', { defaultValue: 'No scheduled tasks yet' })}
          </p>
        </div>
      )}
      {!isLoading && tasks && tasks.length > 0 && (
        <div className="space-y-2">
          {tasks.map((task) => (
            <div className="rounded-lg border bg-card" key={task.id}>
              {/* Task header */}
              <div className="flex items-center gap-3 px-4 py-3">
                <button
                  className="flex flex-1 items-center gap-3 text-left"
                  onClick={() => setExpandedTaskId(expandedTaskId === task.id ? null : task.id)}
                  type="button"
                >
                  {expandedTaskId === task.id ? (
                    <ChevronDown className="size-4 shrink-0" />
                  ) : (
                    <ChevronRight className="size-4 shrink-0" />
                  )}
                  <div className="min-w-0 flex-1">
                    <div className="flex items-center gap-2">
                      <span className="font-medium text-sm">{task.name ?? task.command}</span>
                      {!task.enabled && (
                        <span className="rounded bg-muted px-1.5 py-0.5 text-muted-foreground text-xs">
                          {t('tasks.scheduled.paused', { defaultValue: 'Paused' })}
                        </span>
                      )}
                    </div>
                    <div className="flex gap-3 text-muted-foreground text-xs">
                      <span className="font-mono">{task.cron_expression}</span>
                      <span>{task.server_ids.length} server(s)</span>
                      {task.next_run_at && (
                        <span>
                          {t('tasks.scheduled.next_run', { defaultValue: 'Next' })}:{' '}
                          {new Date(task.next_run_at).toLocaleString()}
                        </span>
                      )}
                    </div>
                  </div>
                </button>

                <div className="flex items-center gap-1">
                  <Button
                    onClick={() => runMutation.mutate(task.id)}
                    size="icon"
                    title={t('tasks.scheduled.run_now', { defaultValue: 'Run Now' })}
                    variant="ghost"
                  >
                    <Play className="size-4" />
                  </Button>
                  <Button
                    onClick={() => handleToggleEnabled(task)}
                    size="icon"
                    title={
                      task.enabled
                        ? t('tasks.scheduled.pause', { defaultValue: 'Pause' })
                        : t('tasks.scheduled.resume', { defaultValue: 'Resume' })
                    }
                    variant="ghost"
                  >
                    {task.enabled ? <Pause className="size-4" /> : <Play className="size-4 text-emerald-500" />}
                  </Button>
                  <Button
                    onClick={() => {
                      setEditingTask(task)
                      setDialogOpen(true)
                    }}
                    size="icon"
                    title={t('common:edit', { defaultValue: 'Edit' })}
                    variant="ghost"
                  >
                    <Edit className="size-4" />
                  </Button>
                  <Button
                    onClick={() => handleDelete(task.id)}
                    size="icon"
                    title={t('common:delete', { defaultValue: 'Delete' })}
                    variant="ghost"
                  >
                    <Trash2 className="size-4 text-red-500" />
                  </Button>
                </div>
              </div>

              {/* Execution history */}
              {expandedTaskId === task.id && (
                <div className="border-t">
                  {!results || results.length === 0 ? (
                    <p className="px-4 py-4 text-muted-foreground text-sm">
                      {t('tasks.scheduled.no_runs', { defaultValue: 'No execution history' })}
                    </p>
                  ) : (
                    <div className="max-h-80 divide-y overflow-auto">
                      {groupedResults.slice(0, 20).map(([runId, runResults]) => {
                        const allOk = runResults.every((r) => r.exit_code === 0)
                        const failCount = runResults.filter((r) => r.exit_code !== 0 && r.exit_code !== -2).length
                        const triggerTime = runResults[0]?.started_at ?? runResults[0]?.finished_at
                        return (
                          <div className="px-4 py-2" key={runId}>
                            <div className="mb-1 flex items-center gap-2 text-xs">
                              <span className="text-muted-foreground">
                                {triggerTime ? new Date(triggerTime).toLocaleString() : '—'}
                              </span>
                              <span
                                className={`rounded px-1.5 py-0.5 ${allOk ? 'bg-emerald-500/10 text-emerald-600' : 'bg-red-500/10 text-red-600'}`}
                              >
                                {allOk ? 'OK' : `${failCount} failed`}
                              </span>
                              <span className="text-muted-foreground">{runResults.length} server(s)</span>
                            </div>
                            <div className="space-y-1">
                              {runResults.map((r) => (
                                <div className="flex items-center gap-2 text-xs" key={r.id}>
                                  <span className="w-24 truncate font-medium">
                                    {serverNameMap.get(r.server_id) ?? r.server_id}
                                  </span>
                                  <span className={`rounded px-1 py-0.5 ${exitCodeColor(r.exit_code)}`}>
                                    {r.exit_code === 0 ? 'OK' : `exit ${r.exit_code}`}
                                  </span>
                                  {r.attempt > 1 && <span className="text-muted-foreground">attempt {r.attempt}</span>}
                                  {r.output && (
                                    <span className="max-w-xs truncate text-muted-foreground" title={r.output}>
                                      {r.output}
                                    </span>
                                  )}
                                </div>
                              ))}
                            </div>
                          </div>
                        )
                      })}
                    </div>
                  )}
                </div>
              )}
            </div>
          ))}
        </div>
      )}

      {dialogOpen && (
        <ScheduledTaskDialog
          onClose={() => {
            setDialogOpen(false)
            setEditingTask(null)
          }}
          task={editingTask}
        />
      )}

      {deleteConfirmId && (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50">
          <div className="w-full max-w-sm rounded-lg border bg-background p-6 shadow-lg">
            <p className="mb-4 text-sm">
              {t('tasks.scheduled.confirm_delete', { defaultValue: 'Delete this scheduled task?' })}
            </p>
            <div className="flex justify-end gap-2">
              <Button onClick={() => setDeleteConfirmId(null)} size="sm" variant="outline">
                {t('common:cancel', { defaultValue: 'Cancel' })}
              </Button>
              <Button onClick={confirmDelete} size="sm" variant="destructive">
                {t('common:delete', { defaultValue: 'Delete' })}
              </Button>
            </div>
          </div>
        </div>
      )}
    </div>
  )
}

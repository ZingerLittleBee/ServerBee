import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { createFileRoute } from '@tanstack/react-router'
import { Activity, BarChart3, Trash2 } from 'lucide-react'
import { useState } from 'react'
import { useTranslation } from 'react-i18next'
import { toast } from 'sonner'
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
  AlertDialogTrigger
} from '@/components/ui/alert-dialog'
import { Button } from '@/components/ui/button'
import { Skeleton } from '@/components/ui/skeleton'
import { api } from '@/lib/api-client'
import type { PingTask } from '@/lib/api-schema'
import { PingResultsChart } from './ping-results-chart'
import { PingTaskCreateDialog } from './ping-task-create-dialog'
import type { ProbeType, Server } from './ping-task-types'

export const Route = createFileRoute('/_authed/settings/ping-tasks')({
  component: PingTasksPage
})

function PingTasksPage() {
  const { t } = useTranslation(['settings', 'common'])
  const queryClient = useQueryClient()
  const [expandedTaskId, setExpandedTaskId] = useState<string | null>(null)
  const [deleteId, setDeleteId] = useState<string | null>(null)

  const probeTypeLabels: Record<ProbeType, string> = {
    icmp: t('ping.type_icmp'),
    tcp: t('ping.type_tcp'),
    http: t('ping.type_http')
  }

  const { data: tasks, isLoading } = useQuery<PingTask[]>({
    queryKey: ['ping-tasks'],
    queryFn: () => api.get<PingTask[]>('/api/ping-tasks')
  })

  const { data: servers } = useQuery<Server[]>({
    queryKey: ['servers-list'],
    queryFn: () => api.get<Server[]>('/api/servers')
  })

  const deleteMutation = useMutation({
    mutationFn: (id: string) => api.delete(`/api/ping-tasks/${id}`),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['ping-tasks'] }).catch(() => undefined)
      queryClient.invalidateQueries({ queryKey: ['ping-records'] }).catch(() => undefined)
    }
  })

  const toggleMutation = useMutation({
    mutationFn: ({ enabled, id }: { enabled: boolean; id: string }) =>
      api.put<PingTask>(`/api/ping-tasks/${id}`, { enabled }),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['ping-tasks'] }).catch(() => undefined)
      queryClient.invalidateQueries({ queryKey: ['ping-records'] }).catch(() => undefined)
    }
  })

  return (
    <div>
      <div className="max-w-3xl space-y-4">
        <div className="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
          <h2 className="font-semibold text-lg">{t('ping.probe_tasks')}</h2>
          <PingTaskCreateDialog servers={servers ?? []} />
        </div>

        {isLoading && (
          <div className="space-y-2">
            {Array.from({ length: 2 }, (_, i) => (
              <Skeleton className="h-12" key={`skel-${i.toString()}`} />
            ))}
          </div>
        )}
        {!isLoading && (!tasks || tasks.length === 0) && (
          <p className="text-center text-muted-foreground text-sm">{t('ping.no_tasks')}</p>
        )}
        {tasks && tasks.length > 0 && (
          <div className="divide-y rounded-md border">
            {tasks.map((task) => {
              let serverIds: string[] = []
              try {
                serverIds = JSON.parse(task.server_ids_json || '[]') as string[]
              } catch {
                // ignore malformed JSON
              }
              const isExpanded = expandedTaskId === task.id
              return (
                <div key={task.id}>
                  <div className="flex flex-col gap-3 px-4 py-3 sm:flex-row sm:items-center sm:justify-between">
                    <div className="flex items-center gap-3">
                      <Activity className={`size-4 ${task.enabled ? 'text-green-500' : 'text-muted-foreground'}`} />
                      <div>
                        <p className="font-medium text-sm">
                          {task.name}
                          {!task.enabled && (
                            <span className="ml-2 text-muted-foreground text-xs">{t('ping.disabled')}</span>
                          )}
                        </p>
                        <p className="text-muted-foreground text-xs">
                          {probeTypeLabels[task.probe_type as ProbeType] ?? task.probe_type} | {task.target} |{' '}
                          {task.interval}s
                          {serverIds.length > 0
                            ? ` | ${t('ping.server_count', { count: serverIds.length })}`
                            : ` | ${t('ping.all_servers')}`}
                        </p>
                      </div>
                    </div>
                    <div className="flex gap-1">
                      <Button onClick={() => setExpandedTaskId(isExpanded ? null : task.id)} size="sm" variant="ghost">
                        <BarChart3 className="size-3.5" />
                      </Button>
                      <Button
                        disabled={toggleMutation.isPending}
                        onClick={() =>
                          toggleMutation.mutate(
                            { id: task.id, enabled: !task.enabled },
                            {
                              onSuccess: () => {
                                toast.success(
                                  task.enabled
                                    ? t('ping.task_disabled', { defaultValue: 'Ping task disabled' })
                                    : t('ping.task_enabled', { defaultValue: 'Ping task enabled' })
                                )
                              },
                              onError: (err) => {
                                toast.error(
                                  err instanceof Error
                                    ? err.message
                                    : t('ping.task_toggle_failed', { defaultValue: 'Failed to update ping task' })
                                )
                              }
                            }
                          )
                        }
                        size="sm"
                        variant="outline"
                      >
                        {task.enabled ? t('common:disable') : t('common:enable')}
                      </Button>
                      <AlertDialog
                        onOpenChange={(open) => {
                          if (!open) {
                            setDeleteId(null)
                          }
                        }}
                        open={deleteId === task.id}
                      >
                        <AlertDialogTrigger
                          onClick={() => setDeleteId(task.id)}
                          render={
                            <Button
                              aria-label={`Delete task ${task.name}`}
                              disabled={deleteMutation.isPending}
                              size="sm"
                              variant="destructive"
                            />
                          }
                        >
                          <Trash2 className="size-3.5" />
                        </AlertDialogTrigger>
                        <AlertDialogContent>
                          <AlertDialogHeader>
                            <AlertDialogTitle>{t('common:confirm_title')}</AlertDialogTitle>
                            <AlertDialogDescription>{t('common:confirm_delete_message')}</AlertDialogDescription>
                          </AlertDialogHeader>
                          <AlertDialogFooter>
                            <AlertDialogCancel>{t('common:cancel')}</AlertDialogCancel>
                            <AlertDialogAction
                              onClick={() => {
                                deleteMutation.mutate(task.id, {
                                  onSuccess: () => {
                                    toast.success(t('ping.task_deleted', { defaultValue: 'Ping task deleted' }))
                                  },
                                  onError: (err) => {
                                    toast.error(
                                      err instanceof Error
                                        ? err.message
                                        : t('ping.task_delete_failed', { defaultValue: 'Failed to delete ping task' })
                                    )
                                  }
                                })
                                setDeleteId(null)
                              }}
                              variant="destructive"
                            >
                              {t('common:delete')}
                            </AlertDialogAction>
                          </AlertDialogFooter>
                        </AlertDialogContent>
                      </AlertDialog>
                    </div>
                  </div>
                  {isExpanded && (
                    <div className="border-t bg-muted/20 px-4 py-3">
                      <PingResultsChart key={task.id} taskId={task.id} />
                    </div>
                  )}
                </div>
              )
            })}
          </div>
        )}
      </div>
    </div>
  )
}
